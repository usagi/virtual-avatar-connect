//! Phase ρ: VMC Protocol の **姿勢（位置 + 四元数）** を単発 UDP で送出する Effectful ノード。
//!
//! [`crate::flowgraph::vmc`] が OSC を組み立てる。全骨のストリーミングはグラフ側で複数回 exec する想定。

use crate::flowgraph::node::{
	get_required_int, get_required_json, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor,
	NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use crate::flowgraph::vmc::{self, VMC_EXT_BONE_POS, VMC_EXT_ROOT_POS};
use async_trait::async_trait;

fn err_out(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("bytes_sent", SocketValue::Int(0))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code("vmc.send")))
		.fire_exec("on_error")
}

fn send_outputs() -> Vec<PortSpec> {
	vec![
		PortSpec::exec_output("on_success", "On Success"),
		PortSpec::exec_output("on_error", "On Error"),
		PortSpec::output("bytes_sent", "Bytes Sent", SocketType::Int),
		PortSpec::output("error", "Error", SocketType::String),
		PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Int))),
	]
}

fn success_out(bytes_sent: usize) -> NodeOutput {
	NodeOutput::new()
		.set_data("bytes_sent", SocketValue::Int(bytes_sent as i64))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Int(bytes_sent as i64))))
		.fire_exec("on_success")
}

fn read_pos_rot(inputs: &InputMap) -> Result<([f64; 3], [f64; 4]), NodeExecError> {
	let pos_j = get_required_json(inputs, "position")?;
	let rot_j = get_required_json(inputs, "rotation")?;
	let pos = vmc::parse_json_vec3("position", pos_j).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
	let rot = vmc::parse_json_quat("rotation", rot_j).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
	Ok((pos, rot))
}

/// [`VMC_EXT_BONE_POS`] — `bone_name` は Unity `HumanBodyBones` 名（例: `Head`）。
pub struct VmcSendBonePosNode;

impl NodeDescriptor for VmcSendBonePosNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vmc.send_bone_pos".into(),
			title: "VMC: Send Bone Pos".into(),
			category: "vmc".into(),
			description: Some(
				"VMC `/VMC/Ext/Bone/Pos` を 1 回 UDP 送信。`position` は [x,y,z]、`rotation` は [qx,qy,qz,qw] の JSON 配列".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("bone_name", "Bone name", SocketType::String),
				PortSpec::input("position", "Position [x,y,z]", SocketType::Json),
				PortSpec::input("rotation", "Rotation [qx,qy,qz,qw]", SocketType::Json),
			],
			outputs: send_outputs(),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for VmcSendBonePosNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let host = get_required_string(inputs, "host")?;
		let port = get_required_int(inputs, "port")?;
		let bone = get_required_string(inputs, "bone_name")?;
		let (pos, rot) = read_pos_rot(inputs)?;
		match vmc::send_vmc_transform_pos_udp(host.trim(), port, VMC_EXT_BONE_POS, bone.trim(), pos, rot).await {
			Ok(n) => Ok(success_out(n)),
			Err(e) => Ok(err_out(e)),
		}
	}
}

/// [`VMC_EXT_ROOT_POS`] — 先頭引数は常に `"root"`（仕様）。
pub struct VmcSendRootPosNode;

impl NodeDescriptor for VmcSendRootPosNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vmc.send_root_pos".into(),
			title: "VMC: Send Root Pos".into(),
			category: "vmc".into(),
			description: Some(
				"VMC `/VMC/Ext/Root/Pos` を 1 回 UDP 送信（骨名は常に `root`）。`position` / `rotation` は JSON 配列".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("position", "Position [x,y,z]", SocketType::Json),
				PortSpec::input("rotation", "Rotation [qx,qy,qz,qw]", SocketType::Json),
			],
			outputs: send_outputs(),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for VmcSendRootPosNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let host = get_required_string(inputs, "host")?;
		let port = get_required_int(inputs, "port")?;
		let (pos, rot) = read_pos_rot(inputs)?;
		match vmc::send_vmc_transform_pos_udp(host.trim(), port, VMC_EXT_ROOT_POS, "root", pos, rot).await {
			Ok(n) => Ok(success_out(n)),
			Err(e) => Ok(err_out(e)),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn fired_exec() -> ExecFireSet {
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		fired
	}

	fn inputs(port: i64) -> InputMap {
		[
			("host".to_string(), SocketValue::String("127.0.0.1".into())),
			("port".to_string(), SocketValue::Int(port)),
			("bone_name".to_string(), SocketValue::String("Head".into())),
			("position".to_string(), SocketValue::Json(json!([0.0, 1.0, 2.0]))),
			("rotation".to_string(), SocketValue::Json(json!([0.0, 0.0, 0.0, 1.0]))),
		]
		.into_iter()
		.collect()
	}

	#[tokio::test]
	async fn bone_no_fire_is_noop() {
		let n = VmcSendBonePosNode;
		let mut ctx = ExecCtx::default();
		let out = n.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[test]
	fn vmc_send_outputs_include_result() {
		let outputs = send_outputs();
		let result = outputs.iter().find(|p| p.name == "result").expect("result output");
		assert_eq!(result.ty, SocketType::Result(Box::new(SocketType::Int)));
	}

	#[tokio::test]
	async fn bone_send_emits_result_for_success() {
		let n = VmcSendBonePosNode;
		let mut ctx = ExecCtx::default();
		let out = n
			.execute(&mut ctx, &InputMap::new(), &inputs(9), &fired_exec())
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_success"));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert!(matches!(result.value.as_deref(), Some(SocketValue::Int(n)) if *n > 0));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}

	#[tokio::test]
	async fn bone_send_emits_result_for_error() {
		let n = VmcSendBonePosNode;
		let mut ctx = ExecCtx::default();
		let out = n
			.execute(&mut ctx, &InputMap::new(), &inputs(0), &fired_exec())
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		assert_eq!(out.data.get("bytes_sent"), Some(&SocketValue::Int(0)));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("vmc.send"));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}
}
