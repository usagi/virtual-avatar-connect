//! Phase M4: VMC 生ペイロード → OSC 解釈 → JSON（[`MotionFrameV0`](crate::motion::MotionFrameV0)）。
//!
//! `flowgraph.ingress.vmc_udp` の `__content__`（Base64）をそのまま `payload_b64` に渡す想定。

use crate::flowgraph::node::{
	get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::motion::parse_vmc_payload;
use async_trait::async_trait;
use base64::{engine::general_purpose::STANDARD, Engine as _};

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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::PureEvalHost;

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
}
