//! `flowgraph.table.*`: 汎用 Table 型の I/O と JSON 相互変換（η-2）。
//!
//! 辞書以外の汎用ユースケース（scene registry / Twitch user list / moderation log 等）にも効く
//! primitive 操作ノード群。辞書セマンティクスを知らない純粋な表操作。
//!
//! ## 含まれるノード
//! - `flowgraph.table.from_json` (Pure): `List<Json>` → `Table`（スキーマは先頭 object から推論）
//! - `flowgraph.table.to_json` (Pure): `Table` → `List<Json>`
//! - `flowgraph.table.load_tsv` (Effectful): path → Table（auto / headerful / legacy_loose モード）
//! - `flowgraph.table.write_tsv` (Effectful): Table + path → exec（atomic rename）

use crate::flowgraph::node::{
	get_optional_string, get_required_list, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap,
	NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::flowgraph::table::{ColumnSpec, Row, Table, TableSchema};
use async_trait::async_trait;
use serde_json::Value as JsonValue;

// ---------------------------------------------------------------------
// 共通: SocketValue → JsonValue ヘルパ（ここでも必要）
// ---------------------------------------------------------------------

fn sv_to_json(v: &SocketValue) -> JsonValue {
	match v {
		SocketValue::Bool(b) => JsonValue::Bool(*b),
		SocketValue::Int(i) => JsonValue::Number((*i).into()),
		SocketValue::Float(f) => serde_json::Number::from_f64(*f)
			.map(JsonValue::Number)
			.unwrap_or(JsonValue::Null),
		SocketValue::String(s) => JsonValue::String(s.clone()),
		SocketValue::Json(j) => j.clone(),
		SocketValue::List(xs) => JsonValue::Array(xs.iter().map(sv_to_json).collect()),
		SocketValue::Map(m) => {
			JsonValue::Object(m.iter().map(|(k, v)| (k.clone(), sv_to_json(v))).collect())
		}
		SocketValue::Table(t) => t.to_json_array(),
		// Phase ξ §6.4: table 化境界は value のみ（pass-through）。
		SocketValue::Quantity(q) => serde_json::Number::from_f64(q.value)
			.map(JsonValue::Number)
			.unwrap_or(JsonValue::Null),
	}
}

// ---------------------------------------------------------------------
// flowgraph.table.from_json
// ---------------------------------------------------------------------

pub struct TableFromJsonNode;

impl NodeDescriptor for TableFromJsonNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.table.from_json".into(),
			title: "Table From JSON".into(),
			category: "table".into(),
			description: Some("List<Json> (object の配列) を Table に変換。スキーマは先頭 object から推論".into()),
			inputs: vec![
				PortSpec::input("json", "JSON", SocketType::List(Box::new(SocketType::Json)))
					.with_default(SocketValue::List(vec![])),
			],
			outputs: vec![
				PortSpec::output("table", "Table", SocketType::Table),
				PortSpec::output("row_count", "Row Count", SocketType::Int),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for TableFromJsonNode {
	async fn compute(
		&self,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let list = get_required_list(inputs, "json")?;
		let arr: Vec<JsonValue> = list.iter().map(sv_to_json).collect();
		let table = Table::from_json_array(&arr, None).unwrap_or_else(|_| Table::empty());
		let count = table.len() as i64;
		Ok(NodeOutput::new()
			.set_data("table", SocketValue::Table(table))
			.set_data("row_count", SocketValue::Int(count)))
	}
}

// ---------------------------------------------------------------------
// flowgraph.table.to_json
// ---------------------------------------------------------------------

pub struct TableToJsonNode;

impl NodeDescriptor for TableToJsonNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.table.to_json".into(),
			title: "Table To JSON".into(),
			category: "table".into(),
			description: Some("Table を List<Json> (object の配列) に変換".into()),
			inputs: vec![
				PortSpec::input("table", "Table", SocketType::Table).with_default(SocketValue::Table(Table::empty())),
			],
			outputs: vec![
				PortSpec::output("json", "JSON", SocketType::List(Box::new(SocketType::Json))),
				PortSpec::output("row_count", "Row Count", SocketType::Int),
			],
			properties: vec![],
		}
	}
}

fn get_required_table<'a>(inputs: &'a InputMap, key: &str) -> Result<&'a Table, NodeExecError> {
	let v = inputs.get(key).ok_or_else(|| NodeExecError::MissingRequiredInput(key.into()))?;
	v.as_table().map_err(|_| NodeExecError::TypeMismatch {
		port: key.into(),
		expected: SocketType::Table,
		actual: v.type_of(),
	})
}

#[async_trait]
impl PureNode for TableToJsonNode {
	async fn compute(
		&self,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let table = get_required_table(inputs, "table")?;
		let arr = match table.to_json_array() {
			JsonValue::Array(a) => a,
			_ => Vec::new(),
		};
		let list: Vec<SocketValue> = arr.into_iter().map(SocketValue::Json).collect();
		let count = list.len() as i64;
		Ok(NodeOutput::new()
			.set_data("json", SocketValue::List(list))
			.set_data("row_count", SocketValue::Int(count)))
	}
}

// ---------------------------------------------------------------------
// flowgraph.table.load_tsv
// ---------------------------------------------------------------------

pub struct TableLoadTsvNode;

impl NodeDescriptor for TableLoadTsvNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.table.load_tsv".into(),
			title: "Table Load TSV".into(),
			category: "table".into(),
			description: Some(
				"TSV ファイルを Table に読み込む（auto / headerful / legacy_loose）。Effectful".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("path", "Path", SocketType::String),
				PortSpec::input("mode", "Mode", SocketType::String)
					.with_default(SocketValue::String("auto".into())),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("table", "Table", SocketType::Table),
				PortSpec::output("row_count", "Row Count", SocketType::Int),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for TableLoadTsvNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let path = get_required_string(inputs, "path")?;
		let mode = get_optional_string(inputs, "mode", "auto")?;

		let contents = match tokio::fs::read_to_string(&path).await {
			Ok(s) => s,
			Err(e) => {
				return Ok(err_output_load(format!("read_to_string {path}: {e}")));
			}
		};
		match parse_tsv_with_mode(&contents, &mode) {
			Ok(table) => {
				let count = table.len() as i64;
				Ok(NodeOutput::new()
					.set_data("table", SocketValue::Table(table))
					.set_data("row_count", SocketValue::Int(count))
					.set_data("error", SocketValue::String(String::new()))
					.fire_exec("on_success"))
			}
			Err(e) => Ok(err_output_load(format!("parse: {e}"))),
		}
	}
}

fn err_output_load(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("table", SocketValue::Table(Table::empty()))
		.set_data("row_count", SocketValue::Int(0))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

// ---------------------------------------------------------------------
// flowgraph.table.write_tsv
// ---------------------------------------------------------------------

pub struct TableWriteTsvNode;

impl NodeDescriptor for TableWriteTsvNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.table.write_tsv".into(),
			title: "Table Write TSV".into(),
			category: "table".into(),
			description: Some(
				"Table を TSV ファイルに書き出す（atomic rename）。Effectful".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("table", "Table", SocketType::Table),
				PortSpec::input("path", "Path", SocketType::String),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("bytes_written", "Bytes Written", SocketType::Int),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for TableWriteTsvNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let table = get_required_table(inputs, "table")?.clone();
		let path = get_required_string(inputs, "path")?;
		let serialized = write_tsv_string(&table);
		let bytes = serialized.as_bytes().to_vec();
		let len = bytes.len() as i64;
		let tmp = format!("{path}.tmp");
		if let Err(e) = tokio::fs::write(&tmp, &bytes).await {
			return Ok(err_output_write(format!("write tmp {tmp}: {e}")));
		}
		if let Err(e) = tokio::fs::rename(&tmp, &path).await {
			return Ok(err_output_write(format!("rename {tmp} -> {path}: {e}")));
		}
		Ok(NodeOutput::new()
			.set_data("bytes_written", SocketValue::Int(len))
			.set_data("error", SocketValue::String(String::new()))
			.fire_exec("on_success"))
	}
}

fn err_output_write(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("bytes_written", SocketValue::Int(0))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

// ---------------------------------------------------------------------
// TSV パース / 書き出し（11 カラム辞書スキーマ前提の default 補完を含む）
// ---------------------------------------------------------------------

/// 11 カラム辞書スキーマ（phase-eta-dictionary-unification.md §3.1）。
/// Dictionary 以外の汎用 Table でも、header 行から自前スキーマを作ればよい。
pub const DICTIONARY_COLUMNS: &[&str] = &[
	"source",
	"replacement",
	"kind",
	"priority",
	"is_locked",
	"enabled",
	"by",
	"created_at",
	"expires_at",
	"tags",
	"note",
];

/// 辞書スキーマのデフォルト `ColumnSpec` 配列を作る。
pub fn dictionary_schema() -> TableSchema {
	TableSchema::new(vec![
		ColumnSpec::new("source", SocketType::String),
		ColumnSpec::new("replacement", SocketType::String),
		ColumnSpec::new("kind", SocketType::String),
		ColumnSpec::new("priority", SocketType::Int),
		ColumnSpec::new("is_locked", SocketType::Bool),
		ColumnSpec::new("enabled", SocketType::Bool),
		ColumnSpec::new("by", SocketType::String),
		ColumnSpec::new("created_at", SocketType::String),
		ColumnSpec::new("expires_at", SocketType::String).nullable(),
		ColumnSpec::new("tags", SocketType::String),
		ColumnSpec::new("note", SocketType::String),
	])
}

#[derive(Debug, Clone, thiserror::Error)]
pub enum TsvParseError {
	#[error("TSV パース失敗: {0}")]
	Parse(String),
}

/// TSV 文字列 → Table。
///
/// - `mode = "headerful"`: 1 行目をヘッダとして列名に採用、以降は値の TAB 分割
/// - `mode = "legacy_loose"`: 空白（SPACE/TAB）先頭 2 トークンを `(source, replacement)` に、残余は無視。
///   11 カラムスキーマに固定して読み込む（source/replacement 以外はデフォルト補完）
/// - `mode = "auto"`: 1 行目に `source\t` が含まれる → headerful、そうでなければ legacy_loose
pub fn parse_tsv_with_mode(contents: &str, mode: &str) -> Result<Table, TsvParseError> {
	let mode = match mode {
		"headerful" => "headerful",
		"legacy_loose" => "legacy_loose",
		_ => detect_tsv_mode(contents),
	};
	match mode {
		"headerful" => parse_headerful_tsv(contents),
		_ => parse_legacy_loose(contents),
	}
}

fn detect_tsv_mode(contents: &str) -> &'static str {
	for line in contents.lines() {
		let trimmed = line.trim_start();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		// 1 行目が `source\t` or `source<TAB>...` で始まっていれば headerful 扱い
		if trimmed.starts_with("source\t") || trimmed == "source" {
			return "headerful";
		}
		return "legacy_loose";
	}
	"legacy_loose"
}

fn parse_headerful_tsv(contents: &str) -> Result<Table, TsvParseError> {
	let mut lines = contents
		.lines()
		.filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with('#'));
	let header = lines
		.next()
		.ok_or_else(|| TsvParseError::Parse("空のファイル".into()))?;
	let col_names: Vec<String> = header.split('\t').map(unescape_tsv_field).collect();
	// 既知のカラム名は型を付与、未知カラムは json 扱い
	let columns: Vec<ColumnSpec> = col_names
		.iter()
		.map(|n| ColumnSpec::new(n.clone(), infer_type_for_column(n)).nullable())
		.collect();
	let schema = TableSchema::new(columns);

	let mut rows = Vec::new();
	for (lineno, line) in lines.enumerate() {
		let fields: Vec<String> = line.split('\t').map(unescape_tsv_field).collect();
		if fields.len() > col_names.len() {
			return Err(TsvParseError::Parse(format!(
				"行 {} のカラム数 {} がヘッダ {} を超過",
				lineno + 2,
				fields.len(),
				col_names.len()
			)));
		}
		let mut values = Vec::with_capacity(col_names.len());
		for (i, col) in col_names.iter().enumerate() {
			let raw = fields.get(i).cloned().unwrap_or_default();
			values.push(string_to_json_for_column(col, &raw));
		}
		rows.push(Row::new(values));
	}
	Ok(Table::new(schema, rows))
}

fn parse_legacy_loose(contents: &str) -> Result<Table, TsvParseError> {
	let schema = dictionary_schema();
	let now = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
	let mut rows = Vec::new();
	for line in contents.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		// 先頭 2 トークン（空白 or TAB）を採用
		let mut iter = trimmed.split_whitespace();
		let source = match iter.next() {
			Some(s) => s.to_string(),
			None => continue,
		};
		let replacement = match iter.next() {
			Some(s) => s.to_string(),
			None => continue,
		};
		rows.push(Row::new(vec![
			JsonValue::String(source),
			JsonValue::String(replacement),
			JsonValue::String("literal".into()),
			JsonValue::Number(0.into()),
			JsonValue::Bool(true), // is_locked: ファイル由来は固定扱い
			JsonValue::Bool(true), // enabled
			JsonValue::String("legacy".into()),
			JsonValue::String(now.clone()),
			JsonValue::Null, // expires_at
			JsonValue::String(String::new()),
			JsonValue::String(String::new()),
		]));
	}
	Ok(Table::new(schema, rows))
}

fn infer_type_for_column(name: &str) -> SocketType {
	match name {
		"priority" => SocketType::Int,
		"is_locked" | "enabled" => SocketType::Bool,
		_ => SocketType::String,
	}
}

fn string_to_json_for_column(col: &str, raw: &str) -> JsonValue {
	match col {
		"priority" => raw.parse::<i64>().map(|n| JsonValue::Number(n.into())).unwrap_or(JsonValue::Number(0.into())),
		"is_locked" | "enabled" => match raw.to_ascii_lowercase().as_str() {
			"true" | "1" | "yes" | "y" => JsonValue::Bool(true),
			"false" | "0" | "no" | "n" => JsonValue::Bool(false),
			_ => JsonValue::Bool(col == "enabled"), // 空文字は enabled だけ default true
		},
		"expires_at" => {
			if raw.is_empty() {
				JsonValue::Null
			} else {
				JsonValue::String(raw.to_string())
			}
		}
		_ => JsonValue::String(raw.to_string()),
	}
}

/// Table → TSV 文字列。ヘッダありで出力。値内タブ/改行/バックスラッシュをエスケープ。
pub fn write_tsv_string(table: &Table) -> String {
	let mut out = String::new();
	let schema = table.schema();
	let names: Vec<&str> = schema.column_names().collect();
	out.push_str(&names.iter().map(|n| escape_tsv_field(n)).collect::<Vec<_>>().join("\t"));
	out.push('\n');
	for row in table.rows() {
		let mut cells = Vec::with_capacity(names.len());
		for (i, _col) in names.iter().enumerate() {
			let v = row.get(i).cloned().unwrap_or(JsonValue::Null);
			cells.push(escape_tsv_field(&json_to_tsv_cell(&v)));
		}
		out.push_str(&cells.join("\t"));
		out.push('\n');
	}
	out
}

fn json_to_tsv_cell(v: &JsonValue) -> String {
	match v {
		JsonValue::Null => String::new(),
		JsonValue::Bool(b) => b.to_string(),
		JsonValue::Number(n) => n.to_string(),
		JsonValue::String(s) => s.clone(),
		_ => serde_json::to_string(v).unwrap_or_default(),
	}
}

fn escape_tsv_field(s: &str) -> String {
	let mut out = String::with_capacity(s.len());
	for c in s.chars() {
		match c {
			'\\' => out.push_str(r"\\"),
			'\t' => out.push_str(r"\t"),
			'\n' => out.push_str(r"\n"),
			'\r' => out.push_str(r"\r"),
			c => out.push(c),
		}
	}
	out
}

fn unescape_tsv_field(s: &str) -> String {
	let mut out = String::with_capacity(s.len());
	let mut chars = s.chars().peekable();
	while let Some(c) = chars.next() {
		if c == '\\' {
			match chars.next() {
				Some('\\') => out.push('\\'),
				Some('t') => out.push('\t'),
				Some('n') => out.push('\n'),
				Some('r') => out.push('\r'),
				Some(other) => {
					out.push('\\');
					out.push(other);
				}
				None => out.push('\\'),
			}
		} else {
			out.push(c);
		}
	}
	out
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::InputMap;

	#[tokio::test]
	async fn table_from_json_roundtrip() {
		let node_to = TableFromJsonNode;
		let node_from = TableToJsonNode;
		let src = SocketValue::List(vec![
			SocketValue::Json(serde_json::json!({"a": "x", "b": 1})),
			SocketValue::Json(serde_json::json!({"a": "y", "b": 2})),
		]);
		let mut inputs = InputMap::new();
		inputs.insert("json".into(), src);
		let out = node_to.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		let table_val = out.data.get("table").unwrap().clone();
		assert_eq!(out.data.get("row_count"), Some(&SocketValue::Int(2)));

		let mut inputs2 = InputMap::new();
		inputs2.insert("table".into(), table_val);
		let out2 = node_from.compute(&InputMap::new(), &inputs2, &ExecFireSet::new()).await.unwrap();
		if let Some(SocketValue::List(xs)) = out2.data.get("json") {
			assert_eq!(xs.len(), 2);
		} else {
			panic!("json not a list");
		}
	}

	#[test]
	fn parse_headerful_tsv_basic() {
		let tsv = "source\treplacement\tkind\tpriority\tis_locked\tenabled\tby\tcreated_at\texpires_at\ttags\tnote\n\
		          hello\tgreeting\tliteral\t10\tfalse\ttrue\tuser:a\t2026-04-23T00:00:00Z\t\tcasual\t\n";
		let table = parse_tsv_with_mode(tsv, "headerful").unwrap();
		assert_eq!(table.len(), 1);
		assert_eq!(table.schema().len(), 11);
		let row = &table.rows()[0];
		assert_eq!(row.get(0).and_then(|v| v.as_str()), Some("hello"));
		assert_eq!(row.get(3).and_then(|v| v.as_i64()), Some(10));
		assert_eq!(row.get(4).and_then(|v| v.as_bool()), Some(false));
		assert_eq!(row.get(5).and_then(|v| v.as_bool()), Some(true));
		assert!(row.get(8).unwrap().is_null());
	}

	#[test]
	fn parse_legacy_loose_space_and_tab() {
		// 空白区切り + TAB 区切り混在の legacy ファイル
		let contents = "にんげん 人間\nいかく\t異格\t名詞\t将進酒\n# comment line\n\n";
		let table = parse_tsv_with_mode(contents, "legacy_loose").unwrap();
		assert_eq!(table.len(), 2);
		assert_eq!(table.schema().len(), 11);
		let r0 = &table.rows()[0];
		assert_eq!(r0.get(0).and_then(|v| v.as_str()), Some("にんげん"));
		assert_eq!(r0.get(1).and_then(|v| v.as_str()), Some("人間"));
		assert_eq!(r0.get(4).and_then(|v| v.as_bool()), Some(true)); // is_locked
		let r1 = &table.rows()[1];
		assert_eq!(r1.get(0).and_then(|v| v.as_str()), Some("いかく"));
		assert_eq!(r1.get(1).and_then(|v| v.as_str()), Some("異格"));
	}

	#[test]
	fn auto_detect_chooses_legacy_for_loose_file() {
		let contents = "にんげん 人間\n";
		let table = parse_tsv_with_mode(contents, "auto").unwrap();
		assert_eq!(table.len(), 1);
		assert_eq!(table.schema().len(), 11);
	}

	#[test]
	fn auto_detect_chooses_headerful_when_header_present() {
		let tsv = "source\treplacement\tkind\tpriority\tis_locked\tenabled\tby\tcreated_at\texpires_at\ttags\tnote\n\
		          hi\thello\tliteral\t0\tfalse\ttrue\tme\t2026-04-23T00:00:00Z\t\t\t\n";
		let table = parse_tsv_with_mode(tsv, "auto").unwrap();
		assert_eq!(table.len(), 1);
		assert_eq!(table.schema().columns[0].name, "source");
	}

	#[test]
	fn write_and_reparse_tsv_roundtrip() {
		let tsv = "source\treplacement\tkind\tpriority\tis_locked\tenabled\tby\tcreated_at\texpires_at\ttags\tnote\n\
		          hi\thello\tliteral\t5\tfalse\ttrue\tme\t2026-04-23T00:00:00Z\t\tx,y\tメモ\n";
		let table = parse_tsv_with_mode(tsv, "headerful").unwrap();
		let serialized = write_tsv_string(&table);
		let reparsed = parse_tsv_with_mode(&serialized, "headerful").unwrap();
		assert_eq!(reparsed.rows(), table.rows());
	}

	#[test]
	fn escape_unescape_tsv_field_roundtrip() {
		let orig = "a\tb\nc\\d";
		let esc = escape_tsv_field(orig);
		assert!(!esc.contains('\t'));
		assert!(!esc.contains('\n'));
		assert_eq!(unescape_tsv_field(&esc), orig);
	}
}
