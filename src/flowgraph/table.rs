//! VAC Flowgraph の汎用 Table 型（spec: `docs/roadmap/phase-eta-dictionary-unification.md` §5）。
//!
//! - 表形式データ（Columns × Rows）の primitive として `SocketType::Table` と対になる値型。
//! - 辞書、scene registry、credential store、Twitch ban list、moderation log など
//!   広いユースを下支えする。
//! - Clone は Arc 経由で O(1)。Mutation は `make_mut` による COW で、`version` が monotonically bump する。
//! - Stateful ノード（Dictionary Replace/Match 等）は Arc identity / version / content_hash の 3 段階で
//!   入力変化を判定し、Aho-Corasick / Regex の再コンパイルを回避する。

use crate::flowgraph::socket::SocketType;
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;
use std::sync::{Arc, OnceLock};

type LazyHash = OnceLock<[u8; 32]>;

/// 列定義（列名・型・nullable）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ColumnSpec {
	pub name: String,
	pub ty: SocketType,
	#[serde(default)]
	pub nullable: bool,
}

impl ColumnSpec {
	pub fn new(name: impl Into<String>, ty: SocketType) -> Self {
		Self { name: name.into(), ty, nullable: false }
	}
	pub fn nullable(mut self) -> Self {
		self.nullable = true;
		self
	}
}

/// テーブルのスキーマ（列群）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TableSchema {
	pub columns: Vec<ColumnSpec>,
}

impl TableSchema {
	pub fn empty() -> Self {
		Self { columns: Vec::new() }
	}
	pub fn new(columns: Vec<ColumnSpec>) -> Self {
		Self { columns }
	}
	pub fn column_index(&self, name: &str) -> Option<usize> {
		self.columns.iter().position(|c| c.name == name)
	}
	pub fn column_names(&self) -> impl Iterator<Item = &str> {
		self.columns.iter().map(|c| c.name.as_str())
	}
	pub fn len(&self) -> usize {
		self.columns.len()
	}
	pub fn is_empty(&self) -> bool {
		self.columns.is_empty()
	}
}

/// 行（列順に JSON 値を持つ）。内部表現は `serde_json::Value` で統一する。
///
/// `SocketValue` ではなく JSON を採用する理由:
/// - TSV/CSV/JSONL の read/write と 1 対 1 で往復しやすい
/// - Aho-Corasick / Regex に渡すのも結局 String 化なので、`Value::String` 経由が自然
/// - Schema 型との分離がしやすい（値は 11 カラム辞書以外の自由ユースも受ける）
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Row(pub Vec<JsonValue>);

impl Row {
	pub fn new(values: Vec<JsonValue>) -> Self {
		Self(values)
	}
	pub fn get(&self, idx: usize) -> Option<&JsonValue> {
		self.0.get(idx)
	}
	pub fn len(&self) -> usize {
		self.0.len()
	}
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}
	/// カラム名と値を zip した `serde_json::Map` として取り出す。
	pub fn as_map(&self, schema: &TableSchema) -> serde_json::Map<String, JsonValue> {
		let mut m = serde_json::Map::with_capacity(self.0.len());
		for (col, v) in schema.columns.iter().zip(self.0.iter()) {
			m.insert(col.name.clone(), v.clone());
		}
		m
	}
}

/// Table の内部表現（Arc で包んで共有）。
#[derive(Debug)]
pub struct TableInner {
	pub schema: TableSchema,
	pub rows: Vec<Row>,
	/// Mutation のたびに monotonically 増える。
	pub version: u64,
	/// Lazy 計算する内容ハッシュ。Stateful cache のキー比較で使う。
	/// `make_mut` で新 inner を作ると OnceCell も空で始まる。
	pub content_hash: LazyHash,
}

impl Clone for TableInner {
	fn clone(&self) -> Self {
		Self {
			schema: self.schema.clone(),
			rows: self.rows.clone(),
			version: self.version,
			content_hash: OnceLock::new(),
		}
	}
}

impl TableInner {
	fn new(schema: TableSchema, rows: Vec<Row>) -> Self {
		Self { schema, rows, version: 0, content_hash: OnceLock::new() }
	}
}

/// VAC Flowgraph の汎用 Table 値。
///
/// Clone は Arc 参照カウントの increment のみで O(1)。
/// `make_mut` でコピーが発生する場合は `version` を bump し、content_hash のキャッシュも失効させる。
#[derive(Debug, Clone)]
pub struct Table {
	inner: Arc<TableInner>,
}

impl PartialEq for Table {
	fn eq(&self, other: &Self) -> bool {
		// Arc 同一 or 内容同一
		Arc::ptr_eq(&self.inner, &other.inner)
			|| (self.inner.schema == other.inner.schema && self.inner.rows == other.inner.rows)
	}
}

impl Table {
	pub fn empty() -> Self {
		Self { inner: Arc::new(TableInner::new(TableSchema::empty(), Vec::new())) }
	}

	pub fn new(schema: TableSchema, rows: Vec<Row>) -> Self {
		Self { inner: Arc::new(TableInner::new(schema, rows)) }
	}

	pub fn schema(&self) -> &TableSchema {
		&self.inner.schema
	}

	pub fn rows(&self) -> &[Row] {
		&self.inner.rows
	}

	pub fn len(&self) -> usize {
		self.inner.rows.len()
	}

	pub fn is_empty(&self) -> bool {
		self.inner.rows.is_empty()
	}

	pub fn version(&self) -> u64 {
		self.inner.version
	}

	/// Arc identity ポインタ（Stateful cache の fast path 用）。
	pub fn arc_ptr(&self) -> *const TableInner {
		Arc::as_ptr(&self.inner)
	}

	/// 内容ハッシュを lazy 計算して返す。初回のみ計算、以降はキャッシュ。
	pub fn content_hash(&self) -> &[u8; 32] {
		self.inner.content_hash.get_or_init(|| {
			// schema + rows を canonical JSON に serialize してハッシュ
			let v = serde_json::json!({
				"schema": self.inner.schema,
				"rows": self.inner.rows,
			});
			// canonical 形式: serde_json の default（キーソート無し）で十分（同一構造は同一バイト列）
			let bytes = serde_json::to_vec(&v).unwrap_or_default();
			*blake3::hash(&bytes).as_bytes()
		})
	}

	/// Arc::make_mut 相当の COW mutation。`version` を 1 つ bump、`content_hash` を失効させる。
	pub fn make_mut(&mut self) -> &mut TableInner {
		let inner = Arc::make_mut(&mut self.inner);
		inner.version = inner.version.wrapping_add(1);
		inner.content_hash = OnceLock::new();
		inner
	}

	/// 行を末尾に append（COW）。
	pub fn push_row(&mut self, row: Row) {
		self.make_mut().rows.push(row);
	}

	/// 条件に一致する最新（末尾から走査）1 行を削除。成功なら true。
	pub fn remove_last_where<F: FnMut(&Row) -> bool>(&mut self, mut pred: F) -> bool {
		let idx = self.inner.rows.iter().enumerate().rev().find_map(|(i, r)| pred(r).then_some(i));
		match idx {
			Some(i) => {
				self.make_mut().rows.remove(i);
				true
			}
			None => false,
		}
	}

	/// 条件に一致する全行を削除。削除件数を返す。
	pub fn remove_all_where<F: FnMut(&Row) -> bool>(&mut self, mut pred: F) -> usize {
		let before = self.inner.rows.len();
		let inner = self.make_mut();
		inner.rows.retain(|r| !pred(r));
		before - inner.rows.len()
	}

	/// `List<Map<Json>>` 相当の JSON 配列を構築（書き出し用）。
	pub fn to_json_array(&self) -> JsonValue {
		JsonValue::Array(
			self.inner
				.rows
				.iter()
				.map(|r| JsonValue::Object(r.as_map(&self.inner.schema)))
				.collect(),
		)
	}

	/// 既存の JSON 配列 + 任意スキーマから Table を構築。スキーマ未指定なら先頭 object から推論。
	pub fn from_json_array(
		array: &[JsonValue],
		schema: Option<TableSchema>,
	) -> Result<Self, TableFromJsonError> {
		let schema = match schema {
			Some(s) => s,
			None => infer_schema(array)?,
		};
		let mut rows = Vec::with_capacity(array.len());
		for (i, v) in array.iter().enumerate() {
			let obj = v.as_object().ok_or(TableFromJsonError::RowNotObject { index: i })?;
			let mut values = Vec::with_capacity(schema.columns.len());
			for col in &schema.columns {
				values.push(obj.get(&col.name).cloned().unwrap_or(JsonValue::Null));
			}
			rows.push(Row::new(values));
		}
		Ok(Self::new(schema, rows))
	}
}

fn infer_schema(array: &[JsonValue]) -> Result<TableSchema, TableFromJsonError> {
	// 先頭の object を採用。全 object の union を採るのも考えたが η では最小。
	let first_obj = array
		.iter()
		.find_map(|v| v.as_object())
		.ok_or(TableFromJsonError::CannotInferSchema)?;
	let cols = first_obj
		.keys()
		.map(|k| ColumnSpec::new(k.clone(), SocketType::Json).nullable())
		.collect();
	Ok(TableSchema::new(cols))
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum TableFromJsonError {
	#[error("先頭行が JSON object ではないためスキーマを推論できない")]
	CannotInferSchema,
	#[error("行 {index} が JSON object ではない")]
	RowNotObject { index: usize },
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	fn sample_schema() -> TableSchema {
		TableSchema::new(vec![
			ColumnSpec::new("a", SocketType::String),
			ColumnSpec::new("b", SocketType::Int),
		])
	}

	fn sample_table() -> Table {
		let rows = vec![
			Row::new(vec![JsonValue::String("x".into()), JsonValue::Number(1.into())]),
			Row::new(vec![JsonValue::String("y".into()), JsonValue::Number(2.into())]),
		];
		Table::new(sample_schema(), rows)
	}

	#[test]
	fn clone_shares_arc() {
		let a = sample_table();
		let b = a.clone();
		assert_eq!(a.arc_ptr(), b.arc_ptr());
		assert_eq!(a.version(), 0);
		assert_eq!(b.version(), 0);
	}

	#[test]
	fn make_mut_bumps_version_and_splits_arc() {
		let a = sample_table();
		let b = a.clone();
		let mut c = b.clone();
		c.push_row(Row::new(vec![JsonValue::String("z".into()), JsonValue::Number(3.into())]));
		assert_eq!(a.version(), 0);
		assert_eq!(b.version(), 0);
		assert_eq!(c.version(), 1);
		assert_ne!(a.arc_ptr(), c.arc_ptr());
	}

	#[test]
	fn content_hash_stable_for_same_content() {
		let a = sample_table();
		let b = sample_table();
		// 別 Arc だが内容は同じ
		assert_ne!(a.arc_ptr(), b.arc_ptr());
		assert_eq!(a.content_hash(), b.content_hash());
	}

	#[test]
	fn content_hash_changes_on_mutation() {
		let a = sample_table();
		let h0 = *a.content_hash();
		let mut b = a.clone();
		b.push_row(Row::new(vec![JsonValue::String("z".into()), JsonValue::Number(3.into())]));
		let h1 = *b.content_hash();
		assert_ne!(h0, h1);
	}

	#[test]
	fn remove_last_where_is_undo_ish() {
		let mut a = sample_table();
		a.push_row(Row::new(vec![JsonValue::String("x".into()), JsonValue::Number(99.into())]));
		assert_eq!(a.len(), 3);
		// x の最新 1 件だけ削除
		let removed = a.remove_last_where(|r| r.get(0).and_then(|v| v.as_str()) == Some("x"));
		assert!(removed);
		assert_eq!(a.len(), 2);
		// 残った "x" の b は元の 1
		let last_x = a.rows().iter().find(|r| r.get(0).and_then(|v| v.as_str()) == Some("x")).unwrap();
		assert_eq!(last_x.get(1).and_then(|v| v.as_i64()), Some(1));
	}

	#[test]
	fn remove_all_where_counts() {
		let mut a = sample_table();
		a.push_row(Row::new(vec![JsonValue::String("x".into()), JsonValue::Number(99.into())]));
		let n = a.remove_all_where(|r| r.get(0).and_then(|v| v.as_str()) == Some("x"));
		assert_eq!(n, 2);
		assert_eq!(a.len(), 1);
	}

	#[test]
	fn from_to_json_roundtrip() {
		let a = sample_table();
		let arr = a.to_json_array();
		let parsed =
			Table::from_json_array(arr.as_array().unwrap(), Some(a.schema().clone())).unwrap();
		assert_eq!(parsed.rows(), a.rows());
	}

	#[test]
	fn from_json_infers_schema() {
		let arr = serde_json::json!([{"a": "x", "b": 1}, {"a": "y", "b": 2}]);
		let t = Table::from_json_array(arr.as_array().unwrap(), None).unwrap();
		assert_eq!(t.schema().len(), 2);
		assert_eq!(t.len(), 2);
	}

	#[test]
	fn eq_by_content() {
		let a = sample_table();
		let b = sample_table();
		assert_eq!(a, b); // 別 Arc だが内容同一で eq
	}
}
