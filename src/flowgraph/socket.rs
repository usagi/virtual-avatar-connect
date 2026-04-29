//! VAC Flowgraph の型システム（§2 §2.1〜§2.3）。
//!
//! - `SocketType`: ポート/プロパティが取る静的型
//! - `SocketValue`: ランタイム値
//! - 型表記のパース/整形（`"list<string>"` / `"map<json>"` など）
//! - 暗黙変換は行わない。`as_*()` は型が一致した場合のみ値を返す。

use crate::datetime::DateTime;
use crate::flowgraph::quantity::{parse_unit, Quantity, Unit};
use crate::flowgraph::table::Table;
use crate::motion::MotionFrame;
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use thiserror::Error;

/// VAC Flowgraph のソケット型（入出力ポート・プロパティに共通）。
///
/// `Map` の key 型は常に `String` に固定されている（δ-0 合意, spec §2.1）。
/// value 型のみジェネリックに指定する。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum SocketType {
	Bool,
	Int,
	Float,
	String,
	/// バイナリ payload（LF-5a）。JSON / TOML wire では base64 文字列として表現する。
	Bytes,
	Json,
	List(Box<SocketType>),
	Map(Box<SocketType>),
	Exec,
	/// 汎用表形式データ（`Table = Columns × Rows`、η フェーズ追加）。
	/// 辞書、scene registry、credential store、Twitch user list 等の汎用プリミティブ。
	/// 詳細は `docs/roadmap/phase-eta-dictionary-unification.md` §5 参照。
	Table,
	/// 単位次元を持つ数値（Phase ξ）。`dimensionless` の場合は [`SocketType::Float`] と
	/// 等価だが、明示的に `quantity` ポートを宣言すると unit-aware な経路（`flowgraph.unit.*`
	/// ノード群）と型適合する。ξ-2 時点では dim 制約なしの「任意次元受け」。
	/// 厳密な dim 制約付き variant は ξ-3 以降で追加予定。
	Quantity,
	/// 絶対時刻の boundary 型（Phase π）。内部表現は [`crate::datetime::DateTime`]
	/// (= `jiff::Timestamp` の newtype)。文字列ポートとの暗黙 coerce は
	/// [`coerce_to_type`] で RFC3339 往復によって行われる。
	DateTime,
	/// VMC / OSC ワイヤを解釈したフレーム（Phase M4）。`json` とは
	/// [`MotionFrame`] の JSON オブジェクト形状で双方向に coerce 可能。
	MotionFrame,
}

impl SocketType {
	/// 型が値を運ぶか（`Exec` は発火のみで値を運ばない）。
	pub fn carries_value(&self) -> bool {
		!matches!(self, SocketType::Exec)
	}

	/// 初期値（デフォルト）を返す。`Exec` には存在しない。
	pub fn default_value(&self) -> Option<SocketValue> {
		match self {
			SocketType::Bool => Some(SocketValue::Bool(false)),
			SocketType::Int => Some(SocketValue::Int(0)),
			SocketType::Float => Some(SocketValue::Float(0.0)),
			SocketType::String => Some(SocketValue::String(String::new())),
			SocketType::Bytes => Some(SocketValue::Bytes(Vec::new())),
			SocketType::Json => Some(SocketValue::Json(serde_json::Value::Null)),
			SocketType::List(_) => Some(SocketValue::List(Vec::new())),
			SocketType::Map(_) => Some(SocketValue::Map(BTreeMap::new())),
			SocketType::Exec => None,
			SocketType::Table => Some(SocketValue::Table(Table::empty())),
			SocketType::Quantity => Some(SocketValue::Quantity(Quantity::dimensionless(0.0))),
			SocketType::DateTime => Some(SocketValue::DateTime(DateTime::default())),
			SocketType::MotionFrame => Some(SocketValue::MotionFrame(MotionFrame {
				byte_len: 0,
				osc_messages: Vec::new(),
			})),
		}
	}

	/// 型表記文字列をパース。
	///
	/// - 原始型: `"bool" / "int" / "float" / "string" / "json" / "exec"`
	/// - `"list<T>"` / `"map<T>"` / `"map<string, T>"`（後者互換記法）
	pub fn parse(s: &str) -> Result<Self, TypeParseError> {
		parse_type(s.trim())
	}

	/// エッジ接続の際に 2 つの型が「互換」か判定する。`self` (upstream) が
	/// `other` (downstream) に流し込めるなら `true`。
	///
	/// Phase ξ-3 で追加、ξ-4 で拡張。厳密な構造等価 (`==`) ではなく、以下の暗黙
	/// coerce を許容する:
	///
	/// - `Float ↔ Quantity`: Float は dimensionless Quantity として流せる / 逆も可
	///   （値はランタイムで [`coerce_to_type`] が変換する。非 dimensionless な
	///   Quantity → Float は実行時にエラーを出し、明示 strip を促す）
	/// - `Quantity → String`: [`Quantity`] の Display 実装（`"{value} {unit}"` ないし
	///   dimensionless なら `"{value}"`）で自動文字列化。ξ-4 で `flowgraph.util.log`
	///   や `flowgraph.channel.emit` に Quantity を直接流せるようにするために導入。
	///   逆方向 (`String → Quantity`) は任意文字列を確実に parse できないため非許容。
	/// - `Json ↔ MotionFrame`: M4 フレームの JSON オブジェクト形状での往復（[`coerce_to_type`]）。
	/// - それ以外は `==` と同じ
	///
	/// `List` / `Map` の inner は再帰的に `compatible_with` で判定する。
	pub fn compatible_with(&self, other: &SocketType) -> bool {
		use SocketType::*;
		match (self, other) {
			(Float, Quantity) | (Quantity, Float) => true,
			(Quantity, String) => true,
			// Phase π: String ↔ DateTime は RFC3339 を介して双方向に coerce 可能。
			// parse 失敗はランタイムに [`coerce_to_type`] が [`CoerceError::DateTimeParseError`]
			// として伝搬する（接続時点では型互換とみなす）。
			(String, DateTime) | (DateTime, String) => true,
			(Json, MotionFrame) | (MotionFrame, Json) => true,
			(List(a), List(b)) => a.compatible_with(b),
			(Map(a), Map(b)) => a.compatible_with(b),
			(a, b) => a == b,
		}
	}
}

// ---------------------------------------------------------------------
// 値レベルの暗黙 coerce（Float <-> Quantity）— Phase ξ-3
// ---------------------------------------------------------------------

/// 上流 port の出力値 `value` を下流 port の型 `target` に合わせて暗黙変換する。
///
/// Phase ξ-3 で追加。[`SocketType::compatible_with`] と対で運用し、エッジ接続時は
/// 型互換性チェックが通り、ランタイム値配送時に本関数が呼ばれる。
///
/// - `Float → Quantity`: dimensionless Quantity としてラップ
/// - `Quantity → Float`: dimensionless な場合に限り value を取り出す。非 dimensionless
///   は `Err(CoerceError::NotDimensionless)`（明示的な `flowgraph.unit.strip` を要求）
/// - `Json → MotionFrame`: `serde` で復元。失敗は [`CoerceError::MotionFrameFromJsonError`]
/// - `MotionFrame → Json`: [`MotionFrame::to_json_value`] へ
/// - それ以外で target に既に一致している値はそのまま返す
/// - 型が不一致でかつ上記 coerce に該当しない場合は `Err(CoerceError::TypeMismatch)`
pub fn coerce_to_type(value: SocketValue, target: &SocketType) -> Result<SocketValue, CoerceError> {
	match (value, target) {
		// Float -> Quantity: dimensionless 化
		(SocketValue::Float(f), SocketType::Quantity) => Ok(SocketValue::Quantity(Quantity::dimensionless(f))),
		// Quantity -> Float: dimensionless 限定
		(SocketValue::Quantity(q), SocketType::Float) => {
			if q.is_dimensionless() {
				Ok(SocketValue::Float(q.value))
			} else {
				Err(CoerceError::NotDimensionless { unit: q.unit.canonical() })
			}
		}
		// Quantity -> String: Display impl で "{value} {unit}" へ（Phase xi-4）
		(SocketValue::Quantity(q), SocketType::String) => Ok(SocketValue::String(format!("{}", q))),
		// Phase π: DateTime -> String: RFC3339 (Z suffix, subsecond-preserving) 文字列化。
		// 常に成功する（jiff::Timestamp は任意の値で valid な RFC3339 を出せる）。
		(SocketValue::DateTime(dt), SocketType::String) => Ok(SocketValue::String(dt.to_rfc3339())),
		// Phase π: String -> DateTime: RFC3339 parse。失敗は明示 error を返し、
		// ノード実装側で runtime error に昇格させる（strict stance, Phase ξ D4 と同じ哲学）。
		(SocketValue::String(s), SocketType::DateTime) => {
			DateTime::from_rfc3339(&s)
				.map(SocketValue::DateTime)
				.map_err(|e| CoerceError::DateTimeParseError {
					input: s,
					reason: e.to_string(),
				})
		}
		// List / Map: 要素再帰
		(SocketValue::List(xs), SocketType::List(inner)) => {
			let mut out = Vec::with_capacity(xs.len());
			for x in xs {
				out.push(coerce_to_type(x, inner)?);
			}
			Ok(SocketValue::List(out))
		}
		(SocketValue::Map(m), SocketType::Map(inner)) => {
			let mut out = BTreeMap::new();
			for (k, v) in m {
				out.insert(k, coerce_to_type(v, inner)?);
			}
			Ok(SocketValue::Map(out))
		}
		(SocketValue::Json(j), SocketType::MotionFrame) => serde_json::from_value(j.clone())
			.map(SocketValue::MotionFrame)
			.map_err(|e| CoerceError::MotionFrameFromJsonError { reason: e.to_string() }),
		(SocketValue::MotionFrame(m), SocketType::Json) => Ok(SocketValue::Json(m.to_json_value())),
		// 既に一致しているならそのまま
		(v, t) if v.matches(t) => Ok(v),
		// どれでもなければミスマッチ
		(v, t) => Err(CoerceError::TypeMismatch {
			from: v.type_of(),
			to: t.clone(),
		}),
	}
}

#[derive(Debug, Clone, Error)]
pub enum CoerceError {
	#[error("型不一致: {from} → {to}")]
	TypeMismatch { from: SocketType, to: SocketType },
	#[error("Quantity は非 dimensionless（unit={unit}）、Float へ暗黙変換不可。明示 strip を挟んでください")]
	NotDimensionless { unit: String },
	#[error("String → DateTime 変換失敗: '{input}' ({reason})")]
	DateTimeParseError { input: String, reason: String },
	#[error("JSON → motion_frame 変換失敗: {reason}")]
	MotionFrameFromJsonError { reason: String },
}

impl fmt::Display for SocketType {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			SocketType::Bool => f.write_str("bool"),
			SocketType::Int => f.write_str("int"),
			SocketType::Float => f.write_str("float"),
			SocketType::String => f.write_str("string"),
			SocketType::Bytes => f.write_str("bytes"),
			SocketType::Json => f.write_str("json"),
			SocketType::List(inner) => write!(f, "list<{inner}>"),
			SocketType::Map(inner) => write!(f, "map<{inner}>"),
			SocketType::Exec => f.write_str("exec"),
			SocketType::Table => f.write_str("table"),
			SocketType::Quantity => f.write_str("quantity"),
			SocketType::DateTime => f.write_str("datetime"),
			SocketType::MotionFrame => f.write_str("motion_frame"),
		}
	}
}

impl Serialize for SocketType {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
	where
		S: serde::Serializer,
	{
		serializer.serialize_str(&self.to_string())
	}
}

impl<'de> Deserialize<'de> for SocketType {
	fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
	where
		D: serde::Deserializer<'de>,
	{
		let s = String::deserialize(deserializer)?;
		Self::parse(&s).map_err(serde::de::Error::custom)
	}
}

#[derive(Debug, Clone, Error)]
pub enum TypeParseError {
	#[error("空の型表記")]
	Empty,
	#[error("未知の原始型: '{0}'")]
	UnknownPrimitive(String),
	#[error("'<' に対応する '>' が見つからない: '{0}'")]
	UnbalancedBracket(String),
	#[error("list<T> の要素型が空: '{0}'")]
	EmptyListInner(String),
	#[error("map の value 型が空: '{0}'")]
	EmptyMapInner(String),
	#[error("map の key 型は string のみ許容: '{0}'")]
	InvalidMapKey(String),
}

fn parse_type(s: &str) -> Result<SocketType, TypeParseError> {
	if s.is_empty() {
		return Err(TypeParseError::Empty);
	}
	// 原始型
	match s {
		"bool" => return Ok(SocketType::Bool),
		"int" => return Ok(SocketType::Int),
		"float" => return Ok(SocketType::Float),
		"string" => return Ok(SocketType::String),
		"bytes" => return Ok(SocketType::Bytes),
		"json" => return Ok(SocketType::Json),
		"exec" => return Ok(SocketType::Exec),
		"table" => return Ok(SocketType::Table),
		"quantity" => return Ok(SocketType::Quantity),
		"datetime" => return Ok(SocketType::DateTime),
		"motion_frame" => return Ok(SocketType::MotionFrame),
		_ => {}
	}
	// 複合型: list<T> / map<T> / map<string, T>
	if let Some(inner) = strip_generic(s, "list")? {
		let inner = inner.trim();
		if inner.is_empty() {
			return Err(TypeParseError::EmptyListInner(s.to_string()));
		}
		return Ok(SocketType::List(Box::new(parse_type(inner)?)));
	}
	if let Some(inner) = strip_generic(s, "map")? {
		let inner = inner.trim();
		if inner.is_empty() {
			return Err(TypeParseError::EmptyMapInner(s.to_string()));
		}
		// 互換記法: "string, T" の場合 key を検証して T だけを取り出す。
		if let Some(pos) = find_top_level_comma(inner) {
			let (k, v) = inner.split_at(pos);
			let v = &v[1..];
			let key = k.trim();
			let val = v.trim();
			if key != "string" {
				return Err(TypeParseError::InvalidMapKey(key.to_string()));
			}
			if val.is_empty() {
				return Err(TypeParseError::EmptyMapInner(s.to_string()));
			}
			return Ok(SocketType::Map(Box::new(parse_type(val)?)));
		}
		// "map<T>" 短縮記法
		return Ok(SocketType::Map(Box::new(parse_type(inner)?)));
	}
	Err(TypeParseError::UnknownPrimitive(s.to_string()))
}

fn strip_generic<'a>(s: &'a str, name: &str) -> Result<Option<&'a str>, TypeParseError> {
	if !s.starts_with(name) {
		return Ok(None);
	}
	let rest = &s[name.len()..];
	if !rest.starts_with('<') {
		return Ok(None);
	}
	if !rest.ends_with('>') {
		return Err(TypeParseError::UnbalancedBracket(s.to_string()));
	}
	Ok(Some(&rest[1..rest.len() - 1]))
}

fn find_top_level_comma(s: &str) -> Option<usize> {
	let mut depth = 0usize;
	for (i, c) in s.char_indices() {
		match c {
			'<' => depth += 1,
			'>' => depth = depth.saturating_sub(1),
			',' if depth == 0 => return Some(i),
			_ => {}
		}
	}
	None
}

// ---------------------------------------------------------------------
// SocketValue
// ---------------------------------------------------------------------

/// VAC Flowgraph のランタイム値。暗黙変換なし。
#[derive(Debug, Clone, PartialEq)]
pub enum SocketValue {
	Bool(bool),
	Int(i64),
	Float(f64),
	String(String),
	/// Binary payload。JSON/TOML 表現では base64 string に畳む。
	Bytes(Vec<u8>),
	Json(serde_json::Value),
	List(Vec<SocketValue>),
	/// key は常に String（spec §2.1）。
	Map(BTreeMap<String, SocketValue>),
	/// 汎用表形式データ（η フェーズ追加）。Arc 共有 + COW mutation。
	Table(Table),
	/// 単位次元付き数値（Phase ξ）。`dimensionless` は `Float` と数値的に等価だが、
	/// `flowgraph.unit.*` ノード群の入出力として明示的に unit を伴う経路を形成する。
	Quantity(Quantity),
	/// 絶対時刻（Phase π）。内部は [`DateTime`] = `jiff::Timestamp` の newtype。
	/// 文字列ポートとの暗黙 coerce は RFC3339 経由で双方向に行われる。
	DateTime(DateTime),
	/// M4 OSC フレーム（[`MotionFrame`]）。`json` との coerce でワイヤ JSON と往復。
	MotionFrame(MotionFrame),
	// Exec は値を持たないので variant なし。
}

impl SocketValue {
	/// この値の実体型を返す（`List`/`Map` は要素型を精査して返す。
	/// 空コレクションは要素型を `Json` として扱う）。
	pub fn type_of(&self) -> SocketType {
		match self {
			SocketValue::Bool(_) => SocketType::Bool,
			SocketValue::Int(_) => SocketType::Int,
			SocketValue::Float(_) => SocketType::Float,
			SocketValue::String(_) => SocketType::String,
			SocketValue::Bytes(_) => SocketType::Bytes,
			SocketValue::Json(_) => SocketType::Json,
			SocketValue::List(xs) => {
				let inner = xs.first().map(|v| v.type_of()).unwrap_or(SocketType::Json);
				SocketType::List(Box::new(inner))
			}
			SocketValue::Map(m) => {
				let inner = m.values().next().map(|v| v.type_of()).unwrap_or(SocketType::Json);
				SocketType::Map(Box::new(inner))
			}
			SocketValue::Table(_) => SocketType::Table,
			SocketValue::Quantity(_) => SocketType::Quantity,
			SocketValue::DateTime(_) => SocketType::DateTime,
			SocketValue::MotionFrame(_) => SocketType::MotionFrame,
		}
	}

	pub fn as_bool(&self) -> Result<bool, ValueCastError> {
		match self {
			SocketValue::Bool(v) => Ok(*v),
			_ => Err(ValueCastError::Mismatch {
				expected: "bool",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_i64(&self) -> Result<i64, ValueCastError> {
		match self {
			SocketValue::Int(v) => Ok(*v),
			_ => Err(ValueCastError::Mismatch {
				expected: "int",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_f64(&self) -> Result<f64, ValueCastError> {
		match self {
			SocketValue::Float(v) => Ok(*v),
			_ => Err(ValueCastError::Mismatch {
				expected: "float",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_str(&self) -> Result<&str, ValueCastError> {
		match self {
			SocketValue::String(v) => Ok(v.as_str()),
			_ => Err(ValueCastError::Mismatch {
				expected: "string",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_bytes(&self) -> Result<&[u8], ValueCastError> {
		match self {
			SocketValue::Bytes(v) => Ok(v.as_slice()),
			_ => Err(ValueCastError::Mismatch {
				expected: "bytes",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_json(&self) -> Result<&serde_json::Value, ValueCastError> {
		match self {
			SocketValue::Json(v) => Ok(v),
			_ => Err(ValueCastError::Mismatch {
				expected: "json",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_list(&self) -> Result<&[SocketValue], ValueCastError> {
		match self {
			SocketValue::List(v) => Ok(v.as_slice()),
			_ => Err(ValueCastError::Mismatch {
				expected: "list",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_map(&self) -> Result<&BTreeMap<String, SocketValue>, ValueCastError> {
		match self {
			SocketValue::Map(v) => Ok(v),
			_ => Err(ValueCastError::Mismatch {
				expected: "map",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_table(&self) -> Result<&Table, ValueCastError> {
		match self {
			SocketValue::Table(t) => Ok(t),
			_ => Err(ValueCastError::Mismatch {
				expected: "table",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_quantity(&self) -> Result<&Quantity, ValueCastError> {
		match self {
			SocketValue::Quantity(q) => Ok(q),
			_ => Err(ValueCastError::Mismatch {
				expected: "quantity",
				actual: self.type_of(),
			}),
		}
	}
	pub fn as_datetime(&self) -> Result<&DateTime, ValueCastError> {
		match self {
			SocketValue::DateTime(dt) => Ok(dt),
			_ => Err(ValueCastError::Mismatch {
				expected: "datetime",
				actual: self.type_of(),
			}),
		}
	}

	pub fn as_motion_frame(&self) -> Result<&MotionFrame, ValueCastError> {
		match self {
			SocketValue::MotionFrame(m) => Ok(m),
			_ => Err(ValueCastError::Mismatch {
				expected: "motion_frame",
				actual: self.type_of(),
			}),
		}
	}

	/// 値の型が指定の `SocketType` に適合するかの軽量チェック。
	/// `List`/`Map` の内部型は空の場合はパスとみなす。
	pub fn matches(&self, expected: &SocketType) -> bool {
		match (self, expected) {
			(SocketValue::Bool(_), SocketType::Bool)
			| (SocketValue::Int(_), SocketType::Int)
			| (SocketValue::Float(_), SocketType::Float)
			| (SocketValue::String(_), SocketType::String)
			| (SocketValue::Bytes(_), SocketType::Bytes)
			| (SocketValue::Json(_), SocketType::Json)
			| (SocketValue::Table(_), SocketType::Table)
			| (SocketValue::Quantity(_), SocketType::Quantity)
			| (SocketValue::DateTime(_), SocketType::DateTime)
			| (SocketValue::MotionFrame(_), SocketType::MotionFrame) => true,
			(SocketValue::List(xs), SocketType::List(inner)) => xs.iter().all(|v| v.matches(inner)),
			(SocketValue::Map(m), SocketType::Map(inner)) => m.values().all(|v| v.matches(inner)),
			_ => false,
		}
	}
}

#[derive(Debug, Clone, Error)]
pub enum ValueCastError {
	#[error("型ミスマッチ: expected {expected}, actual {actual}")]
	Mismatch { expected: &'static str, actual: SocketType },
}

// ---------------------------------------------------------------------
// TOML 書式の SocketValue 復元
// ---------------------------------------------------------------------

/// `toml::Value` を `SocketType` に合わせて `SocketValue` に復元する。
/// `from_str` のような一次復元口（プロパティ解釈、リテラル読み込み用）。
pub fn from_toml_value(expected: &SocketType, v: &toml::Value) -> Result<SocketValue, FromTomlError> {
	match (expected, v) {
		(SocketType::Bool, toml::Value::Boolean(b)) => Ok(SocketValue::Bool(*b)),
		(SocketType::Int, toml::Value::Integer(i)) => Ok(SocketValue::Int(*i)),
		(SocketType::Float, toml::Value::Float(f)) => Ok(SocketValue::Float(*f)),
		(SocketType::Float, toml::Value::Integer(i)) => Ok(SocketValue::Float(*i as f64)),
		(SocketType::String, toml::Value::String(s)) => Ok(SocketValue::String(s.clone())),
		(SocketType::Bytes, toml::Value::String(s)) => decode_base64_bytes(s).map(SocketValue::Bytes),
		(SocketType::Json, any) => toml_to_json(any).map(SocketValue::Json),
		(SocketType::MotionFrame, any) => {
			let j = toml_to_json(any)?;
			serde_json::from_value(j)
				.map(SocketValue::MotionFrame)
				.map_err(|e| FromTomlError::Mismatch {
					expected: "motion_frame (byte_len + osc_messages JSON shape)".into(),
					actual: format!("deserialize: {e}"),
				})
		}
		(SocketType::List(inner), toml::Value::Array(arr)) => {
			let mut out = Vec::with_capacity(arr.len());
			for elem in arr {
				out.push(from_toml_value(inner, elem)?);
			}
			Ok(SocketValue::List(out))
		}
		(SocketType::Map(inner), toml::Value::Table(tbl)) => {
			let mut out = BTreeMap::new();
			for (k, v) in tbl {
				out.insert(k.clone(), from_toml_value(inner, v)?);
			}
			Ok(SocketValue::Map(out))
		}
		(SocketType::Quantity, toml::Value::Float(f)) => Ok(SocketValue::Quantity(Quantity::dimensionless(*f))),
		(SocketType::Quantity, toml::Value::Integer(i)) => Ok(SocketValue::Quantity(Quantity::dimensionless(*i as f64))),
		(SocketType::Quantity, toml::Value::String(s)) => {
			parse_quantity_string(s)
				.map(SocketValue::Quantity)
				.map_err(|e| FromTomlError::Mismatch {
					expected: "quantity (\"<value> <unit>\" string form)".into(),
					actual: format!("parse error: {e}"),
				})
		}
		(SocketType::Quantity, toml::Value::Table(tbl)) => quantity_from_toml_table(tbl).map(SocketValue::Quantity),
		// Phase π: DateTime は TOML native Datetime / RFC3339 文字列を受理。
		// naive (tz-less) な TOML Datetime は π-4a 層では `Err`（π-4c 以降、
		// `FlowgraphInstanceConfig.default_timezone` と組み合わせて受容する予定）。
		(SocketType::DateTime, toml::Value::Datetime(dt)) => {
			DateTime::from_rfc3339(&dt.to_string())
				.map(SocketValue::DateTime)
				.map_err(|e| FromTomlError::Mismatch {
					expected: "datetime (RFC3339 with tz offset)".into(),
					actual: format!("toml datetime '{dt}' parse error: {e}"),
				})
		}
		(SocketType::DateTime, toml::Value::String(s)) => {
			DateTime::from_rfc3339(s)
				.map(SocketValue::DateTime)
				.map_err(|e| FromTomlError::Mismatch {
					expected: "datetime (RFC3339 string)".into(),
					actual: format!("parse error on '{s}': {e}"),
				})
		}
		(SocketType::Exec, _) => Err(FromTomlError::ExecHasNoValue),
		(SocketType::Table, toml::Value::Array(arr)) => {
			// TOML 配列から Table を復元（スキーマは先頭 object から推論）
			let mut rows_json = Vec::with_capacity(arr.len());
			for elem in arr {
				rows_json.push(toml_to_json(elem)?);
			}
			Table::from_json_array(&rows_json, None)
				.map(SocketValue::Table)
				.map_err(|e| FromTomlError::Mismatch {
					expected: "table".into(),
					actual: format!("{e}"),
				})
		}
		(expected, actual) => Err(FromTomlError::Mismatch {
			expected: expected.to_string(),
			actual: format!("{actual:?}"),
		}),
	}
}

fn toml_to_json(v: &toml::Value) -> Result<serde_json::Value, FromTomlError> {
	Ok(match v {
		toml::Value::String(s) => serde_json::Value::String(s.clone()),
		toml::Value::Integer(i) => serde_json::Value::Number((*i).into()),
		toml::Value::Float(f) => serde_json::Number::from_f64(*f)
			.map(serde_json::Value::Number)
			.unwrap_or(serde_json::Value::Null),
		toml::Value::Boolean(b) => serde_json::Value::Bool(*b),
		toml::Value::Datetime(dt) => serde_json::Value::String(dt.to_string()),
		toml::Value::Array(arr) => {
			let mut out = Vec::with_capacity(arr.len());
			for elem in arr {
				out.push(toml_to_json(elem)?);
			}
			serde_json::Value::Array(out)
		}
		toml::Value::Table(tbl) => {
			let mut out = serde_json::Map::new();
			for (k, v) in tbl {
				out.insert(k.clone(), toml_to_json(v)?);
			}
			serde_json::Value::Object(out)
		}
	})
}

fn decode_base64_bytes(s: &str) -> Result<Vec<u8>, FromTomlError> {
	base64::engine::general_purpose::STANDARD
		.decode(s.trim())
		.map_err(|e| FromTomlError::Mismatch {
			expected: "bytes (base64 string)".into(),
			actual: format!("base64 decode error: {e}"),
		})
}

#[derive(Debug, Clone, Error)]
pub enum FromTomlError {
	#[error("Exec 型は値を持たない")]
	ExecHasNoValue,
	#[error("TOML 型ミスマッチ: expected {expected}, actual {actual}")]
	Mismatch { expected: String, actual: String },
}

// ---------------------------------------------------------------------
// Quantity literal 解釈ヘルパ（Phase ξ TOML wire format §6.3 案 A+B）
// ---------------------------------------------------------------------

/// `"42.5 m/s^2"` / `"9.8"` / `"1 km"` などの文字列から [`Quantity`] を組み立てる。
///
/// 仕様:
/// - 先頭 token を数値としてパース、残りを単位文字列として [`parse_unit`] に渡す。
/// - 数値のみなら dimensionless。単位のみ（数値なし）は error。
pub fn parse_quantity_string(s: &str) -> Result<Quantity, String> {
	let s = s.trim();
	if s.is_empty() {
		return Err("empty quantity literal".into());
	}
	// 先頭数値を取り出す: whitespace で分割、先頭を f64::from_str で試す。
	// 単位部分にスペースが含まれる（例: "kg m/s^2"）ケースは現状未対応だが、
	// parse_unit が `·` / `*` / space を全て product separator として扱うので
	// 単位部分は本メソッドでは first whitespace で split した残り全体を一塊として渡す。
	let (num_part, unit_part) = match s.split_once(char::is_whitespace) {
		Some((n, u)) => (n.trim(), u.trim()),
		None => (s, ""),
	};
	let value: f64 = num_part.parse().map_err(|e| format!("invalid numeric prefix '{num_part}': {e}"))?;
	if unit_part.is_empty() {
		return Ok(Quantity::dimensionless(value));
	}
	let unit = parse_unit(unit_part).map_err(|e| format!("invalid unit '{unit_part}': {e}"))?;
	Ok(Quantity::of(value, unit))
}

/// TOML inline table `{value = 42.5, unit = "m/s^2"}` から [`Quantity`] を組み立てる。
///
/// `unit` が省略 or 空文字なら dimensionless。`value` は Float / Integer を受理。
fn quantity_from_toml_table(tbl: &toml::map::Map<String, toml::Value>) -> Result<Quantity, FromTomlError> {
	let value = match tbl.get("value") {
		Some(toml::Value::Float(f)) => *f,
		Some(toml::Value::Integer(i)) => *i as f64,
		Some(other) => {
			return Err(FromTomlError::Mismatch {
				expected: "quantity.value (number)".into(),
				actual: format!("{other:?}"),
			});
		}
		None => {
			return Err(FromTomlError::Mismatch {
				expected: "quantity table with 'value' field".into(),
				actual: "missing 'value'".into(),
			});
		}
	};
	let unit: Unit = match tbl.get("unit") {
		Some(toml::Value::String(s)) => {
			let s = s.trim();
			if s.is_empty() {
				Unit::dimensionless()
			} else {
				parse_unit(s).map_err(|e| FromTomlError::Mismatch {
					expected: "valid unit string".into(),
					actual: format!("parse error on '{s}': {e}"),
				})?
			}
		}
		Some(other) => {
			return Err(FromTomlError::Mismatch {
				expected: "quantity.unit (string)".into(),
				actual: format!("{other:?}"),
			});
		}
		None => Unit::dimensionless(),
	};
	Ok(Quantity::of(value, unit))
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_primitive_types() {
		assert_eq!(SocketType::parse("bool").unwrap(), SocketType::Bool);
		assert_eq!(SocketType::parse("int").unwrap(), SocketType::Int);
		assert_eq!(SocketType::parse("float").unwrap(), SocketType::Float);
		assert_eq!(SocketType::parse("string").unwrap(), SocketType::String);
		assert_eq!(SocketType::parse("bytes").unwrap(), SocketType::Bytes);
		assert_eq!(SocketType::parse("json").unwrap(), SocketType::Json);
		assert_eq!(SocketType::parse("exec").unwrap(), SocketType::Exec);
		assert_eq!(SocketType::parse("table").unwrap(), SocketType::Table);
		let mf = SocketType::MotionFrame;
		assert_eq!(mf.to_string(), "motion_frame");
		assert_eq!(SocketType::parse("motion_frame").unwrap(), mf);
	}

	#[test]
	fn bytes_type_roundtrip_and_default() {
		let ty = SocketType::Bytes;
		assert_eq!(ty.to_string(), "bytes");
		assert_eq!(SocketType::parse("bytes").unwrap(), ty);
		assert_eq!(serde_json::to_string(&ty).unwrap(), "\"bytes\"");
		let back: SocketType = serde_json::from_str("\"bytes\"").unwrap();
		assert_eq!(back, ty);
		assert_eq!(ty.default_value(), Some(SocketValue::Bytes(Vec::new())));
	}

	#[test]
	fn table_type_roundtrip_and_default() {
		// display / parse 往復
		let t = SocketType::Table;
		let s = t.to_string();
		assert_eq!(s, "table");
		assert_eq!(SocketType::parse(&s).unwrap(), SocketType::Table);
		// default_value は空 Table
		let dv = SocketType::Table.default_value();
		assert!(matches!(dv, Some(SocketValue::Table(_))));
		// serde
		let j = serde_json::to_string(&SocketType::Table).unwrap();
		assert_eq!(j, "\"table\"");
		let back: SocketType = serde_json::from_str(&j).unwrap();
		assert_eq!(back, SocketType::Table);
	}

	#[test]
	fn parse_list_and_map() {
		assert_eq!(
			SocketType::parse("list<string>").unwrap(),
			SocketType::List(Box::new(SocketType::String))
		);
		assert_eq!(SocketType::parse("map<json>").unwrap(), SocketType::Map(Box::new(SocketType::Json)));
		assert_eq!(
			SocketType::parse("map<string, json>").unwrap(),
			SocketType::Map(Box::new(SocketType::Json))
		);
		assert_eq!(
			SocketType::parse("list<map<string>>").unwrap(),
			SocketType::List(Box::new(SocketType::Map(Box::new(SocketType::String))))
		);
	}

	#[test]
	fn parse_rejects_non_string_map_key() {
		assert!(matches!(
			SocketType::parse("map<int, string>"),
			Err(TypeParseError::InvalidMapKey(_))
		));
	}

	#[test]
	fn parse_rejects_unknown() {
		assert!(matches!(SocketType::parse("unknown"), Err(TypeParseError::UnknownPrimitive(_))));
		assert!(matches!(SocketType::parse("list<>"), Err(TypeParseError::EmptyListInner(_))));
	}

	#[test]
	fn type_display_roundtrip() {
		let t = SocketType::List(Box::new(SocketType::Map(Box::new(SocketType::String))));
		let s = t.to_string();
		assert_eq!(s, "list<map<string>>");
		assert_eq!(SocketType::parse(&s).unwrap(), t);
	}

	#[test]
	fn socket_value_type_of() {
		assert_eq!(SocketValue::Bool(true).type_of(), SocketType::Bool);
		assert_eq!(SocketValue::Int(1).type_of(), SocketType::Int);
		assert_eq!(SocketValue::Bytes(vec![0, 1, 255]).type_of(), SocketType::Bytes);
		let list = SocketValue::List(vec![SocketValue::String("a".into()), SocketValue::String("b".into())]);
		assert_eq!(list.type_of(), SocketType::List(Box::new(SocketType::String)));
	}

	#[test]
	fn socket_value_cast_happy() {
		assert_eq!(SocketValue::Bool(true).as_bool().unwrap(), true);
		assert_eq!(SocketValue::Int(42).as_i64().unwrap(), 42);
		assert_eq!(SocketValue::String("hi".into()).as_str().unwrap(), "hi");
		assert_eq!(SocketValue::Bytes(vec![1, 2, 3]).as_bytes().unwrap(), &[1, 2, 3]);
	}

	#[test]
	fn socket_value_cast_mismatch() {
		assert!(matches!(SocketValue::Int(1).as_bool(), Err(ValueCastError::Mismatch { .. })));
	}

	#[test]
	fn socket_value_matches() {
		let v = SocketValue::List(vec![SocketValue::String("a".into())]);
		assert!(v.matches(&SocketType::List(Box::new(SocketType::String))));
		assert!(!v.matches(&SocketType::List(Box::new(SocketType::Int))));
	}

	#[test]
	fn from_toml_primitive_and_nested() {
		let v = toml::Value::String("hello".into());
		let out = from_toml_value(&SocketType::String, &v).unwrap();
		assert_eq!(out, SocketValue::String("hello".into()));

		let v = toml::Value::String("AAH/".into());
		let out = from_toml_value(&SocketType::Bytes, &v).unwrap();
		assert_eq!(out, SocketValue::Bytes(vec![0, 1, 255]));

		let v = toml::Value::Array(vec![toml::Value::Integer(1), toml::Value::Integer(2), toml::Value::Integer(3)]);
		let out = from_toml_value(&SocketType::List(Box::new(SocketType::Int)), &v).unwrap();
		assert_eq!(
			out,
			SocketValue::List(vec![SocketValue::Int(1), SocketValue::Int(2), SocketValue::Int(3)])
		);

		let mut tbl = toml::map::Map::new();
		tbl.insert("a".into(), toml::Value::Integer(1));
		tbl.insert("b".into(), toml::Value::Integer(2));
		let out = from_toml_value(&SocketType::Map(Box::new(SocketType::Int)), &toml::Value::Table(tbl)).unwrap();
		let SocketValue::Map(m) = out else { panic!("expected map") };
		assert_eq!(m.get("a"), Some(&SocketValue::Int(1)));
		assert_eq!(m.get("b"), Some(&SocketValue::Int(2)));
	}

	#[test]
	fn serde_roundtrip_type() {
		let t = SocketType::Map(Box::new(SocketType::Json));
		let s = serde_json::to_string(&t).unwrap();
		assert_eq!(s, "\"map<json>\"");
		let back: SocketType = serde_json::from_str(&s).unwrap();
		assert_eq!(back, t);
	}

	// ---------------------------------------------------------------------
	// Phase π: DateTime socket type / value
	// ---------------------------------------------------------------------

	#[test]
	fn datetime_type_parse_display_roundtrip() {
		assert_eq!(SocketType::parse("datetime").unwrap(), SocketType::DateTime);
		assert_eq!(SocketType::DateTime.to_string(), "datetime");
		let j = serde_json::to_string(&SocketType::DateTime).unwrap();
		assert_eq!(j, "\"datetime\"");
		let back: SocketType = serde_json::from_str(&j).unwrap();
		assert_eq!(back, SocketType::DateTime);
	}

	#[test]
	fn datetime_type_default_is_unix_epoch() {
		let dv = SocketType::DateTime.default_value().unwrap();
		let SocketValue::DateTime(dt) = dv else {
			panic!("expected DateTime")
		};
		assert_eq!(dt, DateTime::unix_epoch());
	}

	#[test]
	fn datetime_type_carries_value() {
		assert!(SocketType::DateTime.carries_value());
	}

	#[test]
	fn datetime_value_type_of_and_matches() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		let v = SocketValue::DateTime(dt);
		assert_eq!(v.type_of(), SocketType::DateTime);
		assert!(v.matches(&SocketType::DateTime));
		assert!(!v.matches(&SocketType::String));
	}

	#[test]
	fn datetime_as_datetime_happy_and_mismatch() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		let v = SocketValue::DateTime(dt);
		assert_eq!(v.as_datetime().unwrap(), &dt);
		assert!(matches!(SocketValue::Int(0).as_datetime(), Err(ValueCastError::Mismatch { .. })));
	}

	// --- compatible_with ---

	#[test]
	fn datetime_compat_string_bidirectional() {
		assert!(SocketType::String.compatible_with(&SocketType::DateTime));
		assert!(SocketType::DateTime.compatible_with(&SocketType::String));
		assert!(SocketType::DateTime.compatible_with(&SocketType::DateTime));
	}

	#[test]
	fn datetime_compat_rejects_unrelated_types() {
		assert!(!SocketType::DateTime.compatible_with(&SocketType::Int));
		assert!(!SocketType::DateTime.compatible_with(&SocketType::Float));
		assert!(!SocketType::DateTime.compatible_with(&SocketType::Quantity));
		assert!(!SocketType::DateTime.compatible_with(&SocketType::Json));
	}

	#[test]
	fn motion_frame_compat_json_bidirectional() {
		assert!(SocketType::Json.compatible_with(&SocketType::MotionFrame));
		assert!(SocketType::MotionFrame.compatible_with(&SocketType::Json));
		assert!(SocketType::MotionFrame.compatible_with(&SocketType::MotionFrame));
		assert!(!SocketType::MotionFrame.compatible_with(&SocketType::String));
	}

	// --- coerce_to_type ---

	#[test]
	fn coerce_datetime_to_string_is_rfc3339() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56.123Z").unwrap();
		let out = coerce_to_type(SocketValue::DateTime(dt), &SocketType::String).unwrap();
		assert_eq!(out, SocketValue::String("2026-04-24T12:34:56.123Z".into()));
	}

	#[test]
	fn coerce_string_to_datetime_happy() {
		let out = coerce_to_type(SocketValue::String("2026-04-24T21:34:56+09:00".into()), &SocketType::DateTime).unwrap();
		let expected = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		assert_eq!(out, SocketValue::DateTime(expected));
	}

	#[test]
	fn coerce_string_to_datetime_parse_error() {
		let err = coerce_to_type(SocketValue::String("not-a-datetime".into()), &SocketType::DateTime).unwrap_err();
		match err {
			CoerceError::DateTimeParseError { input, .. } => assert_eq!(input, "not-a-datetime"),
			other => panic!("unexpected error variant: {other:?}"),
		}
	}

	#[test]
	fn coerce_string_naive_to_datetime_errs_at_pi_4b_layer() {
		// naive (tz 無し) 文字列は π-4b 層では常に parse error。
		// π-4c で config.default_timezone 経由の opt-in pathway を追加予定。
		let err = coerce_to_type(SocketValue::String("2026-04-24T12:34:56".into()), &SocketType::DateTime).unwrap_err();
		assert!(matches!(err, CoerceError::DateTimeParseError { .. }));
	}

	#[test]
	fn coerce_datetime_identity_passthrough() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		let out = coerce_to_type(SocketValue::DateTime(dt), &SocketType::DateTime).unwrap();
		assert_eq!(out, SocketValue::DateTime(dt));
	}

	#[test]
	fn coerce_datetime_to_int_is_type_mismatch() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		let err = coerce_to_type(SocketValue::DateTime(dt), &SocketType::Int).unwrap_err();
		assert!(matches!(err, CoerceError::TypeMismatch { .. }));
	}

	// --- MotionFrame ↔ Json coerce ---

	#[test]
	fn coerce_json_to_motion_frame_roundtrip() {
		let j = serde_json::json!({
			"byte_len": 3,
			"osc_messages": [{"address": "/t", "args": [1.0]}]
		});
		let mf = coerce_to_type(SocketValue::Json(j.clone()), &SocketType::MotionFrame).unwrap();
		let SocketValue::MotionFrame(m) = mf else {
			panic!("expected MotionFrame");
		};
		assert_eq!(m.byte_len, 3);
		assert_eq!(m.osc_messages.len(), 1);
		let back = coerce_to_type(SocketValue::MotionFrame(m), &SocketType::Json).unwrap();
		let SocketValue::Json(out) = back else {
			panic!("expected Json");
		};
		assert_eq!(out, j);
	}

	#[test]
	fn coerce_json_to_motion_frame_invalid_errors() {
		// JSON 配列ルートは `MotionFrame` オブジェクトとしては不正（serde が別解釈しないことを期待）
		let err = coerce_to_type(
			SocketValue::Json(serde_json::json!({"byte_len": "nan", "osc_messages": []})),
			&SocketType::MotionFrame,
		)
		.unwrap_err();
		assert!(matches!(err, CoerceError::MotionFrameFromJsonError { .. }));
	}

	#[test]
	fn motion_frame_value_type_of_and_as() {
		use crate::motion::{MotionFrame, OscMessageWire};
		let m = MotionFrame {
			byte_len: 0,
			osc_messages: vec![OscMessageWire {
				address: "/a".into(),
				args: vec![],
			}],
		};
		let v = SocketValue::MotionFrame(m.clone());
		assert_eq!(v.type_of(), SocketType::MotionFrame);
		assert!(v.matches(&SocketType::MotionFrame));
		assert_eq!(v.as_motion_frame().unwrap(), &m);
	}

	// --- TOML from_toml_value ---

	#[test]
	fn from_toml_datetime_string_literal() {
		let v = toml::Value::String("2026-04-24T12:34:56Z".into());
		let out = from_toml_value(&SocketType::DateTime, &v).unwrap();
		assert_eq!(out, SocketValue::DateTime(DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap()));
	}

	#[test]
	fn from_toml_datetime_native_offset_datetime() {
		// TOML spec の offset-datetime リテラル。`toml::from_str` で document として parse する。
		let parsed: toml::Table = toml::from_str("ts = 2026-04-24T21:34:56+09:00").unwrap();
		let v = parsed.get("ts").unwrap();
		assert!(matches!(v, toml::Value::Datetime(_)), "expected Datetime, got {v:?}");
		let out = from_toml_value(&SocketType::DateTime, v).unwrap();
		let expected = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		assert_eq!(out, SocketValue::DateTime(expected));
	}

	#[test]
	fn from_toml_datetime_naive_rejected_at_pi_4b_layer() {
		// local-datetime (tz 無し) は π-4b 層では常に reject。π-4c で config 経由で opt-in。
		let parsed: toml::Table = toml::from_str("ts = 2026-04-24T12:34:56").unwrap();
		let v = parsed.get("ts").unwrap();
		assert!(matches!(v, toml::Value::Datetime(_)));
		assert!(from_toml_value(&SocketType::DateTime, v).is_err());
	}
}
