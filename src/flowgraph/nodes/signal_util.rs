//! Phase ο-5: signal util (`flowgraph.util.edge_detect`, `prev_value`, `sample_hold`, `debounce`, `throttle`).
//!
//! ## `edge_detect` と exec
//!
//! Engine の pull 評価（`evaluate_pure_or_stateful`）ではノードが返した `fired_exec` を配送しない。
//! そのため `edge_detect` は **`exec_in` が発火したタイミング**で `value` を読み、前回値と比較して
//! `on_edge` / `edge_type` を更新する（`bool` の `changed` などと組み合わせる想定）。

use crate::flowgraph::node::{
	get_optional_int, get_required_bool, get_required_json, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec,
	PortSpec, PropertySpec, StatefulCtx, StatefulNode, TriggerEvent,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::Value as JsonValue;
use std::any::Any;
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------
// edge_detect
// ---------------------------------------------------------------------

pub struct EdgeDetectNode;

#[derive(Default)]
pub struct EdgeDetectState {
	prev: Option<bool>,
	last_edge_type: String,
}

impl NodeDescriptor for EdgeDetectNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.edge_detect".into(),
			title: "Edge detect".into(),
			category: "util".into(),
			description: Some(
				"On each `exec_in`, compares `value` to the previous sample and may fire `on_edge` \
				 with `edge_type` (`rising` / `falling`). Property `mode`: `rising` | `falling` | `both` (default). \
				 Pull-only reads return the last `edge_type` string (initially empty). \
				 Wire `exec_in` together with the signal you sample (e.g. `state.bool` `changed` exec)."
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Sample"),
				PortSpec::input("value", "Value", SocketType::Bool),
			],
			outputs: vec![
				PortSpec::output("edge_type", "Edge type", SocketType::String),
				PortSpec::exec_output("on_edge", "On edge"),
			],
			properties: vec![
				PropertySpec::new("mode", "Mode", SocketType::String, SocketValue::String("both".into()))
					.description("rising: low→high only, falling: high→low only, both: either transition.")
					.with_choices(["rising", "falling", "both"]),
			],
		}
	}
}

fn edge_mode(props: &InputMap) -> Result<String, NodeExecError> {
	let s = props.get("mode").and_then(|v| v.as_str().ok()).unwrap_or("both");
	match s {
		"rising" | "falling" | "both" => Ok(s.to_string()),
		_ => Err(NodeExecError::Generic(anyhow::anyhow!(
			"edge_detect: unknown mode '{s}', expected rising | falling | both"
		))),
	}
}

#[async_trait]
impl StatefulNode for EdgeDetectNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(EdgeDetectState::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
		_ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let state = state.downcast_mut::<EdgeDetectState>().expect("EdgeDetectState");

		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new().set_data("edge_type", SocketValue::String(state.last_edge_type.clone())));
		}

		let cur = get_required_bool(inputs, "value")?;
		let mode = edge_mode(props)?;

		let mut out = NodeOutput::new().set_data("edge_type", SocketValue::String(String::new()));

		if let Some(prev) = state.prev {
			if prev != cur {
				let rising = !prev && cur;
				let falling = prev && !cur;
				let (fire, label) = match mode.as_str() {
					"rising" => (rising, "rising"),
					"falling" => (falling, "falling"),
					"both" => {
						if rising {
							(true, "rising")
						} else if falling {
							(true, "falling")
						} else {
							(false, "")
						}
					}
					_ => (false, ""),
				};
				if fire && !label.is_empty() {
					state.last_edge_type = label.to_string();
					out = out.set_data("edge_type", SocketValue::String(label.into())).fire_exec("on_edge");
				}
			}
		}
		state.prev = Some(cur);
		Ok(out)
	}
}

// ---------------------------------------------------------------------
// prev_value
// ---------------------------------------------------------------------

pub struct PrevValueNode;

#[derive(Default)]
pub struct PrevValueState {
	last: Option<JsonValue>,
}

impl NodeDescriptor for PrevValueNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.prev_value".into(),
			title: "Previous value".into(),
			category: "util".into(),
			description: Some(
				"Each time `value` is evaluated, outputs `prev`: the **previous** input JSON. \
				 First sample: `prev` equals the current `value`."
					.into(),
			),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null))],
			outputs: vec![PortSpec::output("prev", "Previous", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl StatefulNode for PrevValueNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(PrevValueState::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		_props: &InputMap,
		inputs: &InputMap,
		_fired_exec: &ExecFireSet,
		_ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let state = state.downcast_mut::<PrevValueState>().expect("PrevValueState");
		let cur = get_required_json(inputs, "value")?.clone();
		let prev_out = match &state.last {
			None => cur.clone(),
			Some(p) => p.clone(),
		};
		state.last = Some(cur);
		Ok(NodeOutput::new().set_data("prev", SocketValue::Json(prev_out)))
	}
}

// ---------------------------------------------------------------------
// sample_hold
// ---------------------------------------------------------------------

pub struct SampleHoldNode;

#[derive(Default)]
pub struct SampleHoldState {
	held: JsonValue,
}

impl NodeDescriptor for SampleHoldNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.sample_hold".into(),
			title: "Sample & hold".into(),
			category: "util".into(),
			description: Some(
				"While `sample_exec` fires, captures the current `value` JSON into `held`. \
				 Between samples, `held` stays constant."
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("sample_exec", "Sample"),
				PortSpec::input("value", "Value", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
			],
			outputs: vec![PortSpec::output("held", "Held", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl StatefulNode for SampleHoldNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(SampleHoldState::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
		_ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let state = state.downcast_mut::<SampleHoldState>().expect("SampleHoldState");
		if fired_exec.contains("sample_exec") {
			state.held = get_required_json(inputs, "value")?.clone();
		}
		Ok(NodeOutput::new().set_data("held", SocketValue::Json(state.held.clone())))
	}
}

// ---------------------------------------------------------------------
// debounce
// ---------------------------------------------------------------------

pub struct DebounceNode;

pub struct DebounceState {
	next_id: u64,
	armed_id: Option<u64>,
	pending: JsonValue,
	last_raw: Option<JsonValue>,
	last_out: JsonValue,
}

impl Default for DebounceState {
	fn default() -> Self {
		Self {
			next_id: 0,
			armed_id: None,
			pending: JsonValue::Null,
			last_raw: None,
			last_out: JsonValue::Null,
		}
	}
}

impl NodeDescriptor for DebounceNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.debounce".into(),
			title: "Debounce".into(),
			category: "util".into(),
			description: Some(
				"After `value` JSON stops changing for `deadtime_ms`, updates `value_out` to the stable value. \
				 Each change restarts the timer (`ctx.trigger` + `__resume__`). Requires `run_forever` for async debounce."
					.into(),
			),
			inputs: vec![
				PortSpec::input("value", "Value", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
				PortSpec::input("deadtime_ms", "Dead time (ms)", SocketType::Int).with_default(SocketValue::Int(100)),
				PortSpec::exec_input("__resume__", "(internal)"),
				PortSpec::input("__pending_id__", "(internal)", SocketType::Int).with_default(SocketValue::Int(-1)),
			],
			outputs: vec![PortSpec::output("value_out", "Value out", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl StatefulNode for DebounceNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(DebounceState::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
		ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let state = state.downcast_mut::<DebounceState>().expect("DebounceState");

		if fired_exec.contains("__resume__") {
			let id = get_optional_int(inputs, "__pending_id__", -1)? as u64;
			if state.armed_id == Some(id) {
				state.last_out = state.pending.clone();
				state.armed_id = None;
			}
			return Ok(NodeOutput::new().set_data("value_out", SocketValue::Json(state.last_out.clone())));
		}

		let vin = get_required_json(inputs, "value")?.clone();
		let dead_ms = get_optional_int(inputs, "deadtime_ms", 100)?.max(0) as u64;
		let wait = Duration::from_millis(dead_ms.max(1));

		let changed = state.last_raw.as_ref() != Some(&vin);
		if changed {
			state.last_raw = Some(vin.clone());
			state.pending = vin;
			if let Some(handle) = ctx.trigger {
				let handle = handle.clone();
				let node_id = ctx.node_id.to_string();
				let id = state.next_id;
				state.next_id = state.next_id.wrapping_add(1);
				state.armed_id = Some(id);
				tokio::spawn(async move {
					tokio::time::sleep(wait).await;
					let _ = handle.send(
						TriggerEvent::new(node_id)
							.with_exec("__resume__")
							.with_override("__pending_id__", SocketValue::Int(id as i64)),
					);
				});
			}
		}

		Ok(NodeOutput::new().set_data("value_out", SocketValue::Json(state.last_out.clone())))
	}
}

// ---------------------------------------------------------------------
// throttle
// ---------------------------------------------------------------------

pub struct ThrottleNode;

#[derive(Default)]
pub struct ThrottleState {
	last_sent: JsonValue,
	last_emit_at: Option<Instant>,
}

impl NodeDescriptor for ThrottleNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.throttle".into(),
			title: "Throttle".into(),
			category: "util".into(),
			description: Some(
				"Leading-edge throttle on JSON `value`: when the value **differs** from the last emitted one, \
				 emit immediately only if at least `interval_ms` has passed since the last emit; otherwise drop the update. \
				 Identical consecutive values are passed through without resetting the timer."
					.into(),
			),
			inputs: vec![
				PortSpec::input("value", "Value", SocketType::Json).with_default(SocketValue::Json(JsonValue::Null)),
				PortSpec::input("interval_ms", "Interval (ms)", SocketType::Int).with_default(SocketValue::Int(100)),
			],
			outputs: vec![PortSpec::output("value_out", "Value out", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl StatefulNode for ThrottleNode {
	fn init_state(&self) -> Box<dyn Any + Send> {
		Box::new(ThrottleState::default())
	}

	async fn compute(
		&self,
		state: &mut (dyn Any + Send),
		_props: &InputMap,
		inputs: &InputMap,
		_fired_exec: &ExecFireSet,
		_ctx: &StatefulCtx<'_>,
	) -> Result<NodeOutput, NodeExecError> {
		let state = state.downcast_mut::<ThrottleState>().expect("ThrottleState");
		let vin = get_required_json(inputs, "value")?.clone();
		let interval_ms = get_optional_int(inputs, "interval_ms", 100)?.max(0) as u64;
		let interval = Duration::from_millis(interval_ms.max(1));

		if json_eq(&vin, &state.last_sent) {
			return Ok(NodeOutput::new().set_data("value_out", SocketValue::Json(state.last_sent.clone())));
		}

		let now = Instant::now();
		let can_emit = state.last_emit_at.map(|t| now.duration_since(t) >= interval).unwrap_or(true);

		if can_emit {
			state.last_sent = vin.clone();
			state.last_emit_at = Some(now);
		}

		Ok(NodeOutput::new().set_data("value_out", SocketValue::Json(state.last_sent.clone())))
	}
}

fn json_eq(a: &JsonValue, b: &JsonValue) -> bool {
	a == b
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::engine::{create_trigger_bus, FlowgraphBuilder, PortRef};
	use crate::flowgraph::node::NodeImpl;
	use crate::flowgraph::node::TriggerHandle;
	use crate::flowgraph::nodes::literal::StringLiteralNode;
	use crate::flowgraph::nodes::log::LogNode;
	use crate::flowgraph::nodes::state::BoolStateNode;
	use crate::flowgraph::nodes::timer_interval::TimerIntervalNode;
	use std::sync::Arc;
	use std::time::Duration;

	fn sctx<'a>(node_id: &'a str, trigger: Option<&'a TriggerHandle>) -> StatefulCtx<'a> {
		StatefulCtx { node_id, trigger }
	}

	#[tokio::test]
	async fn prev_value_first_then_second() {
		let n = PrevValueNode;
		let mut st: Box<dyn Any + Send> = Box::new(PrevValueState::default());
		let ctx = sctx("p", None);
		let out1 = n
			.compute(
				st.as_mut(),
				&InputMap::new(),
				&[("value".into(), SocketValue::Json(serde_json::json!(1)))].into_iter().collect(),
				&ExecFireSet::new(),
				&ctx,
			)
			.await
			.unwrap();
		assert_eq!(out1.data.get("prev").unwrap().as_json().unwrap(), &serde_json::json!(1));
		let out2 = n
			.compute(
				st.as_mut(),
				&InputMap::new(),
				&[("value".into(), SocketValue::Json(serde_json::json!(2)))].into_iter().collect(),
				&ExecFireSet::new(),
				&ctx,
			)
			.await
			.unwrap();
		assert_eq!(out2.data.get("prev").unwrap().as_json().unwrap(), &serde_json::json!(1));
	}

	#[tokio::test]
	async fn edge_detect_rising_on_exec() {
		let n = EdgeDetectNode;
		let mut st: Box<dyn Any + Send> = Box::new(EdgeDetectState::default());
		let props: InputMap = [("mode".into(), SocketValue::String("rising".into()))].into_iter().collect();
		let ctx = sctx("e", None);
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let _ = n
			.compute(
				st.as_mut(),
				&props,
				&[("value".into(), SocketValue::Bool(false))].into_iter().collect(),
				&fired,
				&ctx,
			)
			.await
			.unwrap();
		let out = n
			.compute(
				st.as_mut(),
				&props,
				&[("value".into(), SocketValue::Bool(true))].into_iter().collect(),
				&fired,
				&ctx,
			)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_edge"));
		assert_eq!(out.data.get("edge_type").unwrap().as_str().unwrap(), "rising");
	}

	#[tokio::test]
	async fn sample_hold_updates_on_sample_only() {
		let n = SampleHoldNode;
		let mut st: Box<dyn Any + Send> = Box::new(SampleHoldState::default());
		let ctx = sctx("s", None);
		let mut fired = ExecFireSet::new();
		fired.insert("sample_exec");
		n.compute(
			st.as_mut(),
			&InputMap::new(),
			&[("value".into(), SocketValue::Json(serde_json::json!("a")))].into_iter().collect(),
			&fired,
			&ctx,
		)
		.await
		.unwrap();
		let out = n
			.compute(
				st.as_mut(),
				&InputMap::new(),
				&[("value".into(), SocketValue::Json(serde_json::json!("b")))].into_iter().collect(),
				&ExecFireSet::new(),
				&ctx,
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("held").unwrap().as_json().unwrap(), &serde_json::json!("a"));
	}

	#[tokio::test]
	async fn debounce_emits_after_deadtime_via_trigger() {
		let n = DebounceNode;
		let mut st: Box<dyn Any + Send> = Box::new(DebounceState::default());
		let (handle, mut rx) = create_trigger_bus();
		let ctx = sctx("d", Some(&handle));

		let out0 = n
			.compute(
				st.as_mut(),
				&InputMap::new(),
				&[
					("value".into(), SocketValue::Json(serde_json::json!(1))),
					("deadtime_ms".into(), SocketValue::Int(30)),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
				&ctx,
			)
			.await
			.unwrap();
		assert_eq!(out0.data.get("value_out").unwrap().as_json().unwrap(), &JsonValue::Null);

		let ev = tokio::time::timeout(std::time::Duration::from_millis(500), rx.recv())
			.await
			.expect("timeout")
			.expect("recv");
		assert_eq!(ev.node_id, "d");

		let mut fired = ExecFireSet::new();
		fired.insert("__resume__");
		let out1 = n
			.compute(
				st.as_mut(),
				&InputMap::new(),
				&[
					("value".into(), SocketValue::Json(serde_json::json!(99))),
					("deadtime_ms".into(), SocketValue::Int(30)),
					("__pending_id__".into(), SocketValue::Int(0)),
				]
				.into_iter()
				.collect(),
				&fired,
				&ctx,
			)
			.await
			.unwrap();
		assert_eq!(out1.data.get("value_out").unwrap().as_json().unwrap(), &serde_json::json!(1));
	}

	#[tokio::test]
	async fn throttle_suppresses_rapid_change() {
		let n = ThrottleNode;
		let mut st: Box<dyn Any + Send> = Box::new(ThrottleState::default());
		let ctx = sctx("t", None);
		let inputs = |v: i64| {
			[
				("value".into(), SocketValue::Json(serde_json::json!(v))),
				("interval_ms".into(), SocketValue::Int(60_000)),
			]
			.into_iter()
			.collect::<InputMap>()
		};
		let o1 = n
			.compute(st.as_mut(), &InputMap::new(), &inputs(1), &ExecFireSet::new(), &ctx)
			.await
			.unwrap();
		assert_eq!(o1.data.get("value_out").unwrap().as_json().unwrap(), &serde_json::json!(1));
		let o2 = n
			.compute(st.as_mut(), &InputMap::new(), &inputs(2), &ExecFireSet::new(), &ctx)
			.await
			.unwrap();
		assert_eq!(o2.data.get("value_out").unwrap().as_json().unwrap(), &serde_json::json!(1));
	}

	#[tokio::test]
	async fn edge_detect_integration_timer_bool_changed_to_log() {
		let mut b = FlowgraphBuilder::new();
		b.add_node("ti", NodeImpl::stateful(Arc::new(TimerIntervalNode)), InputMap::new());
		b.add_node("bs", NodeImpl::stateful(Arc::new(BoolStateNode)), InputMap::new());
		b.add_node("ed", NodeImpl::stateful(Arc::new(EdgeDetectNode)), InputMap::new());
		b.add_node("log", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.add_node(
			"msg",
			NodeImpl::pure(Arc::new(StringLiteralNode)),
			[("value".to_string(), SocketValue::String("edge".into()))].into_iter().collect(),
		);
		b.connect_exec(PortRef::new("ti", "on_tick"), PortRef::new("bs", "toggle"));
		b.connect_exec(PortRef::new("bs", "changed"), PortRef::new("ed", "exec_in"));
		b.connect(PortRef::new("bs", "value"), PortRef::new("ed", "value"));
		b.connect_exec(PortRef::new("ed", "on_edge"), PortRef::new("log", "exec_in"));
		b.connect(PortRef::new("msg", "value"), PortRef::new("log", "value"));
		b.add_node(
			"iv",
			NodeImpl::pure(Arc::new(crate::flowgraph::nodes::literal::FloatLiteralNode)),
			[("value".to_string(), SocketValue::Float(0.08))].into_iter().collect(),
		);
		b.connect(PortRef::new("iv", "value"), PortRef::new("ti", "interval_sec"));

		let mut prog = b.build().expect("build");
		let mut ctx = crate::flowgraph::node::ExecCtx::default();
		let shutdown = tokio::time::sleep(Duration::from_millis(400));
		prog.run_forever(&mut ctx, shutdown).await.expect("run_forever");
		assert!(
			ctx.trace.iter().any(|l| l.contains("edge")),
			"expected log lines, got {:?}",
			ctx.trace
		);
	}
}
