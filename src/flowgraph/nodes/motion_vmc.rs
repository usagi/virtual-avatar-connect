//! Phase M4: VMC 生ペイロード → OSC 解釈 → JSON（[`MotionFrameV0`](crate::motion::MotionFrameV0)）。
//!
//! `flowgraph.ingress.vmc_udp` の `__content__`（Base64）をそのまま `payload_b64` に渡す想定。

use crate::flowgraph::node::{
	get_optional_string, get_required_json, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError,
	NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::motion::parse_vmc_payload;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};
use serde_json::Value as JsonValue;

pub struct VmcParseNode;

impl NodeDescriptor for VmcParseNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.motion.vmc_parse".into(),
			title: "Motion: VMC OSC Parse".into(),
			category: "motion".into(),
			description: Some(
				"Base64 された UDP ペイロードを OSC として解釈し、address/args を JSON にまとめる（Phase M4 v0）".into(),
			),
			inputs: vec![PortSpec::input("payload_b64", "Payload (Base64)", SocketType::String)],
			outputs: vec![PortSpec::output("frame", "Frame (JSON)", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for VmcParseNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let b64 = get_required_string(inputs, "payload_b64")?;
		let raw = STANDARD
			.decode(b64.trim())
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("payload_b64 decode: {e}")))?;
		let frame = parse_vmc_payload(&raw).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		Ok(NodeOutput::new().set_data("frame", SocketValue::Json(frame.to_json_value())))
	}
}

// ---------------------------------------------------------------------
// flowgraph.motion.filter
// ---------------------------------------------------------------------

/// `flowgraph.motion.vmc_parse` の `frame` JSON から、`osc_messages[]` をアドレス条件で絞り込む。
pub struct MotionFilterNode;

impl NodeDescriptor for MotionFilterNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.motion.filter".into(),
			title: "Motion: Filter OSC Messages".into(),
			category: "motion".into(),
			description: Some(
				"`vmc_parse` の frame JSON の `osc_messages` を、`address_prefix` と `address_substring`（両方省略可）でフィルタする".into(),
			),
			inputs: vec![
				PortSpec::input("frame", "Frame (JSON)", SocketType::Json),
				PortSpec::input("address_prefix", "Address prefix", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("address_substring", "Address substring", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![PortSpec::output("frame_out", "Filtered frame (JSON)", SocketType::Json)],
			properties: vec![],
		}
	}
}

fn filter_motion_frame_json(frame: &JsonValue, prefix: &str, substring: &str) -> Result<JsonValue, NodeExecError> {
	let obj = frame
		.as_object()
		.ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("frame は JSON オブジェクトである必要があります")))?;
	let arr = obj
		.get("osc_messages")
		.and_then(|v| v.as_array())
		.ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("frame に osc_messages 配列が必要です（vmc_parse の出力形式）")))?;
	let prefix = prefix.trim();
	let substring = substring.trim();
	let filtered: Vec<JsonValue> = arr
		.iter()
		.filter(|m| {
			let addr = m.get("address").and_then(|v| v.as_str()).unwrap_or("");
			let pass_p = prefix.is_empty() || addr.starts_with(prefix);
			let pass_s = substring.is_empty() || addr.contains(substring);
			pass_p && pass_s
		})
		.cloned()
		.collect();
	let mut out = frame.clone();
	if let Some(o) = out.as_object_mut() {
		o.insert("osc_messages".into(), JsonValue::Array(filtered));
	}
	Ok(out)
}

#[async_trait]
impl PureNode for MotionFilterNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let frame = get_required_json(inputs, "frame")?;
		let prefix = get_optional_string(inputs, "address_prefix", "")?;
		let substring = get_optional_string(inputs, "address_substring", "")?;
		let out = filter_motion_frame_json(frame, &prefix, &substring)?;
		Ok(NodeOutput::new().set_data("frame_out", SocketValue::Json(out)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::PureEvalHost;
	use serde_json::json;

	#[test]
	fn filter_keeps_matching_prefix() {
		let frame = json!({
			"byte_len": 100,
			"osc_messages": [
				{"address": "/V/Blend/A", "args": [1.0]},
				{"address": "/other", "args": []},
			]
		});
		let out = filter_motion_frame_json(&frame, "/V/", "").unwrap();
		let arr = out["osc_messages"].as_array().unwrap();
		assert_eq!(arr.len(), 1);
		assert_eq!(arr[0]["address"], "/V/Blend/A");
	}

	#[tokio::test]
	async fn parses_fixture_b64() {
		use rosc::encoder;
		use rosc::{OscMessage, OscPacket, OscType};
		let pkt = OscPacket::Message(OscMessage {
			addr: "/V/Blend/Proxy".into(),
			args: vec![OscType::Float(1.0)],
		});
		let bytes = encoder::encode(&pkt).unwrap();
		let b64 = STANDARD.encode(&bytes);
		let mut inputs = InputMap::new();
		inputs.insert("payload_b64".into(), SocketValue::String(b64));
		let out = VmcParseNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		let v = out.data.get("frame").unwrap().as_json().unwrap();
		assert_eq!(v["byte_len"], bytes.len());
		assert!(v["osc_messages"].as_array().unwrap().len() >= 1);
	}

	#[tokio::test]
	async fn filter_node_wire() {
		let frame = json!({
			"byte_len": 1,
			"osc_messages": [
				{"address": "/a", "args": []},
				{"address": "/b/x", "args": []},
			]
		});
		let mut inputs = InputMap::new();
		inputs.insert("frame".into(), SocketValue::Json(frame));
		inputs.insert("address_prefix".into(), SocketValue::String("/b".into()));
		inputs.insert("address_substring".into(), SocketValue::String(String::new()));
		let out = MotionFilterNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		let v = out.data.get("frame_out").unwrap().as_json().unwrap();
		assert_eq!(v["osc_messages"].as_array().unwrap().len(), 1);
	}
}
