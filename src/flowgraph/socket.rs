//! VAC Flowgraph の型システム（§2 §2.1〜§2.3）。
//!
//! - `SocketType`: ポート/プロパティが取る静的型
//! - `SocketValue`: ランタイム値
//! - 型表記のパース/整形（`"list<string>"` / `"map<json>"` など）
//! - 暗黙変換は行わない。`as_*()` は型が一致した場合のみ値を返す。

use crate::flowgraph::table::Table;
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
 Json,
 List(Box<SocketType>),
 Map(Box<SocketType>),
 Exec,
 /// 汎用表形式データ（`Table = Columns × Rows`、η フェーズ追加）。
 /// 辞書、scene registry、credential store、Twitch user list 等の汎用プリミティブ。
 /// 詳細は `docs/roadmap/phase-eta-dictionary-unification.md` §5 参照。
 Table,
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
   SocketType::Json => Some(SocketValue::Json(serde_json::Value::Null)),
   SocketType::List(_) => Some(SocketValue::List(Vec::new())),
   SocketType::Map(_) => Some(SocketValue::Map(BTreeMap::new())),
   SocketType::Exec => None,
   SocketType::Table => Some(SocketValue::Table(Table::empty())),
  }
 }

 /// 型表記文字列をパース。
 ///
 /// - 原始型: `"bool" / "int" / "float" / "string" / "json" / "exec"`
 /// - `"list<T>"` / `"map<T>"` / `"map<string, T>"`（後者互換記法）
 pub fn parse(s: &str) -> Result<Self, TypeParseError> {
  parse_type(s.trim())
 }
}

impl fmt::Display for SocketType {
 fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
  match self {
   SocketType::Bool => f.write_str("bool"),
   SocketType::Int => f.write_str("int"),
   SocketType::Float => f.write_str("float"),
   SocketType::String => f.write_str("string"),
   SocketType::Json => f.write_str("json"),
   SocketType::List(inner) => write!(f, "list<{inner}>"),
   SocketType::Map(inner) => write!(f, "map<{inner}>"),
   SocketType::Exec => f.write_str("exec"),
   SocketType::Table => f.write_str("table"),
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
  "json" => return Ok(SocketType::Json),
  "exec" => return Ok(SocketType::Exec),
  "table" => return Ok(SocketType::Table),
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
 Json(serde_json::Value),
 List(Vec<SocketValue>),
 /// key は常に String（spec §2.1）。
 Map(BTreeMap<String, SocketValue>),
 /// 汎用表形式データ（η フェーズ追加）。Arc 共有 + COW mutation。
 Table(Table),
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
  }
 }

 pub fn as_bool(&self) -> Result<bool, ValueCastError> {
  match self {
   SocketValue::Bool(v) => Ok(*v),
   _ => Err(ValueCastError::Mismatch { expected: "bool", actual: self.type_of() }),
  }
 }
 pub fn as_i64(&self) -> Result<i64, ValueCastError> {
  match self {
   SocketValue::Int(v) => Ok(*v),
   _ => Err(ValueCastError::Mismatch { expected: "int", actual: self.type_of() }),
  }
 }
 pub fn as_f64(&self) -> Result<f64, ValueCastError> {
  match self {
   SocketValue::Float(v) => Ok(*v),
   _ => Err(ValueCastError::Mismatch { expected: "float", actual: self.type_of() }),
  }
 }
 pub fn as_str(&self) -> Result<&str, ValueCastError> {
  match self {
   SocketValue::String(v) => Ok(v.as_str()),
   _ => Err(ValueCastError::Mismatch { expected: "string", actual: self.type_of() }),
  }
 }
 pub fn as_json(&self) -> Result<&serde_json::Value, ValueCastError> {
  match self {
   SocketValue::Json(v) => Ok(v),
   _ => Err(ValueCastError::Mismatch { expected: "json", actual: self.type_of() }),
  }
 }
 pub fn as_list(&self) -> Result<&[SocketValue], ValueCastError> {
  match self {
   SocketValue::List(v) => Ok(v.as_slice()),
   _ => Err(ValueCastError::Mismatch { expected: "list", actual: self.type_of() }),
  }
 }
 pub fn as_map(&self) -> Result<&BTreeMap<String, SocketValue>, ValueCastError> {
  match self {
   SocketValue::Map(v) => Ok(v),
   _ => Err(ValueCastError::Mismatch { expected: "map", actual: self.type_of() }),
  }
 }
 pub fn as_table(&self) -> Result<&Table, ValueCastError> {
  match self {
   SocketValue::Table(t) => Ok(t),
   _ => Err(ValueCastError::Mismatch { expected: "table", actual: self.type_of() }),
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
   | (SocketValue::Json(_), SocketType::Json)
   | (SocketValue::Table(_), SocketType::Table) => true,
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
  (SocketType::Json, any) => toml_to_json(any).map(SocketValue::Json),
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

#[derive(Debug, Clone, Error)]
pub enum FromTomlError {
 #[error("Exec 型は値を持たない")]
 ExecHasNoValue,
 #[error("TOML 型ミスマッチ: expected {expected}, actual {actual}")]
 Mismatch { expected: String, actual: String },
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
  assert_eq!(SocketType::parse("json").unwrap(), SocketType::Json);
  assert_eq!(SocketType::parse("exec").unwrap(), SocketType::Exec);
  assert_eq!(SocketType::parse("table").unwrap(), SocketType::Table);
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
  assert_eq!(
   SocketType::parse("map<json>").unwrap(),
   SocketType::Map(Box::new(SocketType::Json))
  );
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
  assert!(matches!(SocketType::parse("map<int, string>"), Err(TypeParseError::InvalidMapKey(_))));
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
  let list = SocketValue::List(vec![SocketValue::String("a".into()), SocketValue::String("b".into())]);
  assert_eq!(list.type_of(), SocketType::List(Box::new(SocketType::String)));
 }

 #[test]
 fn socket_value_cast_happy() {
  assert_eq!(SocketValue::Bool(true).as_bool().unwrap(), true);
  assert_eq!(SocketValue::Int(42).as_i64().unwrap(), 42);
  assert_eq!(SocketValue::String("hi".into()).as_str().unwrap(), "hi");
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

  let v = toml::Value::Array(vec![
   toml::Value::Integer(1),
   toml::Value::Integer(2),
   toml::Value::Integer(3),
  ]);
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
}
