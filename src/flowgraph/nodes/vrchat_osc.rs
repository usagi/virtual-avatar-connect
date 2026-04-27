//! Phase ρ: **VRChat** 向け OSC（Avatar Parameters / Chatbox）を UDP で送出する Effectful ノード。
//!
//! 仕様: VRChat 公式 OSC ドキュメント。`host` / `port` は例: `127.0.0.1` + VRChat の受信ポート（環境依存）。

use crate::flowgraph::node::{
	get_optional_bool, get_required_bool, get_required_float, get_required_int, get_required_string, EffectfulNode, ExecCtx,
	ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use crate::flowgraph::vrchat;
use async_trait::async_trait;

fn err_out(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("bytes_sent", SocketValue::Int(0))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

async fn send_encoded(host: &str, port: i64, bytes: Vec<u8>) -> Result<NodeOutput, NodeExecError> {
	match vrchat::send_vrchat_osc(host.trim(), port, &bytes).await {
		Ok(n) => Ok(NodeOutput::new()
			.set_data("bytes_sent", SocketValue::Int(n as i64))
			.set_data("error", SocketValue::String(String::new()))
			.fire_exec("on_success")),
		Err(e) => Ok(err_out(e)),
	}
}

/// `/avatar/parameters/{name}` に **Float** を 1 つ送る。
pub struct VrchatAvatarParameterFloatNode;

impl NodeDescriptor for VrchatAvatarParameterFloatNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vrchat.avatar_parameter_float".into(),
			title: "VRChat: Avatar Parameter (Float)".into(),
			category: "vrchat".into(),
			description: Some(
				"OSC `/avatar/parameters/<name>` に float を 1 つ送信（VRChat OSC Avatar Parameters）".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("parameter_name", "Parameter name", SocketType::String),
				PortSpec::input("value", "Value", SocketType::Float),
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
impl EffectfulNode for VrchatAvatarParameterFloatNode {
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
		let name = get_required_string(inputs, "parameter_name")?;
		let v = get_required_float(inputs, "value")?;
		let path = vrchat::avatar_parameter_address(name.trim())
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		let bytes = vrchat::encode_avatar_parameter_float(&path, v as f32)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		send_encoded(&host, port, bytes).await
	}
}

/// `/avatar/parameters/{name}` に **Int** を 1 つ送る。
pub struct VrchatAvatarParameterIntNode;

impl NodeDescriptor for VrchatAvatarParameterIntNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vrchat.avatar_parameter_int".into(),
			title: "VRChat: Avatar Parameter (Int)".into(),
			category: "vrchat".into(),
			description: Some("OSC `/avatar/parameters/<name>` に int を 1 つ送信".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("parameter_name", "Parameter name", SocketType::String),
				PortSpec::input("value", "Value", SocketType::Int),
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
impl EffectfulNode for VrchatAvatarParameterIntNode {
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
		let name = get_required_string(inputs, "parameter_name")?;
		let v = get_required_int(inputs, "value")?;
		let path = vrchat::avatar_parameter_address(name.trim())
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		let bytes = vrchat::encode_avatar_parameter_int(&path, v)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		send_encoded(&host, port, bytes).await
	}
}

/// `/avatar/parameters/{name}` に **Bool** を 1 つ送る。
pub struct VrchatAvatarParameterBoolNode;

impl NodeDescriptor for VrchatAvatarParameterBoolNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vrchat.avatar_parameter_bool".into(),
			title: "VRChat: Avatar Parameter (Bool)".into(),
			category: "vrchat".into(),
			description: Some("OSC `/avatar/parameters/<name>` に bool を 1 つ送信".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("parameter_name", "Parameter name", SocketType::String),
				PortSpec::input("value", "Value", SocketType::Bool),
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
impl EffectfulNode for VrchatAvatarParameterBoolNode {
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
		let name = get_required_string(inputs, "parameter_name")?;
		let v = get_required_bool(inputs, "value")?;
		let path = vrchat::avatar_parameter_address(name.trim())
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		let bytes = vrchat::encode_avatar_parameter_bool(&path, v)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		send_encoded(&host, port, bytes).await
	}
}

/// `/chatbox/input` — 文字列 + 即送信 + 通知 SE（公式 3 引数）。
pub struct VrchatChatboxInputNode;

impl NodeDescriptor for VrchatChatboxInputNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vrchat.chatbox_input".into(),
			title: "VRChat: Chatbox Input".into(),
			category: "vrchat".into(),
			description: Some(
				"`/chatbox/input` に (text, send_immediately, play_notification_sfx)。テキストは最大 144 文字に切り詰め".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("text", "Text", SocketType::String),
				PortSpec::input("send_immediately", "Send immediately", SocketType::Bool).with_default(SocketValue::Bool(true)),
				PortSpec::input("play_notification_sfx", "Play notification SFX", SocketType::Bool).with_default(SocketValue::Bool(true)),
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
impl EffectfulNode for VrchatChatboxInputNode {
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
		let text = get_required_string(inputs, "text")?;
		let send_now = get_optional_bool(inputs, "send_immediately", true)?;
		let notify = get_optional_bool(inputs, "play_notification_sfx", true)?;
		let bytes = vrchat::encode_chatbox_input(&text, send_now, notify)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		send_encoded(&host, port, bytes).await
	}
}

/// `/chatbox/typing` — bool。
pub struct VrchatChatboxTypingNode;

impl NodeDescriptor for VrchatChatboxTypingNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.vrchat.chatbox_typing".into(),
			title: "VRChat: Chatbox Typing".into(),
			category: "vrchat".into(),
			description: Some("`/chatbox/typing` に bool を 1 つ送信".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("host", "Host", SocketType::String),
				PortSpec::input("port", "Port", SocketType::Int),
				PortSpec::input("typing", "Typing on", SocketType::Bool),
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
impl EffectfulNode for VrchatChatboxTypingNode {
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
		let on = get_required_bool(inputs, "typing")?;
		let bytes = vrchat::encode_chatbox_typing(on).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		send_encoded(&host, port, bytes).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn float_no_fire_is_noop() {
		let n = VrchatAvatarParameterFloatNode;
		let mut ctx = ExecCtx::default();
		let out = n.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
	}
}
