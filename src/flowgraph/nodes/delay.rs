//! `flowgraph.util.delay`: exec_in 発火で data をキャプチャして async sleep 後に
//! 自己 trigger 経由で exec_out を発火する StatefulNode。
//!
//! ## 設計メモ
//!
//! - blocking せずに他の exec チェーンを止めない。
//! - 複数の並行遅延に対応（pending 値を `HashMap<id, SocketValue>` で保持）。
//! - 時系列でバラバラに完了しても、id で正しい値を取り出せる。
//! - 1-shot `execute()` モード（`ctx.trigger == None`）で起動された場合は
//!   timer を spawn せず、何も起きない（安全側の no-op）。
//!
//! ## ポート仕様
//!
//! - 入力:
//!   - `exec_in` (Exec): ユーザ入口
//!   - `value` (Json): 遅延させる任意の値
//!   - `delay_ms` (Int): 遅延ミリ秒（optional、default 1000）
//!   - `__resume__` (Exec, internal): engine 専用の再開入口
//!   - `__pending_id__` (Int, internal, optional): 再開時に TriggerEvent の override で流入
//! - 出力:
//!   - `exec_out` (Exec): 遅延完了時に発火
//!   - `value` (Json): キャプチャされた値をそのまま流す

use crate::flowgraph::node::{
 get_optional_int, get_required_int, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
 StatefulCtx, StatefulNode, TriggerEvent,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use std::any::Any;
use std::collections::HashMap;
use std::time::Duration;

pub struct DelayNode;

#[derive(Default)]
pub struct DelayState {
 next_id: u64,
 pending: HashMap<u64, SocketValue>,
}

impl NodeDescriptor for DelayNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.util.delay".into(),
   title: "Delay".into(),
   category: "util".into(),
   description: Some("exec_in 発火で value を保持し、delay_ms 後に exec_out を発火".into()),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("value", "Value", SocketType::Json)
     .with_default(SocketValue::Json(serde_json::Value::Null)),
    PortSpec::input("delay_ms", "Delay (ms)", SocketType::Int).with_default(SocketValue::Int(1000)),
    PortSpec::exec_input("__resume__", "(internal)"),
    PortSpec::input("__pending_id__", "(internal)", SocketType::Int).with_default(SocketValue::Int(-1)),
   ],
   outputs: vec![
    PortSpec::exec_output("exec_out", "Exec Out"),
    PortSpec::output("value", "Value", SocketType::Json),
   ],
   properties: vec![],
  }
 }
}

#[async_trait]
impl StatefulNode for DelayNode {
 fn init_state(&self) -> Box<dyn Any + Send> {
  Box::new(DelayState::default())
 }

 async fn compute(
  &self,
  state: &mut (dyn Any + Send),
  _props: &InputMap,
  inputs: &InputMap,
  fired_exec: &crate::flowgraph::node::ExecFireSet,
  ctx: &StatefulCtx<'_>,
 ) -> Result<NodeOutput, NodeExecError> {
  let state = state.downcast_mut::<DelayState>().expect("DelayState");

  // --- ケース 1: ユーザ発火 (exec_in) ---
  if fired_exec.contains("exec_in") {
   let value = inputs
    .get("value")
    .cloned()
    .unwrap_or(SocketValue::Json(serde_json::Value::Null));
   let delay_ms = get_optional_int(inputs, "delay_ms", 1000)?.max(0) as u64;

   let id = state.next_id;
   state.next_id = state.next_id.wrapping_add(1);
   state.pending.insert(id, value);

   // `ctx.trigger` が無い場合（1-shot 実行）は spawn しない → no-op
   if let Some(handle) = ctx.trigger {
    let handle = handle.clone();
    let node_id = ctx.node_id.to_string();
    tokio::spawn(async move {
     tokio::time::sleep(Duration::from_millis(delay_ms)).await;
     let event = TriggerEvent::new(node_id)
      .with_exec("__resume__")
      .with_override("__pending_id__", SocketValue::Int(id as i64));
     let _ = handle.send(event);
    });
   }
   return Ok(NodeOutput::new());
  }

  // --- ケース 2: engine 発火 (__resume__) ---
  if fired_exec.contains("__resume__") {
   let id = get_required_int(inputs, "__pending_id__")? as u64;
   let value = state.pending.remove(&id).ok_or_else(|| {
    NodeExecError::Generic(anyhow::anyhow!(
     "Delay resume: pending id {} not found (already consumed?)",
     id
    ))
   })?;
   return Ok(NodeOutput::new().set_data("value", value).fire_exec("exec_out"));
  }

  Ok(NodeOutput::new())
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 use crate::flowgraph::engine::{FlowgraphBuilder, PortRef};
 use crate::flowgraph::node::{ExecCtx, NodeImpl};
 use crate::flowgraph::nodes::flow::SequenceNode;
 use crate::flowgraph::nodes::log::LogNode;
 use std::sync::Arc;
 use std::time::Duration;

 #[tokio::test]
 async fn delay_on_one_shot_execute_does_not_fire_out() {
  // 1-shot では timer が spawn されない → exec_out は発火しない
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(1))), InputMap::new());
  b.add_node("delay", NodeImpl::stateful(Arc::new(DelayNode)), InputMap::new());
  b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
  b.connect_exec(PortRef::new("seq", "exec_1"), PortRef::new("delay", "exec_in"));
  b.connect_exec(PortRef::new("delay", "exec_out"), PortRef::new("log", "exec_in"));

  // value/delay_ms はデフォルト使用
  // Log の value 入力は未接続 → default が必要。Log のスペックには default なし。
  // そこでダミー文字列ソースを繋ぐ。
  use crate::flowgraph::nodes::literal::StringLiteralNode;
  b.add_node(
   "msg",
   NodeImpl::pure(Arc::new(StringLiteralNode)),
   [("value".to_string(), SocketValue::String("x".into()))].into_iter().collect(),
  );
  b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));

  let mut prog = b.build().expect("build");
  let mut ctx = ExecCtx::default();
  prog.execute(&mut ctx).await.expect("execute");
  assert!(ctx.trace.is_empty(), "1-shot モードでは delay 以降は発火しない");
 }

 #[tokio::test]
 async fn delay_fires_after_sleep_via_run_forever() {
  let mut b = FlowgraphBuilder::new();
  b.add_node("seq", NodeImpl::pure(Arc::new(SequenceNode::new(1))), InputMap::new());
  b.add_node("delay", NodeImpl::stateful(Arc::new(DelayNode)), InputMap::new());
  b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());

  use crate::flowgraph::nodes::literal::{IntLiteralNode, StringLiteralNode};
  b.add_node(
   "dur",
   NodeImpl::pure(Arc::new(IntLiteralNode)),
   [("value".to_string(), SocketValue::Int(30))].into_iter().collect(),
  );
  b.add_node(
   "msg",
   NodeImpl::pure(Arc::new(StringLiteralNode)),
   [("value".to_string(), SocketValue::String("awake".into()))].into_iter().collect(),
  );

  b.connect_exec(PortRef::new("seq", "exec_1"), PortRef::new("delay", "exec_in"));
  b.connect(PortRef::new("dur", "value"), PortRef::new("delay", "delay_ms"));
  b.connect_exec(PortRef::new("delay", "exec_out"), PortRef::new("log", "exec_in"));
  b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));

  let mut prog = b.build().expect("build");
  let mut ctx = ExecCtx::default();
  let shutdown = tokio::time::sleep(Duration::from_millis(200));
  prog.run_forever(&mut ctx, shutdown).await.expect("run_forever");
  assert_eq!(ctx.trace.len(), 1, "delay 完了で log が 1 度発火");
  assert!(ctx.trace[0].contains("awake"));
 }

 #[tokio::test]
 async fn delay_handles_multiple_overlapping_pending() {
  // 異なる delay_ms で 2 回呼ばれる: Sequence(2) → Delay
  // Sequence の 2 分岐がそれぞれ Delay を exec_in にぶつけるが、
  // DAG 的には "1 つの Delay ノード" に 2 本の exec 入力エッジは張れない（single input）。
  // なのでこのテストでは DelayNode::compute を直接叩いて内部挙動を検証。
  let node = DelayNode;
  let mut state: Box<dyn Any + Send> = node.init_state();

  // 1 回目: value=A, delay=100
  let inputs1: InputMap = [
   ("value".to_string(), SocketValue::Json(serde_json::json!("A"))),
   ("delay_ms".to_string(), SocketValue::Int(100)),
  ]
  .into_iter()
  .collect();
  let mut fired = crate::flowgraph::node::ExecFireSet::new();
  fired.insert("exec_in");
  let sctx = StatefulCtx { node_id: "d", trigger: None };
  let out1 = node
   .compute(state.as_mut(), &InputMap::new(), &inputs1, &fired, &sctx)
   .await
   .unwrap();
  assert!(out1.fired_exec.is_empty(), "exec_in 時点では exec_out 発火なし");

  // 2 回目: value=B, delay=50
  let inputs2: InputMap = [
   ("value".to_string(), SocketValue::Json(serde_json::json!("B"))),
   ("delay_ms".to_string(), SocketValue::Int(50)),
  ]
  .into_iter()
  .collect();
  let out2 = node
   .compute(state.as_mut(), &InputMap::new(), &inputs2, &fired, &sctx)
   .await
   .unwrap();
  assert!(out2.fired_exec.is_empty());

  // 再開 (id=1 が先) で B が出る
  let mut resume_fired = crate::flowgraph::node::ExecFireSet::new();
  resume_fired.insert("__resume__");
  let inputs_r: InputMap = [("__pending_id__".to_string(), SocketValue::Int(1))].into_iter().collect();
  let out_r1 = node
   .compute(state.as_mut(), &InputMap::new(), &inputs_r, &resume_fired, &sctx)
   .await
   .unwrap();
  assert!(out_r1.fired_exec.contains("exec_out"));
  assert_eq!(out_r1.data.get("value").cloned(), Some(SocketValue::Json(serde_json::json!("B"))));

  // 次に id=0 で A
  let inputs_r0: InputMap = [("__pending_id__".to_string(), SocketValue::Int(0))].into_iter().collect();
  let out_r0 = node
   .compute(state.as_mut(), &InputMap::new(), &inputs_r0, &resume_fired, &sctx)
   .await
   .unwrap();
  assert_eq!(out_r0.data.get("value").cloned(), Some(SocketValue::Json(serde_json::json!("A"))));

  // 3 度目の再開は pending が空 → エラー
  let err = node
   .compute(state.as_mut(), &InputMap::new(), &inputs_r, &resume_fired, &sctx)
   .await
   .unwrap_err();
  assert!(matches!(err, NodeExecError::Generic(_)));
 }

 #[tokio::test]
 async fn delay_with_unknown_exec_is_noop() {
  let node = DelayNode;
  let mut state: Box<dyn Any + Send> = node.init_state();
  let sctx = StatefulCtx { node_id: "d", trigger: None };
  // どちらの exec も発火していない
  let out = node
   .compute(
    state.as_mut(),
    &InputMap::new(),
    &InputMap::new(),
    &crate::flowgraph::node::ExecFireSet::new(),
    &sctx,
   )
   .await
   .unwrap();
  assert!(out.fired_exec.is_empty());
  assert!(out.data.is_empty());
 }
}
