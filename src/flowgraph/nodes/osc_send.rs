//! Phase ρ 先取り: **単発 OSC メッセージ**を UDP で送出する Effectful ノード。
//!
//! 汎用 `flowgraph.osc.send`。VMC 専用ではない（アドレス・引数はプロパティ／入力で指定）。
//! エンコード・送信の本体は [`crate::flowgraph::osc`]。

use crate::flowgraph::node::{
	get_required_int, get_required_json, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor,
	NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::osc;
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use async_trait::async_trait;

fn err_out(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("bytes_sent", SocketValue::Int(0))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code("osc.send")))
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
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Int))),
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
				.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Int(n as i64))))
				.fire_exec("on_success")),
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

	fn inputs(path: &str) -> InputMap {
		[
			("host".into(), SocketValue::String("127.0.0.1".into())),
			("port".into(), SocketValue::Int(9)),
			("path".into(), SocketValue::String(path.into())),
			("args".into(), SocketValue::Json(json!([null]))),
		]
		.into_iter()
		.collect()
	}

	#[tokio::test]
	async fn no_fire_is_noop() {
		let node = OscSendNode;
		let mut ctx = ExecCtx::default();
		let out = node.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[tokio::test]
	async fn osc_send_emits_result_for_success() {
		let node = OscSendNode;
		let mut ctx = ExecCtx::default();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs("/vac/test"), &fired_exec()).await.unwrap();
		assert!(out.fired_exec.contains("on_success"));
		assert_eq!(out.data.get("error"), Some(&SocketValue::String(String::new())));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert!(matches!(result.value.as_deref(), Some(SocketValue::Int(n)) if *n > 0));
			}
			_ => panic!("expected Result"),
		}
	}

	#[tokio::test]
	async fn osc_send_emits_result_for_error() {
		let node = OscSendNode;
		let mut ctx = ExecCtx::default();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs("not/slash"), &fired_exec()).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		assert_eq!(out.data.get("bytes_sent"), Some(&SocketValue::Int(0)));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("osc.send"));
				assert!(result.error.as_deref().unwrap_or_default().contains('/'));
			}
			_ => panic!("expected Result"),
		}
	}
}
