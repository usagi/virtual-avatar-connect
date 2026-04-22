//! Stateful な「ミニ state machine」ノード群（δ-3c）。
//!
//! いずれも `StatefulNode` で、engine が保持する state slot を `&mut` で借りて状態遷移する。
//! I/O はしない（`ExecCtx` を受け取らない）ため、Pure な合成則に抵触せず、
//! generation 管理下でキャッシュ整合も engine 側が面倒を見る。
//!
//! ## 含まれるノード
//! - `flowgraph.state.bool`: true/false/toggle 可能なブール格納
//! - `flowgraph.state.int_counter`: increment/decrement/reset 可能な整数カウンタ
//! - `flowgraph.state.latch`: 任意 Json を保持する 1 値ラッチ
//! - `flowgraph.state.accumulator`: Json を List に溜め込むアキュムレータ

use crate::flowgraph::node::{
 get_optional_int, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, StatefulCtx,
 StatefulNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::any::Any;

// ---------------------------------------------------------------------
// state.bool
// ---------------------------------------------------------------------

pub struct BoolStateNode;

#[derive(Default)]
pub struct BoolStateState {
 value: bool,
}

impl NodeDescriptor for BoolStateNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.state.bool".into(),
   title: "Bool State".into(),
   category: "state".into(),
   description: Some("set_true/set_false/toggle でブール値を切り替える".into()),
   inputs: vec![
    PortSpec::exec_input("set_true", "Set True"),
    PortSpec::exec_input("set_false", "Set False"),
    PortSpec::exec_input("toggle", "Toggle"),
   ],
   outputs: vec![
    PortSpec::output("value", "Value", SocketType::Bool),
    PortSpec::exec_output("changed", "Changed"),
   ],
   properties: vec![],
  }
 }
}

#[async_trait]
impl StatefulNode for BoolStateNode {
 fn init_state(&self) -> Box<dyn Any + Send> {
  Box::new(BoolStateState::default())
 }

 async fn compute(
  &self,
  state: &mut (dyn Any + Send),
  _props: &InputMap,
  _inputs: &InputMap,
  fired_exec: &ExecFireSet,
  _ctx: &StatefulCtx<'_>,
 ) -> Result<NodeOutput, NodeExecError> {
  let state = state.downcast_mut::<BoolStateState>().expect("BoolStateState");
  let before = state.value;

  if fired_exec.contains("set_true") {
   state.value = true;
  } else if fired_exec.contains("set_false") {
   state.value = false;
  } else if fired_exec.contains("toggle") {
   state.value = !state.value;
  }

  let mut out = NodeOutput::new().set_data("value", SocketValue::Bool(state.value));
  if state.value != before && !fired_exec.is_empty() {
   out = out.fire_exec("changed");
  }
  Ok(out)
 }
}

// ---------------------------------------------------------------------
// state.int_counter
// ---------------------------------------------------------------------

pub struct IntCounterNode;

#[derive(Default)]
pub struct IntCounterState {
 value: i64,
}

impl NodeDescriptor for IntCounterNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.state.int_counter".into(),
   title: "Int Counter".into(),
   category: "state".into(),
   description: Some("step だけ増減/リセット可能な整数カウンタ".into()),
   inputs: vec![
    PortSpec::exec_input("increment", "Increment"),
    PortSpec::exec_input("decrement", "Decrement"),
    PortSpec::exec_input("reset", "Reset"),
    PortSpec::input("step", "Step", SocketType::Int).with_default(SocketValue::Int(1)),
    PortSpec::input("reset_value", "Reset Value", SocketType::Int).with_default(SocketValue::Int(0)),
   ],
   outputs: vec![
    PortSpec::output("value", "Value", SocketType::Int),
    PortSpec::exec_output("changed", "Changed"),
   ],
   properties: vec![],
  }
 }
}

#[async_trait]
impl StatefulNode for IntCounterNode {
 fn init_state(&self) -> Box<dyn Any + Send> {
  Box::new(IntCounterState::default())
 }

 async fn compute(
  &self,
  state: &mut (dyn Any + Send),
  _props: &InputMap,
  inputs: &InputMap,
  fired_exec: &ExecFireSet,
  _ctx: &StatefulCtx<'_>,
 ) -> Result<NodeOutput, NodeExecError> {
  let state = state.downcast_mut::<IntCounterState>().expect("IntCounterState");
  let before = state.value;
  let step = get_optional_int(inputs, "step", 1)?;
  let reset_value = get_optional_int(inputs, "reset_value", 0)?;

  if fired_exec.contains("increment") {
   state.value = state.value.wrapping_add(step);
  } else if fired_exec.contains("decrement") {
   state.value = state.value.wrapping_sub(step);
  } else if fired_exec.contains("reset") {
   state.value = reset_value;
  }

  let mut out = NodeOutput::new().set_data("value", SocketValue::Int(state.value));
  if state.value != before && !fired_exec.is_empty() {
   out = out.fire_exec("changed");
  }
  Ok(out)
 }
}

// ---------------------------------------------------------------------
// state.latch
// ---------------------------------------------------------------------

pub struct LatchNode;

pub struct LatchState {
 value: JsonValue,
 has_value: bool,
}

impl Default for LatchState {
 fn default() -> Self {
  Self { value: JsonValue::Null, has_value: false }
 }
}

impl NodeDescriptor for LatchNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.state.latch".into(),
   title: "Latch".into(),
   category: "state".into(),
   description: Some("exec_in で input を保持。以後 value 出力は保持値を返す".into()),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("input", "Input", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
   ],
   outputs: vec![
    PortSpec::output("value", "Value", SocketType::Json),
    PortSpec::output("has_value", "Has Value", SocketType::Bool),
    PortSpec::exec_output("updated", "Updated"),
   ],
   properties: vec![],
  }
 }
}

#[async_trait]
impl StatefulNode for LatchNode {
 fn init_state(&self) -> Box<dyn Any + Send> {
  Box::new(LatchState::default())
 }

 async fn compute(
  &self,
  state: &mut (dyn Any + Send),
  _props: &InputMap,
  inputs: &InputMap,
  fired_exec: &ExecFireSet,
  _ctx: &StatefulCtx<'_>,
 ) -> Result<NodeOutput, NodeExecError> {
  let state = state.downcast_mut::<LatchState>().expect("LatchState");

  let mut fire_updated = false;
  if fired_exec.contains("exec_in") {
   if let Some(v) = inputs.get("input") {
    state.value = socket_value_to_json(v);
    state.has_value = true;
    fire_updated = true;
   }
  }

  let mut out = NodeOutput::new()
   .set_data("value", SocketValue::Json(state.value.clone()))
   .set_data("has_value", SocketValue::Bool(state.has_value));
  if fire_updated {
   out = out.fire_exec("updated");
  }
  Ok(out)
 }
}

// ---------------------------------------------------------------------
// state.accumulator
// ---------------------------------------------------------------------

pub struct AccumulatorNode;

#[derive(Default)]
pub struct AccumulatorState {
 items: Vec<JsonValue>,
}

impl NodeDescriptor for AccumulatorNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.state.accumulator".into(),
   title: "Accumulator".into(),
   category: "state".into(),
   description: Some("push で input を List に蓄積、clear で空に戻す".into()),
   inputs: vec![
    PortSpec::exec_input("push", "Push"),
    PortSpec::exec_input("clear", "Clear"),
    PortSpec::input("input", "Input", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
   ],
   outputs: vec![
    PortSpec::output("items", "Items", SocketType::List(Box::new(SocketType::Json))),
    PortSpec::output("count", "Count", SocketType::Int),
    PortSpec::exec_output("updated", "Updated"),
    PortSpec::exec_output("cleared", "Cleared"),
   ],
   properties: vec![],
  }
 }
}

#[async_trait]
impl StatefulNode for AccumulatorNode {
 fn init_state(&self) -> Box<dyn Any + Send> {
  Box::new(AccumulatorState::default())
 }

 async fn compute(
  &self,
  state: &mut (dyn Any + Send),
  _props: &InputMap,
  inputs: &InputMap,
  fired_exec: &ExecFireSet,
  _ctx: &StatefulCtx<'_>,
 ) -> Result<NodeOutput, NodeExecError> {
  let state = state.downcast_mut::<AccumulatorState>().expect("AccumulatorState");
  let mut fire_updated = false;
  let mut fire_cleared = false;

  if fired_exec.contains("push") {
   if let Some(v) = inputs.get("input") {
    state.items.push(socket_value_to_json(v));
    fire_updated = true;
   }
  }
  if fired_exec.contains("clear") && !state.items.is_empty() {
   state.items.clear();
   fire_cleared = true;
  }

  let list: Vec<SocketValue> = state.items.iter().map(|v| SocketValue::Json(v.clone())).collect();
  let mut out = NodeOutput::new()
   .set_data("items", SocketValue::List(list))
   .set_data("count", SocketValue::Int(state.items.len() as i64));
  if fire_updated {
   out = out.fire_exec("updated");
  }
  if fire_cleared {
   out = out.fire_exec("cleared");
  }
  Ok(out)
 }
}

// ---------------------------------------------------------------------
// helpers
// ---------------------------------------------------------------------

fn socket_value_to_json(v: &SocketValue) -> JsonValue {
 match v {
  SocketValue::Bool(b) => JsonValue::Bool(*b),
  SocketValue::Int(i) => JsonValue::Number((*i).into()),
  SocketValue::Float(f) => serde_json::Number::from_f64(*f)
   .map(JsonValue::Number)
   .unwrap_or(JsonValue::Null),
  SocketValue::String(s) => JsonValue::String(s.clone()),
  SocketValue::Json(j) => j.clone(),
  SocketValue::List(xs) => JsonValue::Array(xs.iter().map(socket_value_to_json).collect()),
  SocketValue::Map(m) => {
   JsonValue::Object(m.iter().map(|(k, v)| (k.clone(), socket_value_to_json(v))).collect())
  }
  SocketValue::Table(t) => t.to_json_array(),
 }
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
 use super::*;

 fn fire(port: &str) -> ExecFireSet {
  let mut f = ExecFireSet::new();
  f.insert(port);
  f
 }
 fn sctx<'a>() -> StatefulCtx<'a> {
  StatefulCtx { node_id: "n", trigger: None }
 }

 // ----- bool -----

 #[tokio::test]
 async fn bool_state_set_and_toggle() {
  let node = BoolStateNode;
  let mut state: Box<dyn Any + Send> = node.init_state();
  let empty = InputMap::new();
  let sctx = sctx();

  let out = node.compute(state.as_mut(), &empty, &empty, &fire("set_true"), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Bool(true)));
  assert!(out.fired_exec.contains("changed"));

  // already true → set_true で変化なし → changed 非発火
  let out = node.compute(state.as_mut(), &empty, &empty, &fire("set_true"), &sctx).await.unwrap();
  assert!(!out.fired_exec.contains("changed"));

  let out = node.compute(state.as_mut(), &empty, &empty, &fire("toggle"), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Bool(false)));
  assert!(out.fired_exec.contains("changed"));

  // no firing → data のみ返す
  let out = node.compute(state.as_mut(), &empty, &empty, &ExecFireSet::new(), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Bool(false)));
  assert!(out.fired_exec.is_empty());
 }

 // ----- int_counter -----

 #[tokio::test]
 async fn int_counter_increment_decrement_reset() {
  let node = IntCounterNode;
  let mut state: Box<dyn Any + Send> = node.init_state();
  let sctx = sctx();
  let inputs: InputMap = [
   ("step".into(), SocketValue::Int(5)),
   ("reset_value".into(), SocketValue::Int(100)),
  ]
  .into_iter()
  .collect();

  let out = node.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("increment"), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Int(5)));
  let out = node.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("increment"), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Int(10)));
  let out = node.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("decrement"), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Int(5)));
  let out = node.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("reset"), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Int(100)));
 }

 // ----- latch -----

 #[tokio::test]
 async fn latch_holds_last_value() {
  let node = LatchNode;
  let mut state: Box<dyn Any + Send> = node.init_state();
  let sctx = sctx();

  // 未 set 状態では has_value = false
  let out = node.compute(state.as_mut(), &InputMap::new(), &InputMap::new(), &ExecFireSet::new(), &sctx).await.unwrap();
  assert_eq!(out.data.get("has_value"), Some(&SocketValue::Bool(false)));

  let inputs: InputMap = [("input".into(), SocketValue::Json(serde_json::json!("hello")))].into_iter().collect();
  let out = node.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("exec_in"), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Json(serde_json::json!("hello"))));
  assert_eq!(out.data.get("has_value"), Some(&SocketValue::Bool(true)));
  assert!(out.fired_exec.contains("updated"));

  // 非発火時でも value は保持値
  let out = node.compute(state.as_mut(), &InputMap::new(), &InputMap::new(), &ExecFireSet::new(), &sctx).await.unwrap();
  assert_eq!(out.data.get("value"), Some(&SocketValue::Json(serde_json::json!("hello"))));
  assert!(out.fired_exec.is_empty());
 }

 // ----- accumulator -----

 #[tokio::test]
 async fn accumulator_push_and_clear() {
  let node = AccumulatorNode;
  let mut state: Box<dyn Any + Send> = node.init_state();
  let sctx = sctx();

  for n in 1..=3 {
   let inputs: InputMap = [("input".into(), SocketValue::Int(n))].into_iter().collect();
   let out = node.compute(state.as_mut(), &InputMap::new(), &inputs, &fire("push"), &sctx).await.unwrap();
   assert!(out.fired_exec.contains("updated"));
   assert_eq!(out.data.get("count"), Some(&SocketValue::Int(n)));
  }
  let out = node
   .compute(state.as_mut(), &InputMap::new(), &InputMap::new(), &fire("clear"), &sctx)
   .await
   .unwrap();
  assert!(out.fired_exec.contains("cleared"));
  assert_eq!(out.data.get("count"), Some(&SocketValue::Int(0)));
  // 再 clear は no-op（cleared 発火なし）
  let out = node
   .compute(state.as_mut(), &InputMap::new(), &InputMap::new(), &fire("clear"), &sctx)
   .await
   .unwrap();
  assert!(!out.fired_exec.contains("cleared"));
 }
}
