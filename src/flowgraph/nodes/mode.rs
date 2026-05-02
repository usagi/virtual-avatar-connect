//! RM-2 / RM-5: Runtime Mode の観測（Pure）と遷移要求（Effectful）。

use crate::flowgraph::node::{
	get_optional_string, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput,
	NodeSpec, PortSpec, PropertySpec, PureEvalHost, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct ModeGetNode;

impl NodeDescriptor for ModeGetNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.mode.get".into(),
			title: "Mode Get".into(),
			category: "mode".into(),
			description: Some(
				"現在の実効 Runtime Mode ID。Control API で上書きした値。未上書き時は conf.default_runtime_mode に従う（空のことあり）。"
					.into(),
			),
			inputs: vec![],
			outputs: vec![PortSpec::output("mode", "Mode", SocketType::String)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for ModeGetNode {
	async fn compute(
		&self,
		host: &PureEvalHost,
		_props: &InputMap,
		_inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		Ok(NodeOutput::new().set_data("mode", SocketValue::String(host.effective_runtime_mode_id())))
	}
}

pub struct ModeEqualsNode;

impl NodeDescriptor for ModeEqualsNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.mode.equals".into(),
			title: "Mode Equals".into(),
			category: "mode".into(),
			description: Some(
				"実効 Runtime Mode が expected と一致するか（前後空白は無視）。未上書き時は default_runtime_mode 相当と比較。".into(),
			),
			inputs: vec![PortSpec::input("expected", "Expected", SocketType::String)],
			outputs: vec![PortSpec::output("equals", "Equals", SocketType::Bool)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for ModeEqualsNode {
	async fn compute(
		&self,
		host: &PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let expected = get_required_string(inputs, "expected")?;
		let cur = host.effective_runtime_mode_id();
		let eq = cur.trim() == expected.trim();
		Ok(NodeOutput::new().set_data("equals", SocketValue::Bool(eq)))
	}
}

pub struct ModeTransitNode;

impl NodeDescriptor for ModeTransitNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.mode.transit".into(),
			title: "Mode Transit".into(),
			category: "mode".into(),
			description: Some(
				"指定 mode へ Runtime Mode を切り替える（`State.runtime_mode_id` + TriggerGate 再計算）。\
				 `mode` が空なら default_runtime_mode 相当。`State` 未接続・conf 再読込失敗・未知 mode では on_reject。"
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("mode", "Mode", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("reason", "Reason", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "On Success"),
				PortSpec::exec_output("on_reject", "On Reject"),
				PortSpec::output("accepted", "Accepted", SocketType::Bool),
				PortSpec::output("message", "Message", SocketType::String),
			],
			properties: vec![PropertySpec::new(
				"noop_message",
				"No-op Message",
				SocketType::String,
				SocketValue::String("noop".into()),
			)
			.description("実効 mode が変わらなかったときの `message` 文字列。")],
		}
	}

	fn control_triggerable(&self) -> bool {
		false
	}
}

#[async_trait]
impl EffectfulNode for ModeTransitNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		properties: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}

		let mode_raw = get_required_string(inputs, "mode")?;
		let normalized: Option<String> = {
			let t = mode_raw.trim();
			if t.is_empty() {
				None
			} else {
				Some(t.to_string())
			}
		};
		let reason_s = get_optional_string(inputs, "reason", "")?;
		let reason_opt = {
			let t = reason_s.trim();
			if t.is_empty() {
				None
			} else {
				Some(t.to_string())
			}
		};

		let noop_msg = properties
			.get("noop_message")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("noop")
			.to_string();

		let Some(weak) = ctx.state_handle.as_ref() else {
			ctx.log("mode.transit: state_handle なし → on_reject");
			return Ok(NodeOutput::new()
				.set_data("accepted", SocketValue::Bool(false))
				.set_data("message", SocketValue::String("state_handle_unset".into()))
				.fire_exec("on_reject"));
		};
		let Some(state_arc) = weak.upgrade() else {
			ctx.log("mode.transit: State が drop 済み → on_reject");
			return Ok(NodeOutput::new()
				.set_data("accepted", SocketValue::Bool(false))
				.set_data("message", SocketValue::String("state_dropped".into()))
				.fire_exec("on_reject"));
		};

		let path = { state_arc.read().await.conf_source_path.clone() };
		let Some(path) = path else {
			ctx.log("mode.transit: conf_source_path なし → on_reject");
			return Ok(NodeOutput::new()
				.set_data("accepted", SocketValue::Bool(false))
				.set_data("message", SocketValue::String("conf_source_path_unset".into()))
				.fire_exec("on_reject"));
		};

		let conf = match crate::conf::Conf::new_noop_probe(&path) {
			Ok(c) => c,
			Err(e) => {
				ctx.log(format!("mode.transit: conf 再読込失敗: {e}"));
				return Ok(NodeOutput::new()
					.set_data("accepted", SocketValue::Bool(false))
					.set_data("message", SocketValue::String(format!("conf_load_failed: {e}")))
					.fire_exec("on_reject"));
			}
		};

		let current_slot = {
			let g = state_arc.read().await;
			g.runtime_mode_id.read().ok().and_then(|x| x.clone())
		};
		let plan = match crate::conf::build_mode_transition_plan(&conf, current_slot.as_deref(), normalized.as_deref()) {
			Ok(p) => p,
			Err(msg) => {
				ctx.log(format!("mode.transit: plan 失敗: {msg}"));
				return Ok(NodeOutput::new()
					.set_data("accepted", SocketValue::Bool(false))
					.set_data("message", SocketValue::String(format!("plan_failed: {msg}")))
					.fire_exec("on_reject"));
			}
		};

		let apply_result = if plan.noop {
			crate::state::apply_runtime_mode_change(&state_arc, &conf, normalized.clone(), reason_opt).await
		} else {
			let Some(_guard) = crate::state::try_begin_runtime_mode_transition(&state_arc).await else {
				ctx.log("mode.transit: transition_busy → on_reject");
				return Ok(NodeOutput::new()
					.set_data("accepted", SocketValue::Bool(false))
					.set_data("message", SocketValue::String("transition_busy".into()))
					.fire_exec("on_reject"));
			};
			crate::state::apply_runtime_mode_transition_full(&state_arc, &conf, normalized.clone(), reason_opt)
				.await
				.map(|o| o.applied)
		};

		match apply_result {
			Ok(applied) => {
				let msg = if applied.noop { noop_msg } else { "ok".into() };
				ctx.log(format!("mode.transit: accepted mode_slot={:?}", applied.mode_slot));
				Ok(NodeOutput::new()
					.set_data("accepted", SocketValue::Bool(true))
					.set_data("message", SocketValue::String(msg))
					.fire_exec("exec_out"))
			}
			Err(e) => {
				ctx.log(format!("mode.transit: reject {e}"));
				Ok(NodeOutput::new()
					.set_data("accepted", SocketValue::Bool(false))
					.set_data("message", SocketValue::String(e.to_string()))
					.fire_exec("on_reject"))
			}
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::{ExecCtx, ExecFireSet, InputMap};

	#[tokio::test]
	async fn mode_get_uses_effective_id() {
		let host = PureEvalHost {
			runtime_mode: None,
			default_runtime_mode: Some("daily".into()),
		};
		let out = ModeGetNode
			.compute(&host, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("mode"), Some(&SocketValue::String("daily".into())));
	}

	#[tokio::test]
	async fn mode_equals_trims_and_slot_wins() {
		let slot = std::sync::Arc::new(std::sync::RwLock::new(Some(" work ".into())));
		let host = PureEvalHost {
			runtime_mode: Some(slot),
			default_runtime_mode: Some("daily".into()),
		};
		let mut inputs: InputMap = [("expected".into(), SocketValue::String("work".into()))].into_iter().collect();
		let out = ModeEqualsNode
			.compute(&host, &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("equals"), Some(&SocketValue::Bool(true)));

		inputs.insert("expected".into(), SocketValue::String("daily".into()));
		let out2 = ModeEqualsNode
			.compute(&host, &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out2.data.get("equals"), Some(&SocketValue::Bool(false)));
	}

	#[tokio::test]
	async fn mode_transit_without_state_fires_on_reject() {
		let node = ModeTransitNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let inputs: InputMap = [("mode".into(), SocketValue::String("daily".into()))].into_iter().collect();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_reject"));
		assert!(!out.fired_exec.contains("exec_out"));
		assert_eq!(out.data.get("accepted"), Some(&SocketValue::Bool(false)));
	}
}
