//! Phase ρ: [`MotionFrame`](crate::motion::MotionFrame) から VMC **姿勢メッセージ**を Pure に取り出す。
//!
//! UDP 受信は `flowgraph.ingress.vmc_udp`（または `osc_udp`）→ `flowgraph.motion.vmc_parse` を前提とする。

use crate::flowgraph::node::{
	get_optional_string, get_required_motion_frame, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
	PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::flowgraph::vmc;
use async_trait::async_trait;

/// 最初の `/VMC/Ext/Bone/Pos` を JSON 化。`bone_name` が空でなければ骨名 **完全一致**で最初の 1 件。
pub struct VmcExtractBonePosNode;

impl NodeDescriptor for VmcExtractBonePosNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vmc.extract_bone_pos".into(),
			title: "VMC: Extract Bone Pos".into(),
			category: "vmc".into(),
			description: Some(
				"`MotionFrame` から `/VMC/Ext/Bone/Pos` を探し、`{ bone, position, rotation }` の JSON を返す。無ければ null".into(),
			),
			inputs: vec![
				PortSpec::input("frame", "Frame", SocketType::MotionFrame),
				PortSpec::input("bone_name", "Bone name (exact, optional)", SocketType::String)
					.with_default(SocketValue::String(String::new())),
			],
			outputs: vec![PortSpec::output("pose", "Pose (JSON)", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for VmcExtractBonePosNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let frame = get_required_motion_frame(inputs, "frame")?;
		let filter = get_optional_string(inputs, "bone_name", "")?;
		let pose = vmc::extract_first_bone_pos(&frame, &filter)
			.map(|s| s.to_json_value())
			.unwrap_or(serde_json::Value::Null);
		Ok(NodeOutput::new().set_data("pose", SocketValue::Json(pose)))
	}
}

/// 最初の `/VMC/Ext/Root/Pos` を JSON 化。
pub struct VmcExtractRootPosNode;

impl NodeDescriptor for VmcExtractRootPosNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vmc.extract_root_pos".into(),
			title: "VMC: Extract Root Pos".into(),
			category: "vmc".into(),
			description: Some(
				"`MotionFrame` から `/VMC/Ext/Root/Pos` を探し、`{ bone, position, rotation }` の JSON を返す。無ければ null".into(),
			),
			inputs: vec![PortSpec::input("frame", "Frame", SocketType::MotionFrame)],
			outputs: vec![PortSpec::output("pose", "Pose (JSON)", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for VmcExtractRootPosNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let frame = get_required_motion_frame(inputs, "frame")?;
		let pose = vmc::extract_first_root_pos(&frame)
			.map(|s| s.to_json_value())
			.unwrap_or(serde_json::Value::Null);
		Ok(NodeOutput::new().set_data("pose", SocketValue::Json(pose)))
	}
}

/// 最初の `/VMC/Ext/Blend/Val` を JSON 化。`blendshape_name` が空でなければ名前 **完全一致**で最初の 1 件。
pub struct VmcExtractBlendshapeNode;

impl NodeDescriptor for VmcExtractBlendshapeNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vmc.extract_blendshape".into(),
			title: "VMC: Extract BlendShape".into(),
			category: "vmc".into(),
			description: Some("`MotionFrame` から `/VMC/Ext/Blend/Val` を探し、`{ name, value }` の JSON を返す。無ければ null".into()),
			inputs: vec![
				PortSpec::input("frame", "Frame", SocketType::MotionFrame),
				PortSpec::input("blendshape_name", "BlendShape name (exact, optional)", SocketType::String)
					.with_default(SocketValue::String(String::new())),
			],
			outputs: vec![PortSpec::output("blendshape", "BlendShape (JSON)", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for VmcExtractBlendshapeNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let frame = get_required_motion_frame(inputs, "frame")?;
		let filter = get_optional_string(inputs, "blendshape_name", "")?;
		let sample = vmc::extract_first_blendshape(&frame, &filter)
			.map(|s| s.to_json_value())
			.unwrap_or(serde_json::Value::Null);
		Ok(NodeOutput::new().set_data("blendshape", SocketValue::Json(sample)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::PureEvalHost;
	use crate::flowgraph::vmc::{VMC_EXT_BLEND_VAL, VMC_EXT_BONE_POS};
	use crate::motion::{MotionFrame, OscMessageWire};
	use serde_json::json;

	#[tokio::test]
	async fn extract_bone_node_wire() {
		let frame = MotionFrame {
			byte_len: 0,
			osc_messages: vec![OscMessageWire {
				address: VMC_EXT_BONE_POS.into(),
				args: vec![
					json!("Head"),
					json!(1.0),
					json!(2.0),
					json!(3.0),
					json!(0.0),
					json!(0.0),
					json!(0.0),
					json!(1.0),
				],
			}],
		};
		let mut inputs = InputMap::new();
		inputs.insert("frame".into(), SocketValue::MotionFrame(frame));
		inputs.insert("bone_name".into(), SocketValue::String(String::new()));
		let out = VmcExtractBonePosNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		let v = out.data.get("pose").unwrap().as_json().unwrap();
		assert_eq!(v["bone"], "Head");
		assert_eq!(v["position"], json!([1.0, 2.0, 3.0]));
	}

	#[tokio::test]
	async fn extract_blendshape_node_wire() {
		let frame = MotionFrame {
			byte_len: 0,
			osc_messages: vec![OscMessageWire {
				address: VMC_EXT_BLEND_VAL.into(),
				args: vec![json!("Joy"), json!(0.5)],
			}],
		};
		let mut inputs = InputMap::new();
		inputs.insert("frame".into(), SocketValue::MotionFrame(frame));
		inputs.insert("blendshape_name".into(), SocketValue::String("Joy".into()));
		let out = VmcExtractBlendshapeNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		let v = out.data.get("blendshape").unwrap().as_json().unwrap();
		assert_eq!(v["name"], "Joy");
		assert_eq!(v["value"], json!(0.5));
	}
}
