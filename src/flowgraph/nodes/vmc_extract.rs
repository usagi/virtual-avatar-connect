//! Phase ρ: [`MotionFrame`](crate::motion::MotionFrame) から VMC **姿勢・表情メッセージ**を Pure に取り出す。
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
				"`MotionFrame` から `/VMC/Ext/Bone/Pos` を探し、JSON と型付きの `found/bone/px/py/pz/rx/ry/rz/rw` を返す。無ければ JSON は null、typed output は既定値。".into(),
			),
			inputs: vec![
				PortSpec::input("frame", "Frame", SocketType::MotionFrame),
				PortSpec::input("bone_name", "Bone name (exact, optional)", SocketType::String)
					.with_default(SocketValue::String(String::new())),
			],
			outputs: pose_outputs(),
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
		Ok(pose_output(vmc::extract_first_bone_pos(&frame, &filter)))
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
				"`MotionFrame` から `/VMC/Ext/Root/Pos` を探し、JSON と型付きの `found/bone/px/py/pz/rx/ry/rz/rw` を返す。無ければ JSON は null、typed output は既定値。".into(),
			),
			inputs: vec![PortSpec::input("frame", "Frame", SocketType::MotionFrame)],
			outputs: pose_outputs(),
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
		Ok(pose_output(vmc::extract_first_root_pos(&frame)))
	}
}

fn pose_outputs() -> Vec<PortSpec> {
	vec![
		PortSpec::output("pose", "Pose (JSON)", SocketType::Json),
		PortSpec::output("found", "Found", SocketType::Bool),
		PortSpec::output("bone", "Bone", SocketType::String),
		PortSpec::output("px", "Position X", SocketType::Float),
		PortSpec::output("py", "Position Y", SocketType::Float),
		PortSpec::output("pz", "Position Z", SocketType::Float),
		PortSpec::output("rx", "Rotation X", SocketType::Float),
		PortSpec::output("ry", "Rotation Y", SocketType::Float),
		PortSpec::output("rz", "Rotation Z", SocketType::Float),
		PortSpec::output("rw", "Rotation W", SocketType::Float),
	]
}

fn pose_output(sample: Option<vmc::VmcPosQuatSample>) -> NodeOutput {
	let Some(sample) = sample else {
		return NodeOutput::new()
			.set_data("pose", SocketValue::Json(serde_json::Value::Null))
			.set_data("found", SocketValue::Bool(false))
			.set_data("bone", SocketValue::String(String::new()))
			.set_data("px", SocketValue::Float(0.0))
			.set_data("py", SocketValue::Float(0.0))
			.set_data("pz", SocketValue::Float(0.0))
			.set_data("rx", SocketValue::Float(0.0))
			.set_data("ry", SocketValue::Float(0.0))
			.set_data("rz", SocketValue::Float(0.0))
			.set_data("rw", SocketValue::Float(1.0));
	};
	NodeOutput::new()
		.set_data("pose", SocketValue::Json(sample.to_json_value()))
		.set_data("found", SocketValue::Bool(true))
		.set_data("bone", SocketValue::String(sample.bone))
		.set_data("px", SocketValue::Float(sample.position[0]))
		.set_data("py", SocketValue::Float(sample.position[1]))
		.set_data("pz", SocketValue::Float(sample.position[2]))
		.set_data("rx", SocketValue::Float(sample.rotation[0]))
		.set_data("ry", SocketValue::Float(sample.rotation[1]))
		.set_data("rz", SocketValue::Float(sample.rotation[2]))
		.set_data("rw", SocketValue::Float(sample.rotation[3]))
}

/// 最初の `/VMC/Ext/Blend/Val` を抽出。`blendshape_name` が空でなければ名前 **完全一致**で最初の 1 件。
pub struct VmcExtractBlendshapeNode;

impl NodeDescriptor for VmcExtractBlendshapeNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vmc.extract_blendshape".into(),
			title: "VMC: Extract BlendShape".into(),
			category: "vmc".into(),
			description: Some("`MotionFrame` から `/VMC/Ext/Blend/Val` を探し、JSON と型付きの `found/name/value` を返す。無ければ JSON は null、typed output は既定値。".into()),
			inputs: vec![
				PortSpec::input("frame", "Frame", SocketType::MotionFrame),
				PortSpec::input("blendshape_name", "BlendShape name (exact, optional)", SocketType::String)
					.with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::output("blendshape", "BlendShape (JSON)", SocketType::Json),
				PortSpec::output("found", "Found", SocketType::Bool),
				PortSpec::output("name", "Name", SocketType::String),
				PortSpec::output("value", "Value", SocketType::Float),
			],
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
		let Some(sample) = vmc::extract_first_blendshape(&frame, &filter) else {
			return Ok(NodeOutput::new()
				.set_data("blendshape", SocketValue::Json(serde_json::Value::Null))
				.set_data("found", SocketValue::Bool(false))
				.set_data("name", SocketValue::String(String::new()))
				.set_data("value", SocketValue::Float(0.0)));
		};
		Ok(NodeOutput::new()
			.set_data("blendshape", SocketValue::Json(sample.to_json_value()))
			.set_data("found", SocketValue::Bool(true))
			.set_data("name", SocketValue::String(sample.name))
			.set_data("value", SocketValue::Float(sample.value)))
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
		assert_eq!(out.data.get("found").unwrap().as_bool().unwrap(), true);
		assert_eq!(out.data.get("bone").unwrap().as_str().unwrap(), "Head");
		assert_eq!(out.data.get("px").unwrap().as_f64().unwrap(), 1.0);
		assert_eq!(out.data.get("py").unwrap().as_f64().unwrap(), 2.0);
		assert_eq!(out.data.get("pz").unwrap().as_f64().unwrap(), 3.0);
		assert_eq!(out.data.get("rw").unwrap().as_f64().unwrap(), 1.0);
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
		assert_eq!(out.data.get("found").unwrap().as_bool().unwrap(), true);
		assert_eq!(out.data.get("name").unwrap().as_str().unwrap(), "Joy");
		assert_eq!(out.data.get("value").unwrap().as_f64().unwrap(), 0.5);
	}

	#[tokio::test]
	async fn extract_blendshape_node_missing_returns_typed_defaults() {
		let frame = MotionFrame {
			byte_len: 0,
			osc_messages: vec![OscMessageWire {
				address: VMC_EXT_BLEND_VAL.into(),
				args: vec![json!("Joy"), json!(0.5)],
			}],
		};
		let mut inputs = InputMap::new();
		inputs.insert("frame".into(), SocketValue::MotionFrame(frame));
		inputs.insert("blendshape_name".into(), SocketValue::String("Angry".into()));
		let out = VmcExtractBlendshapeNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!(out.data.get("blendshape").unwrap().as_json().unwrap().is_null());
		assert_eq!(out.data.get("found").unwrap().as_bool().unwrap(), false);
		assert_eq!(out.data.get("name").unwrap().as_str().unwrap(), "");
		assert_eq!(out.data.get("value").unwrap().as_f64().unwrap(), 0.0);
	}
}
