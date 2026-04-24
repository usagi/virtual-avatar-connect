//! `flowgraph.util.timer_interval`: periodic exec source (Phase \u{03BF}-4).
//!
//! Stateful self-trigger pattern from [`super::delay::DelayNode`]. When `enabled` is true and
//! `ctx.trigger` is available (`run_forever`), schedules `__tick__` after `interval_sec` (clamped
//! to at least 10ms). Valid tick fires `on_tick` exec and updates `count` / `elapsed_sec`.
//!
//! Spec source: [`docs/roadmap/backlog-nodes.md`](../../../../docs/roadmap/backlog-nodes.md) \u00a71.

use crate::flowgraph::node::{
	get_optional_bool, get_optional_float, get_optional_int, ExecFireSet, InputMap, NodeDescriptor, NodeExecError,
	NodeOutput, NodeSpec, PortSpec, StatefulCtx, StatefulNode, TriggerEvent, TriggerHandle,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use std::any::Any;
use std::time::{Duration, Instant};

/// Max sleep duration (ms) to avoid absurd `tokio::time::sleep` spans (~1 year cap).
const MAX_SLEEP_MS: u64 = 365 * 24 * 3600 * 1000;

pub struct TimerIntervalNode;

#[derive(Debug)]
pub struct TimerIntervalState {
	next_id: u64,
	tick_count: i64,
	/// Pending wake id we are waiting on; `None` when disabled or no timer armed.
	armed_id: Option<u64>,
	last_emit_at: Option<Instant>,
	last_enabled: bool,
}

impl Default for TimerIntervalState {
	fn default() -> Self {
		Self {
			next_id: 0,
			tick_count: 0,
			armed_id: None,
			last_emit_at: None,
			last_enabled: false,
		}
	}
}

fn normalize_interval_sec(raw: f64) -> f64 {
	if !raw.is_finite() || raw <= 0.0 {
		return 1.0;
	}
	raw.max(0.01)
}

fn interval_to_sleep_ms(interval_sec: f64) -> u64 {
	let ms = (interval_sec * 1000.0).max(10.0);
	if !ms.is_finite() {
		return 10;
	}
	if ms > MAX_SLEEP_MS as f64 {
		return MAX_SLEEP_MS;
	}
	ms as u64
}

/// Allocate `id`, spawn sleep + trigger send, return `id`.
fn spawn_tick_arm(state: &mut TimerIntervalState, interval_sec: f64, node_id: &str, trigger: &TriggerHandle) -> u64 {
	let id = state.next_id;
	state.next_id = state.next_id.wrapping_add(1);
	let delay_ms = interval_to_sleep_ms(interval_sec);
	let handle = trigger.clone();
	let nid = node_id.to_string();
	tokio::spawn(async move {
		tokio::time::sleep(Duration::from_millis(delay_ms)).await;
		let _ = handle.send(
			TriggerEvent::new(nid)
				.with_exec("__tick__")
				.with_override("__pending_id__", SocketValue::Int(id as i64)),
		);
	});
	id
}

impl NodeDescriptor for TimerIntervalNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.timer_interval".into(),
			title: "Timer Interval".into(),
			category: "util".into(),
			description: Some(
				"Periodic timer: while `enabled` and `run_forever` trigger bus is active, fires `on_tick` \
				 every `interval_sec` (min 0.01s, sleep min 10ms). Outputs `count` (total ticks) and \
				 `elapsed_sec` (wall time since previous tick, or `interval_sec` on first tick). \
				 Stale wakeups are dropped. `execute()` one-shot mode does not arm (same as `util.delay`)."
					.into(),
			),
			inputs: vec![
				PortSpec::input("enabled", "Enabled", SocketType::Bool).with_default(SocketValue::Bool(true)),
				PortSpec::input("interval_sec", "Interval (s)", SocketType::Float).with_default(SocketValue::Float(1.0)),
				PortSpec::exec_input("__tick__", "(internal)"),
				PortSpec::input("__pending_id__", "(internal)", SocketType::Int).with_default(SocketValue::Int(-1)),
			],
			outputs: vec![
				PortSpec::exec_output("on_tick", "On tick"),
				PortSpec::output("count", "Tick count", SocketType::Int),
				PortSpec::output("elapsed_sec", "Elapsed (s)", SocketType::Float),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl StatefulNode for TimerIntervalNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(TimerIntervalState::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
		ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let st = state.downcast_mut::<TimerIntervalState>().expect("TimerIntervalState");
		let enabled = get_optional_bool(inputs, "enabled", true)?;
		let interval_sec = normalize_interval_sec(get_optional_float(inputs, "interval_sec", 1.0)?);

		// --- internal tick wakeup ---
		if fired_exec.contains("__tick__") {
			let pending = get_optional_int(inputs, "__pending_id__", -1)?;
			if pending < 0 {
				st.armed_id = None;
				st.last_enabled = enabled;
				return Ok(NodeOutput::new());
			}
			let pending_u = pending as u64;
			let matches = st.armed_id == Some(pending_u);

			if !matches || !enabled {
				st.armed_id = None;
				st.last_enabled = enabled;
				return Ok(NodeOutput::new());
			}

			let now = Instant::now();
			let elapsed_sec = st
				.last_emit_at
				.map(|t| now.duration_since(t).as_secs_f64())
				.unwrap_or(interval_sec);
			st.tick_count = st.tick_count.saturating_add(1);
			st.last_emit_at = Some(now);
			st.armed_id = None;

			let mut out = NodeOutput::new()
				.set_data("count", SocketValue::Int(st.tick_count))
				.set_data("elapsed_sec", SocketValue::Float(elapsed_sec));

			if enabled {
				if let Some(handle) = ctx.trigger {
					let id = spawn_tick_arm(st, interval_sec, ctx.node_id, handle);
					st.armed_id = Some(id);
				}
			}

			st.last_enabled = enabled;
			out = out.fire_exec("on_tick");
			return Ok(out);
		}

		// --- data pull / upstream changes (no __tick__) ---
		if !enabled {
			st.armed_id = None;
		} else if st.armed_id.is_none() {
			if let Some(handle) = ctx.trigger {
				let id = spawn_tick_arm(st, interval_sec, ctx.node_id, handle);
				st.armed_id = Some(id);
			}
		}

		st.last_enabled = enabled;
		Ok(NodeOutput::new())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::engine::{FlowgraphBuilder, PortRef};
	use crate::flowgraph::node::NodeImpl;
	use crate::flowgraph::nodes::literal::{BoolLiteralNode, StringLiteralNode};
	use crate::flowgraph::nodes::log::LogNode;
	use std::sync::Arc;
	use std::time::Duration;

	#[tokio::test]
	async fn timer_interval_one_shot_execute_does_not_arm() {
		let mut b = FlowgraphBuilder::new();
		b.add_node("ti", NodeImpl::stateful(Arc::new(TimerIntervalNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.connect_exec(PortRef::new("ti", "on_tick"), PortRef::new("log", "exec_in"));
		b.add_node(
			"msg",
			NodeImpl::pure(Arc::new(StringLiteralNode)),
			[("value".to_string(), SocketValue::String("tick".into()))].into_iter().collect(),
		);
		b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));

		let mut prog = b.build().expect("build");
		let mut ctx = crate::flowgraph::node::ExecCtx::default();
		prog.execute(&mut ctx).await.expect("execute");
		assert!(ctx.trace.is_empty(), "no trigger bus in 1-shot: no ticks");
	}

	#[tokio::test]
	async fn timer_interval_emits_multiple_ticks_under_run_forever() {
		let mut b = FlowgraphBuilder::new();
		b.add_node("ti", NodeImpl::stateful(Arc::new(TimerIntervalNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.connect_exec(PortRef::new("ti", "on_tick"), PortRef::new("log", "exec_in"));
		b.add_node(
			"msg",
			NodeImpl::pure(Arc::new(StringLiteralNode)),
			[("value".to_string(), SocketValue::String("tick".into()))].into_iter().collect(),
		);
		b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));

		b.add_node(
			"iv",
			NodeImpl::pure(Arc::new(crate::flowgraph::nodes::literal::FloatLiteralNode)),
			[("value".to_string(), SocketValue::Float(0.03))].into_iter().collect(),
		);
		b.connect(PortRef::new("iv", "value"), PortRef::new("ti", "interval_sec"));

		let mut prog = b.build().expect("build");
		let mut ctx = crate::flowgraph::node::ExecCtx::default();
		let shutdown = tokio::time::sleep(Duration::from_millis(320));
		prog.run_forever(&mut ctx, shutdown).await.expect("run_forever");

		let n = ctx.trace.len();
		assert!(
			(4..=12).contains(&n),
			"expected several ticks in ~320ms at 30ms interval, got {n}: {:?}",
			ctx.trace
		);
		for line in &ctx.trace {
			assert!(line.contains("tick"), "trace: {line}");
		}
	}

	#[tokio::test]
	async fn timer_interval_respects_enabled_false() {
		let mut b = FlowgraphBuilder::new();
		b.add_node("ti", NodeImpl::stateful(Arc::new(TimerIntervalNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.connect_exec(PortRef::new("ti", "on_tick"), PortRef::new("log", "exec_in"));
		b.add_node(
			"msg",
			NodeImpl::pure(Arc::new(StringLiteralNode)),
			[("value".to_string(), SocketValue::String("x".into()))].into_iter().collect(),
		);
		b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));
		b.add_node(
			"dis",
			NodeImpl::pure(Arc::new(BoolLiteralNode)),
			[("value".to_string(), SocketValue::Bool(false))].into_iter().collect(),
		);
		b.connect(PortRef::new("dis", "value"), PortRef::new("ti", "enabled"));

		let mut prog = b.build().expect("build");
		let mut ctx = crate::flowgraph::node::ExecCtx::default();
		let shutdown = tokio::time::sleep(Duration::from_millis(200));
		prog.run_forever(&mut ctx, shutdown).await.expect("run_forever");
		assert!(ctx.trace.is_empty(), "disabled: no ticks, got {:?}", ctx.trace);
	}

	#[tokio::test]
	async fn timer_interval_state_machine_stale_tick_drops_without_rearm() {
		let node = TimerIntervalNode;
		let mut state: Box<dyn Any + Send> = Box::new(TimerIntervalState::default());

		let mut fired = ExecFireSet::new();
		fired.insert("__tick__");
		let inputs: InputMap = [
			("enabled".into(), SocketValue::Bool(true)),
			("interval_sec".into(), SocketValue::Float(60.0)),
			("__pending_id__".into(), SocketValue::Int(999)),
		]
		.into_iter()
		.collect();

		let sctx = StatefulCtx { node_id: "ti", trigger: None };
		let out = node
			.compute(state.as_mut(), &InputMap::new(), &inputs, &fired, &sctx)
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
		let st = state.downcast_mut::<TimerIntervalState>().unwrap();
		assert!(st.armed_id.is_none());
	}
}
