//! Phase ρ 先取り: **単発 OSC メッセージ**を UDP で送出する Effectful ノード。
//!
//! 汎用 `flowgraph.osc.send`。VMC 専用ではない（アドレス・引数はプロパティ／入力で指定）。

use crate::flowgraph::node::{
	get_required_int, get_required_json, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor,
	NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use rosc::{encoder, OscArray, OscMessage, OscPacket, OscType};
use serde_json::Value as JsonValue;

fn json_to_osc_type(v: &JsonValue) -> Result<OscType, String> {
	match v {
		JsonValue::Null => Ok(OscType::Nil),
		JsonValue::Bool(b) => Ok(OscType::Bool(*b)),
		JsonValue::Number(n) => {
			if let Some(i) = n.as_i64() {
				if i >= i64::from(i32::MIN) && i <= i64::from(i32::MAX) {
					return Ok(OscType::Int(i as i32));
				}
				return Ok(OscType::Long(i));
			}
			let f = n.as_f64().ok_or_else(|| "number".to_string())?;
			Ok(OscType::Double(f))
		}
		JsonValue::String(s) => Ok(OscType::String(s.clone())),
		JsonValue::Array(a) => {
			let inner: Result<Vec<OscType>, String> = a.iter().map(json_to_osc_type).collect();
			Ok(OscType::Array(OscArray { content: inner? }))
		}
		other => Err(format!("OSC 引数として未対応の JSON: {other}")),
	}
}

fn err_out(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("bytes_sent", SocketValue::Int(0))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

pub struct OscSendNode;

impl NodeDescriptor for OscSendNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.osc.send".into(),
			title: "OSC: UDP Send".into(),
			category: "osc".into(),
			description: Some(
				"単一 OSC メッセージを UDP で送信する。args は JSON 配列（数値・文字列・真偽・null・ネスト配列）".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("path", "OSC Address", SocketType::String),
				PortSpec::input("args", "Arguments (JSON array)", SocketType::Json),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("bytes_sent", "Bytes Sent", SocketType::Int),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for OscSendNode {
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
		if port <= 0 || port > u16::MAX as i64 {
			return Ok(err_out(format!("port が不正です: {port}")));
		}
		let path = get_required_string(inputs, "path")?;
		if !path.starts_with('/') {
			return Ok(err_out("OSC address は '/' で始まる必要があります".to_string()));
		}
		let args_json = get_required_json(inputs, "args")?;
		let arr = args_json
			.as_array()
			.ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("args は JSON 配列である必要があります")))?;
		let mut osc_args = Vec::with_capacity(arr.len());
		for v in arr {
			let t = json_to_osc_type(v).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
			osc_args.push(t);
		}
		let msg = OscMessage {
			addr: path,
			args: osc_args,
		};
		let packet = OscPacket::Message(msg);
		let bytes = encoder::encode(&packet).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("OSC encode: {e}")))?;
		let sock = tokio::net::UdpSocket::bind("0.0.0.0:0")
			.await
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("UDP bind: {e}")))?;
		let port_u16 = port as u16;
		let mut targets = tokio::net::lookup_host((host.trim(), port_u16))
			.await
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("DNS lookup: {e}")))?;
		let addr = match targets.next() {
			Some(a) => a,
			None => return Ok(err_out(format!("ホスト解決結果が空です: {host}"))),
		};
		let n = sock
			.send_to(&bytes, addr)
			.await
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("UDP send_to: {e}")))?;
		Ok(NodeOutput::new()
			.set_data("bytes_sent", SocketValue::Int(n as i64))
			.set_data("error", SocketValue::String(String::new()))
			.fire_exec("on_success"))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn no_fire_is_noop() {
		let node = OscSendNode;
		let mut ctx = ExecCtx::default();
		let out = node.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
	}
}
