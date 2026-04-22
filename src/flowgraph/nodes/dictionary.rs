//! `flowgraph.dictionary.replace` / `flowgraph.dictionary.command`。
//!
//! V1 の `Modify` プロセッサのうち「辞書ベース文字列置換」相当と、
//! `DictionaryCommand` の「学習/忘却構文パーサ＋辞書への反映」相当を担当する PureNode 群。
//!
//! ## 辞書の表現
//!
//! - `dictionary` は `List<Json>`。各要素は `{"to": "replacement", "from": "source"}` 形式。
//! - `to`（置換先）/ `from`（置換対象）はいずれも非空の String。
//! - 内部的には安定ソートされた「to を (long-first / index) で走査しつつ from を置換」を行う。
//! - 将来的に他の記法（CSV / TSV / YAML）を使いたい場合は、`convert` / `json_ops` で整形してから投入すればよい。
//!
//! ## dictionary.command について
//!
//! - 「pure な式解析器＋新しい辞書リストを返す」ことに責任を限定する。
//! - ファイルへの永続化や「どこに保存するか」は、下流の effectful ノード（将来の `file.write_lines` 等）で
//!   ユーザが組む想定。状態保持は `state.latch` で受け、次周回の入力に供給するのが典型パターン。

use crate::flowgraph::node::{
 get_required_list, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec,
 PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};

// ---------------------------------------------------------------------
// 共通: dictionary list の扱い
// ---------------------------------------------------------------------

/// dictionary 要素から `(to, from)` を抽出する。失敗は `NodeExecError::Generic` にたたむ。
fn parse_entry(v: &SocketValue) -> Result<(String, String), NodeExecError> {
 let j = match v {
  SocketValue::Json(j) => j.clone(),
  SocketValue::Map(_) | SocketValue::List(_) => {
   // Map は Json へ直列化しておく
   let s = serde_json::to_value(to_json(v)).map_err(|e| NodeExecError::Generic(e.into()))?;
   s
  }
  _ => {
   return Err(NodeExecError::Generic(anyhow::anyhow!(
    "dictionary entry must be Json object, got {:?}",
    v.type_of()
   )))
  }
 };
 let obj = j.as_object().ok_or_else(|| {
  NodeExecError::Generic(anyhow::anyhow!("dictionary entry must be object, got {:?}", j))
 })?;
 let to = obj
  .get("to")
  .and_then(|v| v.as_str())
  .ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("dictionary entry: 'to' is missing or not a string")))?
  .to_string();
 let from = obj
  .get("from")
  .and_then(|v| v.as_str())
  .ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("dictionary entry: 'from' is missing or not a string")))?
  .to_string();
 if from.is_empty() {
  return Err(NodeExecError::Generic(anyhow::anyhow!("dictionary entry: 'from' must be non-empty")));
 }
 Ok((to, from))
}

fn to_json(v: &SocketValue) -> JsonValue {
 match v {
  SocketValue::Bool(b) => JsonValue::Bool(*b),
  SocketValue::Int(i) => JsonValue::Number((*i).into()),
  SocketValue::Float(f) => {
   serde_json::Number::from_f64(*f).map(JsonValue::Number).unwrap_or(JsonValue::Null)
  }
  SocketValue::String(s) => JsonValue::String(s.clone()),
  SocketValue::Json(j) => j.clone(),
  SocketValue::List(xs) => JsonValue::Array(xs.iter().map(to_json).collect()),
  SocketValue::Map(m) => JsonValue::Object(m.iter().map(|(k, v)| (k.clone(), to_json(v))).collect()),
 }
}

// ---------------------------------------------------------------------
// dictionary.replace
// ---------------------------------------------------------------------

pub struct DictionaryReplaceNode;

impl NodeDescriptor for DictionaryReplaceNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.dictionary.replace".into(),
   title: "Dictionary Replace".into(),
   category: "dictionary".into(),
   description: Some("辞書リスト（{to, from}）で content を逐次置換する（Pure）".into()),
   inputs: vec![
    PortSpec::input("content", "Content", SocketType::String),
    PortSpec::input("dictionary", "Dictionary", SocketType::List(Box::new(SocketType::Json)))
     .with_default(SocketValue::List(vec![])),
   ],
   outputs: vec![PortSpec::output("result", "Result", SocketType::String)],
   properties: vec![],
  }
 }
}

#[async_trait]
impl PureNode for DictionaryReplaceNode {
 async fn compute(
  &self,
  _props: &InputMap,
  inputs: &InputMap,
  _fired_exec: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  let mut content = get_required_string(inputs, "content")?;
  let list = get_required_list(inputs, "dictionary")?;
  for entry in list {
   let (to, from) = parse_entry(entry)?;
   content = content.replace(&from, &to);
  }
  Ok(NodeOutput::new().set_data("result", SocketValue::String(content)))
 }
}

// ---------------------------------------------------------------------
// dictionary.command
// ---------------------------------------------------------------------

pub struct DictionaryCommandNode;

impl NodeDescriptor for DictionaryCommandNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.dictionary.command".into(),
   title: "Dictionary Command".into(),
   category: "dictionary".into(),
   description: Some("学習/忘却構文を解釈し、更新後辞書 + feedback を返す（Pure）".into()),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("content", "Content", SocketType::String),
    PortSpec::input("dictionary", "Dictionary", SocketType::List(Box::new(SocketType::Json)))
     .with_default(SocketValue::List(vec![])),
   ],
   outputs: vec![
    PortSpec::exec_output("on_parsed", "On Parsed"),
    PortSpec::exec_output("on_other", "On Other"),
    PortSpec::output("verb", "Verb", SocketType::String),
    PortSpec::output("source", "Source", SocketType::String),
    PortSpec::output("replacement", "Replacement", SocketType::String),
    PortSpec::output("feedback", "Feedback", SocketType::String),
    PortSpec::output("updated_dictionary", "Updated Dictionary", SocketType::List(Box::new(SocketType::Json))),
    PortSpec::output("removed_count", "Removed Count", SocketType::Int),
    PortSpec::output("already_present", "Already Present", SocketType::Bool),
    PortSpec::output("original", "Original", SocketType::String),
   ],
   properties: vec![],
  }
 }
}

/// 制御文パターン。V1 の `COMMAND_RE` と同じ。
static COMMAND_RE: std::sync::LazyLock<regex::Regex> = std::sync::LazyLock::new(|| {
 regex::Regex::new(r"^\s*(?P<verb>学習|忘却|learn|forget)\s*[(（]\s*(?P<body>[^)）]+?)\s*[)）]\s*$").unwrap()
});

#[derive(Debug, Clone, PartialEq, Eq)]
enum ParsedCommand {
 Learn { source: String, replacement: String },
 ForgetExact { source: String, replacement: String },
 ForgetBySource { source: String },
}

fn split_learn_body(body: &str) -> Option<(String, String)> {
 if let Some(idx) = body.find(":=") {
  let src = body[..idx].trim();
  let repl = body[idx + 2..].trim();
  if !src.is_empty() && !repl.is_empty() {
   return Some((src.to_string(), repl.to_string()));
  }
  return None;
 }
 if let Some(idx) = body.find('=') {
  let src = body[..idx].trim();
  let repl = body[idx + 1..].trim();
  if !src.is_empty() && !repl.is_empty() {
   return Some((src.to_string(), repl.to_string()));
  }
  return None;
 }
 None
}

fn parse_command(text: &str) -> Option<ParsedCommand> {
 let caps = COMMAND_RE.captures(text)?;
 let verb = caps.name("verb")?.as_str();
 let body = caps.name("body")?.as_str().trim();
 if body.is_empty() {
  return None;
 }
 match verb {
  "学習" | "learn" => {
   let (src, repl) = split_learn_body(body)?;
   Some(ParsedCommand::Learn { source: src, replacement: repl })
  }
  "忘却" | "forget" => {
   if let Some((src, repl)) = split_learn_body(body) {
    Some(ParsedCommand::ForgetExact { source: src, replacement: repl })
   } else {
    if body.contains(char::is_whitespace) {
     return None;
    }
    Some(ParsedCommand::ForgetBySource { source: body.to_string() })
   }
  }
  _ => None,
 }
}

fn entry_equals(entry: &JsonValue, to: &str, from: &str) -> bool {
 entry.as_object().is_some_and(|o| {
  o.get("to").and_then(JsonValue::as_str) == Some(to) && o.get("from").and_then(JsonValue::as_str) == Some(from)
 })
}

fn entry_from_is(entry: &JsonValue, from: &str) -> bool {
 entry.as_object().is_some_and(|o| o.get("from").and_then(JsonValue::as_str) == Some(from))
}

#[async_trait]
impl PureNode for DictionaryCommandNode {
 async fn compute(
  &self,
  _props: &InputMap,
  inputs: &InputMap,
  fired_exec: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  if !fired_exec.contains("exec_in") {
   return Ok(NodeOutput::new());
  }
  let content = get_required_string(inputs, "content")?;
  let dictionary = get_required_list(inputs, "dictionary")?;

  // 現在の辞書を Json List へ正規化して保持（compute は pure なのでコピー）
  let mut dict_json: Vec<JsonValue> = dictionary.iter().map(to_json).collect();

  let mut out = NodeOutput::new().set_data("original", SocketValue::String(content.clone()));

  let parsed = parse_command(content.trim());
  let Some(cmd) = parsed else {
   // コマンド外: 辞書そのまま通す
   out = out
    .set_data("verb", SocketValue::String(String::new()))
    .set_data("source", SocketValue::String(String::new()))
    .set_data("replacement", SocketValue::String(String::new()))
    .set_data("feedback", SocketValue::String(String::new()))
    .set_data(
     "updated_dictionary",
     SocketValue::List(dict_json.into_iter().map(SocketValue::Json).collect()),
    )
    .set_data("removed_count", SocketValue::Int(0))
    .set_data("already_present", SocketValue::Bool(false))
    .fire_exec("on_other");
   return Ok(out);
  };

  let (verb_str, source, replacement, feedback, removed_count, already_present) = match cmd {
   ParsedCommand::Learn { source, replacement } => {
    let exists = dict_json.iter().any(|e| entry_equals(e, &replacement, &source));
    if exists {
     let fb = format!(
      "既に学習済み: {source} → {replacement}（辞書数={}）",
      dict_json.len()
     );
     ("learn".to_string(), source, replacement, fb, 0_i64, true)
    } else {
     dict_json.push(json!({"to": replacement, "from": source}));
     let fb = format!(
      "学習しました: {source} → {replacement}（辞書数={}）",
      dict_json.len()
     );
     ("learn".to_string(), source, replacement, fb, 0_i64, false)
    }
   }
   ParsedCommand::ForgetExact { source, replacement } => {
    let before = dict_json.len();
    dict_json.retain(|e| !entry_equals(e, &replacement, &source));
    let removed = before - dict_json.len();
    let fb = if removed == 0 {
     format!("忘却対象なし: {source} → {replacement}")
    } else {
     format!(
      "忘却しました: {source} → {replacement}（{removed} 件削除、辞書数={}）",
      dict_json.len()
     )
    };
    ("forget_exact".to_string(), source, replacement, fb, removed as i64, false)
   }
   ParsedCommand::ForgetBySource { source } => {
    let before = dict_json.len();
    dict_json.retain(|e| !entry_from_is(e, &source));
    let removed = before - dict_json.len();
    let fb = if removed == 0 {
     format!("忘却対象なし: {source}")
    } else {
     format!("忘却しました: {source}（{removed} 件削除、辞書数={}）", dict_json.len())
    };
    ("forget_by_source".to_string(), source, String::new(), fb, removed as i64, false)
   }
  };

  out = out
   .set_data("verb", SocketValue::String(verb_str))
   .set_data("source", SocketValue::String(source))
   .set_data("replacement", SocketValue::String(replacement))
   .set_data("feedback", SocketValue::String(feedback))
   .set_data(
    "updated_dictionary",
    SocketValue::List(dict_json.into_iter().map(SocketValue::Json).collect()),
   )
   .set_data("removed_count", SocketValue::Int(removed_count))
   .set_data("already_present", SocketValue::Bool(already_present))
   .fire_exec("on_parsed");
  Ok(out)
 }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
 use super::*;

 fn fired(port: &str) -> ExecFireSet {
  let mut f = ExecFireSet::new();
  f.insert(port);
  f
 }

 fn entry(to: &str, from: &str) -> SocketValue {
  SocketValue::Json(json!({"to": to, "from": from}))
 }

 // ----- dictionary.replace -----

 #[tokio::test]
 async fn replace_applies_all_entries() {
  let node = DictionaryReplaceNode;
  let dict = SocketValue::List(vec![
   entry("ドクターウサギ", "Dr.USAGI"),
   entry("Kawaii", "かわいい"),
  ]);
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("Dr.USAGI はかわいい".into())),
   ("dictionary".into(), dict),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::String("ドクターウサギ はKawaii".into())));
 }

 #[tokio::test]
 async fn replace_with_empty_dictionary_is_identity() {
  let node = DictionaryReplaceNode;
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("hello".into())),
   ("dictionary".into(), SocketValue::List(vec![])),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert_eq!(out.data.get("result"), Some(&SocketValue::String("hello".into())));
 }

 #[tokio::test]
 async fn replace_errors_on_malformed_entry() {
  let node = DictionaryReplaceNode;
  // `from` 欠落
  let bad = SocketValue::Json(json!({"to": "X"}));
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("hi".into())),
   ("dictionary".into(), SocketValue::List(vec![bad])),
  ]
  .into_iter()
  .collect();
  let err = node.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap_err();
  assert!(matches!(err, NodeExecError::Generic(_)));
 }

 // ----- dictionary.command -----

 #[tokio::test]
 async fn command_learn_appends_entry() {
  let node = DictionaryCommandNode;
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("学習(Dr.USAGI:=ドクターウサギ)".into())),
   ("dictionary".into(), SocketValue::List(vec![])),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
  assert!(out.fired_exec.contains("on_parsed"));
  assert_eq!(out.data.get("verb"), Some(&SocketValue::String("learn".into())));
  assert_eq!(out.data.get("already_present"), Some(&SocketValue::Bool(false)));
  let dict = out.data.get("updated_dictionary").unwrap();
  match dict {
   SocketValue::List(xs) => {
    assert_eq!(xs.len(), 1);
    match &xs[0] {
     SocketValue::Json(j) => {
      assert_eq!(j["to"], "ドクターウサギ");
      assert_eq!(j["from"], "Dr.USAGI");
     }
     _ => panic!("expected Json"),
    }
   }
   _ => panic!("expected List"),
  }
 }

 #[tokio::test]
 async fn command_learn_is_idempotent() {
  let node = DictionaryCommandNode;
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("学習(Dr.USAGI:=ドクターウサギ)".into())),
   (
    "dictionary".into(),
    SocketValue::List(vec![entry("ドクターウサギ", "Dr.USAGI")]),
   ),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
  assert_eq!(out.data.get("already_present"), Some(&SocketValue::Bool(true)));
  // 辞書のサイズは変わらない
  match out.data.get("updated_dictionary").unwrap() {
   SocketValue::List(xs) => assert_eq!(xs.len(), 1),
   _ => panic!(),
  }
 }

 #[tokio::test]
 async fn command_forget_exact_removes_matching() {
  let node = DictionaryCommandNode;
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("忘却(Dr.USAGI:=ドクターウサギ)".into())),
   (
    "dictionary".into(),
    SocketValue::List(vec![entry("ドクターウサギ", "Dr.USAGI"), entry("X", "Y")]),
   ),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
  assert_eq!(out.data.get("removed_count"), Some(&SocketValue::Int(1)));
  match out.data.get("updated_dictionary").unwrap() {
   SocketValue::List(xs) => {
    assert_eq!(xs.len(), 1);
    match &xs[0] {
     SocketValue::Json(j) => assert_eq!(j["from"], "Y"),
     _ => panic!(),
    }
   }
   _ => panic!(),
  }
 }

 #[tokio::test]
 async fn command_forget_by_source_removes_all_matching_from() {
  let node = DictionaryCommandNode;
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("忘却(Dr.USAGI)".into())),
   (
    "dictionary".into(),
    SocketValue::List(vec![
     entry("Foo", "Dr.USAGI"),
     entry("Bar", "Dr.USAGI"),
     entry("X", "Y"),
    ]),
   ),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
  assert_eq!(out.data.get("removed_count"), Some(&SocketValue::Int(2)));
  match out.data.get("updated_dictionary").unwrap() {
   SocketValue::List(xs) => assert_eq!(xs.len(), 1),
   _ => panic!(),
  }
 }

 #[tokio::test]
 async fn command_non_command_fires_on_other() {
  let node = DictionaryCommandNode;
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("just chatting".into())),
   ("dictionary".into(), SocketValue::List(vec![entry("x", "y")])),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
  assert!(out.fired_exec.contains("on_other"));
  assert!(!out.fired_exec.contains("on_parsed"));
  // 辞書は維持
  match out.data.get("updated_dictionary").unwrap() {
   SocketValue::List(xs) => assert_eq!(xs.len(), 1),
   _ => panic!(),
  }
 }
}
