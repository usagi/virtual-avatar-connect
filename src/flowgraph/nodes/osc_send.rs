//! Phase ρ 先取り: **単発 OSC メッセージ**を UDP で送出する Effectful ノード。
//!
//! 汎用 `flowgraph.osc.send`。VMC 専用ではない（アドレス・引数はプロパティ／入力で指定）。
//! エンコード・送信の本体は [`crate::flowgraph::osc`]。

use crate::flowgraph::node::{
	get_required_int, get_required_json, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor,
	NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::osc;
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

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
		let path = get_required_string(inputs, "path")?;
		let args_json = get_required_json(inputs, "args")?;
		let arr = args_json
			.as_array()
			.ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("args は JSON 配列である必要があります")))?;
		match osc::send_osc_udp_json_args(host.trim(), port, path.trim(), arr).await {
			Ok(n) => Ok(NodeOutput::new()
				.set_data("bytes_sent", SocketValue::Int(n as i64))
				.set_data("error", SocketValue::String(String::new()))
				.fire_exec("on_success")),
			Err(e) => Ok(err_out(e)),
		}
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
