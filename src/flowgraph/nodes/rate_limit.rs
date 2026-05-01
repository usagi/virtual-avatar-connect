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
	get_optional_int, get_required_int, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, StatefulCtx,
	StatefulNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::flowgraph::FlowgraphStateModel;
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::collections::VecDeque;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

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

	fn state_model(&self) -> FlowgraphStateModel {
		FlowgraphStateModel::volatile_node_instance_json_snapshot()
	}

	fn snapshot_state(&self, state: &(dyn Any + Send)) -> Option<JsonValue> {
		let state = state.downcast_ref::<RateLimitState>()?;
		let now = Instant::now();
		let recent_elapsed_ms: Vec<u64> = state
			.recent
			.iter()
			.map(|instant| duration_ms_u64(now.checked_duration_since(*instant).unwrap_or_default()))
			.collect();
		Some(serde_json::json!({
			"recorded_at_unix_ms": unix_time_ms(),
			"recent_elapsed_ms": recent_elapsed_ms,
		}))
	}

	fn restore_state(&self, state: &mut (dyn Any + Send), value: &JsonValue) -> Result<(), String> {
		let state = state
			.downcast_mut::<RateLimitState>()
			.ok_or_else(|| "RateLimitState downcast failed".to_string())?;
		let restored_recent = value
			.get("recent_elapsed_ms")
			.and_then(|value| value.as_array())
			.ok_or_else(|| "expected object with array field 'recent_elapsed_ms'".to_string())?;
		let recorded_at_unix_ms = match value.get("recorded_at_unix_ms") {
			Some(value) => Some(
				value
					.as_u64()
					.ok_or_else(|| "expected integer field 'recorded_at_unix_ms'".to_string())?,
			),
			None => None,
		};
		let now = Instant::now();
		let now_unix_ms = unix_time_ms();
		let mut recent = VecDeque::new();
		for value in restored_recent {
			let elapsed_ms = value
				.as_u64()
				.ok_or_else(|| "expected array field 'recent_elapsed_ms' to contain integers".to_string())?;
			let restored_elapsed_ms = if let Some(recorded_at_unix_ms) = recorded_at_unix_ms {
				let event_unix_ms = recorded_at_unix_ms.saturating_sub(elapsed_ms);
				now_unix_ms.saturating_sub(event_unix_ms)
			} else {
				elapsed_ms
			};
			recent.push_back(instant_from_elapsed_ms(now, restored_elapsed_ms));
		}
		state.recent = recent;
		Ok(())
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
			Ok(NodeOutput::new()
				.set_data("remaining", SocketValue::Int(remaining))
				.fire_exec("on_allow"))
		} else {
			let remaining = 0;
			Ok(NodeOutput::new()
				.set_data("remaining", SocketValue::Int(remaining))
				.fire_exec("on_deny"))
		}
	}
}

fn unix_time_ms() -> u64 {
	duration_ms_u64(SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default())
}

fn duration_ms_u64(duration: Duration) -> u64 {
	duration.as_millis().min(u128::from(u64::MAX)) as u64
}

fn instant_from_elapsed_ms(now: Instant, elapsed_ms: u64) -> Instant {
	let mut duration = Duration::from_millis(elapsed_ms.min(3_600_000));
	while duration > Duration::ZERO {
		if let Some(instant) = now.checked_sub(duration) {
			return instant;
		}
		duration /= 2;
	}
	now
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
		let ctx = StatefulCtx {
			node_id: "rl",
			trigger: None,
		};

		for i in 0..3 {
			let out = node
				.compute(st.as_mut(), &InputMap::new(), &inputs_with(3, 10_000), &fired, &ctx)
				.await
				.unwrap();
			assert!(out.fired_exec.contains("on_allow"), "iter {i}: expected allow");
			let remaining = out.data.get("remaining").and_then(|v| v.as_i64().ok()).unwrap();
			assert_eq!(remaining, 3 - (i as i64) - 1);
		}
		let out = node
			.compute(st.as_mut(), &InputMap::new(), &inputs_with(3, 10_000), &fired, &ctx)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_deny"));
	}

	#[tokio::test]
	async fn window_expiry_refills_budget() {
		let node = RateLimitNode;
		let mut st: Box<dyn Any + Send> = node.init_state();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let ctx = StatefulCtx {
			node_id: "rl",
			trigger: None,
		};

		// 50ms 窓で 2 回まで。3 回目 deny。
		for _ in 0..2 {
			let out = node
				.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 50), &fired, &ctx)
				.await
				.unwrap();
			assert!(out.fired_exec.contains("on_allow"));
		}
		let out = node
			.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 50), &fired, &ctx)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_deny"));

		// 窓を過ぎるまで待って再度発火
		tokio::time::sleep(Duration::from_millis(80)).await;
		let out = node
			.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 50), &fired, &ctx)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_allow"), "window should have expired and refilled");
	}

	#[tokio::test]
	async fn no_fire_reports_remaining_without_consuming() {
		let node = RateLimitNode;
		let mut st: Box<dyn Any + Send> = node.init_state();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let ctx = StatefulCtx {
			node_id: "rl",
			trigger: None,
		};

		let out = node
			.compute(st.as_mut(), &InputMap::new(), &inputs_with(5, 10_000), &fired, &ctx)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_allow"));

		// no fire で呼び出し → 現在値を pull しただけのイメージ
		let out = node
			.compute(st.as_mut(), &InputMap::new(), &inputs_with(5, 10_000), &ExecFireSet::new(), &ctx)
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
		let remaining = out.data.get("remaining").and_then(|v| v.as_i64().ok()).unwrap();
		assert_eq!(remaining, 4);
	}

	#[tokio::test]
	async fn snapshot_restore_preserves_rate_limit_budget() {
		let node = RateLimitNode;
		let mut st: Box<dyn Any + Send> = node.init_state();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let ctx = StatefulCtx {
			node_id: "rl",
			trigger: None,
		};

		assert!(node.state_model().snapshot_supported);
		assert!(node.state_model().restore_supported);
		for _ in 0..2 {
			let out = node
				.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 10_000), &fired, &ctx)
				.await
				.unwrap();
			assert!(out.fired_exec.contains("on_allow"));
		}
		let snapshot = node.snapshot_state(st.as_ref()).expect("snapshot");
		assert!(snapshot.get("recorded_at_unix_ms").and_then(|value| value.as_u64()).is_some());
		assert_eq!(
			snapshot
				.get("recent_elapsed_ms")
				.and_then(|value| value.as_array())
				.map(|values| values.len()),
			Some(2)
		);

		let mut restored: Box<dyn Any + Send> = node.init_state();
		node.restore_state(restored.as_mut(), &snapshot).unwrap();
		let out = node
			.compute(restored.as_mut(), &InputMap::new(), &inputs_with(2, 10_000), &fired, &ctx)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_deny"));
	}

	#[tokio::test]
	async fn restore_with_recorded_wall_time_expires_old_entries() {
		let node = RateLimitNode;
		let mut st: Box<dyn Any + Send> = node.init_state();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let ctx = StatefulCtx {
			node_id: "rl",
			trigger: None,
		};

		node.restore_state(
			st.as_mut(),
			&serde_json::json!({
				"recorded_at_unix_ms": 0,
				"recent_elapsed_ms": [0, 0],
			}),
		)
		.unwrap();
		let out = node
			.compute(st.as_mut(), &InputMap::new(), &inputs_with(2, 50), &fired, &ctx)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_allow"));
		assert_eq!(
			node.restore_state(st.as_mut(), &serde_json::json!({ "recent_elapsed_ms": [null] }))
				.unwrap_err(),
			"expected array field 'recent_elapsed_ms' to contain integers"
		);
	}
}
