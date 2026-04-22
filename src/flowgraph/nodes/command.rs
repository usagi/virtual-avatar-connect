//! `flowgraph.command.match`: 「prefix 付きコマンド文字列」をパースして
//! verb + args に分解する PureNode。V1 の `Command` processor の「どの命令か判定する部分」を
//! Flowgraph ネイティブ化したもの。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec)
//!   - `content` (String): 入力文字列
//!   - `prefix` (String, optional default `/`): コマンド接頭辞
//! - 出力:
//!   - `on_command` (Exec): prefix にマッチしたとき発火
//!   - `on_other` (Exec): マッチしなかったとき発火
//!   - `command` (String): prefix を剥がしたあとの最初のトークン。非コマンド時は空文字
//!   - `args` (List<String>): 残りのトークン。非コマンド時は空リスト
//!   - `original` (String): 元の content 全文
//!
//! ## 設計メモ
//!
//! V1 の `Command` は quit/disable/enable/reload/set の 5 コマンドを内部ハードコードしていたが、
//! Flowgraph ではディスパッチ自体を **ユーザ側のグラフ**（`compare.eq` + `branch` の連鎖 等）に委ねる。
//! この方が拡張も可読性も高く、Pure に保てる。

use crate::flowgraph::node::{
 get_optional_string, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec,
 PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct CommandMatchNode;

impl NodeDescriptor for CommandMatchNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.command.match".into(),
   title: "Command Match".into(),
   category: "command".into(),
   description: Some("prefix 付きコマンド文字列を verb + args にパースして exec を分岐".into()),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("content", "Content", SocketType::String),
    PortSpec::input("prefix", "Prefix", SocketType::String).with_default(SocketValue::String("/".into())),
   ],
   outputs: vec![
    PortSpec::exec_output("on_command", "On Command"),
    PortSpec::exec_output("on_other", "On Other"),
    PortSpec::output("command", "Command", SocketType::String),
    PortSpec::output("args", "Args", SocketType::List(Box::new(SocketType::String))),
    PortSpec::output("original", "Original", SocketType::String),
   ],
   properties: vec![],
  }
 }
}

#[async_trait]
impl PureNode for CommandMatchNode {
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
  let prefix = get_optional_string(inputs, "prefix", "/")?;

  let mut out = NodeOutput::new().set_data("original", SocketValue::String(content.clone()));

  if !prefix.is_empty() && content.starts_with(&prefix) {
   let body = content.trim_start_matches(&prefix);
   let mut tokens = body.split_whitespace();
   let verb = tokens.next().unwrap_or("").to_string();
   let args: Vec<SocketValue> = tokens.map(|t| SocketValue::String(t.to_string())).collect();
   out = out
    .set_data("command", SocketValue::String(verb))
    .set_data("args", SocketValue::List(args))
    .fire_exec("on_command");
  } else {
   out = out
    .set_data("command", SocketValue::String(String::new()))
    .set_data("args", SocketValue::List(vec![]))
    .fire_exec("on_other");
  }
  Ok(out)
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 fn fired() -> ExecFireSet {
  let mut f = ExecFireSet::new();
  f.insert("exec_in");
  f
 }

 #[tokio::test]
 async fn matches_slash_command_and_splits_args() {
  let node = CommandMatchNode;
  let inputs: InputMap = [("content".into(), SocketValue::String("/set foo bar".into()))]
   .into_iter()
   .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired()).await.unwrap();
  assert!(out.fired_exec.contains("on_command"));
  assert!(!out.fired_exec.contains("on_other"));
  assert_eq!(out.data.get("command"), Some(&SocketValue::String("set".into())));
  let args = out.data.get("args").cloned().unwrap();
  assert_eq!(
   args,
   SocketValue::List(vec![SocketValue::String("foo".into()), SocketValue::String("bar".into())])
  );
 }

 #[tokio::test]
 async fn non_command_triggers_on_other() {
  let node = CommandMatchNode;
  let inputs: InputMap = [("content".into(), SocketValue::String("hello world".into()))]
   .into_iter()
   .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired()).await.unwrap();
  assert!(out.fired_exec.contains("on_other"));
  assert!(!out.fired_exec.contains("on_command"));
  assert_eq!(out.data.get("command"), Some(&SocketValue::String("".into())));
  assert_eq!(out.data.get("args"), Some(&SocketValue::List(vec![])));
  assert_eq!(out.data.get("original"), Some(&SocketValue::String("hello world".into())));
 }

 #[tokio::test]
 async fn custom_prefix_works() {
  let node = CommandMatchNode;
  let inputs: InputMap = [
   ("content".into(), SocketValue::String("!quit".into())),
   ("prefix".into(), SocketValue::String("!".into())),
  ]
  .into_iter()
  .collect();
  let out = node.compute(&InputMap::new(), &inputs, &fired()).await.unwrap();
  assert!(out.fired_exec.contains("on_command"));
  assert_eq!(out.data.get("command"), Some(&SocketValue::String("quit".into())));
 }

 #[tokio::test]
 async fn no_exec_firing_is_noop() {
  let node = CommandMatchNode;
  let inputs: InputMap = [("content".into(), SocketValue::String("/x".into()))]
   .into_iter()
   .collect();
  let out = node.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
  assert!(out.fired_exec.is_empty());
  assert!(out.data.is_empty());
 }
}
