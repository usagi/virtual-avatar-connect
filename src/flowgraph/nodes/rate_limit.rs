//! `flowgraph.util.rate_limit`: N 回 / X ms のトークンバケット StatefulNode（δ-4d）。
//!
//! Twitch Helix の「100 req / 30s」系のゲートを graph-level で表現するための汎用ノード。
//! Twitch 以外でも「HTTP 連打防止」「Discord webhook スロットリング」などに使える。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec): 1 発火 = 1 token 取得試行
//!   - `max_count` (Int): 窓内最大回数（1 以上）
//!   - `window_ms` (Int, default 30000): 窓長（ミリ秒）
//! - 出力:
//!   - `on_allow` (Exec): token 取得に成功した場合
//!   - `on_deny` (Exec): レートリミット超過でドロップした場合
//!   - `remaining` (Int): 現在窓内の残り発火可能回数（drop 反映後、次回までの余力）

use crate::flowgraph::node::{
 get_optional_int, get_required_int, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec,
 PortSpec, StatefulCtx, StatefulNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use std::any::Any;
use std::collections::VecDeque;
use std::time::{Duration, Instant};

pub struct RateLimitNode;

#[derive(Default)]
pub struct RateLimitState {
 recent: VecDeque<Instant>,
}

impl NodeDescriptor for RateLimitNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.util.rate_limit".into(),
   title: "Rate Limit".into(),
   category: "util".into(),
   description: Some("N 回 / X ms のトークンバケットで exec_in をゲートする（超過時は on_deny）".into()),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("max_count", "Max Count", SocketType::Int),
    PortSpec::input("window_ms", "Window ms", SocketType::Int).with_default(SocketValue::Int(30000)),
   ],
   outputs: vec![
    PortSpec::exec_output("on_allow", "On Allow"),
    PortSpec::exec_output("on_deny", "On Deny"),
    PortSpec::output("remaining", "Remaining", SocketType::Int),
   ],
   properties: vec![],
  }
 }
}

#[async_trait]
impl StatefulNode for RateLimitNode {
 fn init_state(&self) -> Box<dyn Any + Send> {
  Box::new(RateLimitState::default())
 }

 async fn compute(
  &self,
  state: &mut (dyn Any + Send),
  _props: &InputMap,
  inputs: &InputMap,
  fired_exec: &ExecFireSet,
  _ctx: &StatefulCtx<'_>,
 ) -> Result<NodeOutput, NodeExecError> {
  let st = state.downcast_mut::<RateLimitState>().expect("RateLimitState");
  let max_count = get_required_int(inputs, "max_count")?.max(1);
  let window_ms = get_optional_int(inputs, "window_ms", 30000)?.max(1);

  let now = Instant::now();
  let cutoff = now - Duration::from_millis(window_ms as u64);
  while let Some(front) = st.recent.front() {
   if *front < cutoff {
    st.recent.pop_front();
   } else {
    break;
   }
  }

  if !fired_exec.contains("exec_in") {
   // 発火なし: 残数だけ報告
   let remaining = (max_count - st.recent.len() as i64).max(0);
   return Ok(NodeOutput::new().set_data("remaining", SocketValue::Int(remaining)));
  }

  if (st.recent.len() as i64) < max_count {
   st.recent.push_back(now);
   let remaining = (max_count - st.recent.len() as i64).max(0);
   Ok(NodeOutput::new().set_data("remaining", SocketValue::Int(remaining)).fire_exec("on_allow"))
  } else {
   let remaining = 0;
   Ok(NodeOutput::new().set_data("remaining", SocketValue::Int(remaining)).fire_exec("on_deny"))
  }
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 fn inputs_with(max: i64, window_ms: i64) -> InputMap {
  let mut m = InputMap::new();
  m.insert("max_count".into(), SocketValue::Int(max));
  m.insert("window_ms".into(), SocketValue::Int(window_ms));
  m
 }

 #[tokio::test]
 async fn allows_first_n_then_denies() {
  let node = RateLimitNode;
  let mut st: Box<dyn Any + Send> = node.init_state();
  let mut fired = ExecFireSet::new();
  fired.insert("exec_in");
  let ctx = StatefulCtx { node_id: "rl", trigger: None };

  for i in 0..3 {
   let out = node.compute(st.as_mut(), &InputMap::new(), &inputs_with(3, 10_000), &fired, &ctx).await.unwrap();
   assert!(out.fired_exec.contains("on_allow"), "iter {i}: expected allow");
   let remaining = out.data.get("remaining").and_then(|v| v.as_i64().ok()).unwrap();
   assert_eq!(remaining, 3 - (i as i64) - 1);
  }
  let out = node.compute(st.as_mut(), &InputMap::new(), &inputs_with(3, 10_000), &fired, &ctx).await.unwrap();
  assert!(out.fired_exec.contains("on_deny"));
 }

 #[tokio::test]
 async fn window_expiry_refills_budget() {
  let node = RateLimitNode;
  let mut st: Box<dyn Any + Send> = node.init_state();
  let mut fired = ExecFireSet::new();
  fired.insert("exec_in");
  let ctx = StatefulCtx { node_id: "rl", trigger: None };

  // 50ms 窓で 2 回まで。3 回目 deny。
  for _ in 0..2 {
   let out = node.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 50), &fired, &ctx).await.unwrap();
   assert!(out.fired_exec.contains("on_allow"));
  }
  let out = node.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 50), &fired, &ctx).await.unwrap();
  assert!(out.fired_exec.contains("on_deny"));

  // 窓を過ぎるまで待って再度発火
  tokio::time::sleep(Duration::from_millis(80)).await;
  let out = node.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 50), &fired, &ctx).await.unwrap();
  assert!(out.fired_exec.contains("on_allow"), "window should have expired and refilled");
 }

 #[tokio::test]
 async fn no_fire_reports_remaining_without_consuming() {
  let node = RateLimitNode;
  let mut st: Box<dyn Any + Send> = node.init_state();
  let mut fired = ExecFireSet::new();
  fired.insert("exec_in");
  let ctx = StatefulCtx { node_id: "rl", trigger: None };

  let out = node.compute(st.as_mut(), &InputMap::new(), &inputs_with(5, 10_000), &fired, &ctx).await.unwrap();
  assert!(out.fired_exec.contains("on_allow"));

  // no fire で呼び出し → 現在値を pull しただけのイメージ
  let out = node.compute(st.as_mut(), &InputMap::new(), &inputs_with(5, 10_000), &ExecFireSet::new(), &ctx).await.unwrap();
  assert!(out.fired_exec.is_empty());
  let remaining = out.data.get("remaining").and_then(|v| v.as_i64().ok()).unwrap();
  assert_eq!(remaining, 4);
 }

}
