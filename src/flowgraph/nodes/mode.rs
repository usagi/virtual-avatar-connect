//! RM-2: Runtime Mode の観測（Pure）。`PureEvalHost` 経由で実効 mode id を読む。

use crate::flowgraph::node::{
	get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureEvalHost,
	PureNode,
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
		Ok(NodeOutput::new().set_data(
			"mode",
			SocketValue::String(host.effective_runtime_mode_id()),
		))
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
				"実効 Runtime Mode が expected と一致するか（前後空白は無視）。未上書き時は default_runtime_mode 相当と比較。"
					.into(),
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::{ExecFireSet, InputMap};

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
		let mut inputs: InputMap = [("expected".into(), SocketValue::String("work".into()))]
			.into_iter()
			.collect();
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
}
