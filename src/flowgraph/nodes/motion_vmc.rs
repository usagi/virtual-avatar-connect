//! Phase M4: VMC 生ペイロード → OSC 解釈 → [`MotionFrame`](crate::motion::MotionFrame)（`motion_frame` ソケット）。
//!
//! `json` ポートとのエッジはランタイムで [`crate::flowgraph::socket::coerce_to_type`] により往復可能
//! （`byte_len` / `osc_messages` の JSON オブジェクト形状）。
//!
//! `flowgraph.ingress.vmc_udp` の `__content__`（Base64）をそのまま `payload_b64` に渡す既存経路と、
//! LF-5 の `bytes` payload を `payload` に渡す経路の両方を受け付ける。

use crate::flowgraph::node::{
	get_optional_float, get_optional_string, get_required_motion_frame, get_required_string, ExecFireSet, InputMap, NodeDescriptor,
	NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
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
				"UDP ペイロードを OSC として解釈し、[`MotionFrame`]（`motion_frame`）を出力。`payload` bytes が非空なら優先し、空なら互換用 `payload_b64` を読む。`json` へ接続時は自動変換"
					.into(),
			),
			inputs: vec![
				PortSpec::input("payload", "Payload", SocketType::Bytes).with_default(SocketValue::Bytes(Vec::new())),
				PortSpec::input("payload_b64", "Payload (Base64)", SocketType::String),
			],
			outputs: vec![PortSpec::output("frame", "Frame", SocketType::MotionFrame)],
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
		let raw = match inputs.get("payload") {
			Some(SocketValue::Bytes(bytes)) if !bytes.is_empty() => bytes.clone(),
			Some(SocketValue::Bytes(_)) | None => {
				let b64 = get_required_string(inputs, "payload_b64")?;
				STANDARD
					.decode(b64.trim())
					.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("payload_b64 decode: {e}")))?
			}
			Some(v) => {
				return Err(NodeExecError::TypeMismatch {
					port: "payload".into(),
					expected: SocketType::Bytes,
					actual: v.type_of(),
				});
			}
		};
		let frame = parse_vmc_payload(&raw).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		Ok(NodeOutput::new().set_data("frame", SocketValue::MotionFrame(frame)))
	}
}

// ---------------------------------------------------------------------
// flowgraph.motion.filter
// ---------------------------------------------------------------------

/// [`MotionFrame`] の `osc_messages` をアドレス条件で絞り込む。
pub struct MotionFilterNode;

impl NodeDescriptor for MotionFilterNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.motion.filter".into(),
			title: "Motion: Filter OSC Messages".into(),
			category: "motion".into(),
			description: Some(
				"`osc_messages` を `address_prefix` と `address_substring`（両方省略可）でフィルタする。入出力は `motion_frame`（`json` へ coerce 可）".into(),
			),
			inputs: vec![
				PortSpec::input("frame", "Frame", SocketType::MotionFrame),
				PortSpec::input("address_prefix", "Address prefix", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("address_substring", "Address substring", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![PortSpec::output("frame_out", "Filtered frame", SocketType::MotionFrame)],
			properties: vec![],
		}
	}
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
		let frame = get_required_motion_frame(inputs, "frame")?;
		let prefix = get_optional_string(inputs, "address_prefix", "")?;
		let substring = get_optional_string(inputs, "address_substring", "")?;
		let out = frame.filter_addresses(&prefix, &substring);
		Ok(NodeOutput::new().set_data("frame_out", SocketValue::MotionFrame(out)))
	}
}

// ---------------------------------------------------------------------
// flowgraph.motion.map
// ---------------------------------------------------------------------

/// `osc_messages[].args` 内の **JSON 数値**を再帰的に `float_scale` 倍する（ブレンドシェイプゲイン等）。
pub struct MotionMapNode;

impl NodeDescriptor for MotionMapNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.motion.map".into(),
			title: "Motion: Map Numeric Args".into(),
			category: "motion".into(),
			description: Some(
				"各 `osc_messages[].args` の JSON 数値を再帰的に `float_scale` 倍する。入出力は `motion_frame`（`json` へ coerce 可）"
					.into(),
			),
			inputs: vec![
				PortSpec::input("frame", "Frame", SocketType::MotionFrame),
				PortSpec::input("float_scale", "Scale", SocketType::Float).with_default(SocketValue::Float(1.0)),
			],
			outputs: vec![PortSpec::output("frame_out", "Mapped frame", SocketType::MotionFrame)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for MotionMapNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let frame = get_required_motion_frame(inputs, "frame")?;
		let scale = get_optional_float(inputs, "float_scale", 1.0)?;
		let out = frame
			.map_numeric_args(scale)
			.map_err(|msg| NodeExecError::Generic(anyhow::anyhow!("{msg}")))?;
		Ok(NodeOutput::new().set_data("frame_out", SocketValue::MotionFrame(out)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::PureEvalHost;
	use crate::motion::MotionFrame;
	use serde_json::json;

	fn frame_from_json(v: serde_json::Value) -> MotionFrame {
		serde_json::from_value(v).expect("fixture MotionFrame")
	}

	#[test]
	fn filter_keeps_matching_prefix() {
		let frame = frame_from_json(json!({
			"byte_len": 100,
			"osc_messages": [
				{"address": "/V/Blend/A", "args": [1.0]},
				{"address": "/other", "args": []},
			]
		}));
		let out = frame.filter_addresses("/V/", "");
		assert_eq!(out.osc_messages.len(), 1);
		assert_eq!(out.osc_messages[0].address, "/V/Blend/A");
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
		let f = out.data.get("frame").unwrap().as_motion_frame().unwrap();
		assert_eq!(f.byte_len, bytes.len());
		assert!(f.osc_messages.len() >= 1);
	}

	#[tokio::test]
	async fn parses_fixture_bytes_payload() {
		use rosc::encoder;
		use rosc::{OscMessage, OscPacket, OscType};
		let pkt = OscPacket::Message(OscMessage {
			addr: "/V/Blend/Proxy".into(),
			args: vec![OscType::Float(1.0)],
		});
		let bytes = encoder::encode(&pkt).unwrap();
		let mut inputs = InputMap::new();
		inputs.insert("payload".into(), SocketValue::Bytes(bytes.clone()));
		inputs.insert(
			"payload_b64".into(),
			SocketValue::String("invalid fallback should not be used".into()),
		);
		let out = VmcParseNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		let f = out.data.get("frame").unwrap().as_motion_frame().unwrap();
		assert_eq!(f.byte_len, bytes.len());
		assert!(f.osc_messages.len() >= 1);
	}

	#[tokio::test]
	async fn filter_node_wire() {
		let frame = frame_from_json(json!({
			"byte_len": 1,
			"osc_messages": [
				{"address": "/a", "args": []},
				{"address": "/b/x", "args": []},
			]
		}));
		let mut inputs = InputMap::new();
		inputs.insert("frame".into(), SocketValue::MotionFrame(frame));
		inputs.insert("address_prefix".into(), SocketValue::String("/b".into()));
		inputs.insert("address_substring".into(), SocketValue::String(String::new()));
		let out = MotionFilterNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		let f = out.data.get("frame_out").unwrap().as_motion_frame().unwrap();
		assert_eq!(f.osc_messages.len(), 1);
	}

	#[test]
	fn map_scales_args() {
		let frame = frame_from_json(json!({
			"byte_len": 2,
			"osc_messages": [
				{"address": "/x", "args": [2.0, [0.5, 1]]},
			]
		}));
		let out = frame.map_numeric_args(2.0).unwrap();
		let args = &out.osc_messages[0].args;
		assert_eq!(args[0], json!(4.0));
		assert_eq!(args[1][0], json!(1.0));
		assert_eq!(args[1][1], json!(2.0));
	}

	#[tokio::test]
	async fn map_node_wire() {
		let frame = frame_from_json(json!({
			"osc_messages": [{"address": "/v", "args": [10]}]
		}));
		let mut inputs = InputMap::new();
		inputs.insert("frame".into(), SocketValue::MotionFrame(frame));
		inputs.insert("float_scale".into(), SocketValue::Float(0.1));
		let out = MotionMapNode
			.compute(&PureEvalHost::default(), &InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		let f = out.data.get("frame_out").unwrap().as_motion_frame().unwrap();
		assert_eq!(f.osc_messages[0].args[0], json!(1.0));
	}
}
