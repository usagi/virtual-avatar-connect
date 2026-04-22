//! `flowgraph.channel.emit` ノード（δ-9 Part C）。
//!
//! Flowgraph から V1 の `ChannelDatum` を `State.channel_data` に push する EffectfulNode。
//! WebSocket クライアント / browser-output はこの `channel_data` を 100ms 間隔でポーリングして
//! 描画しているので、Flowgraph 側の終端を本ノードで V1 表示系に流し込む。
//!
//! ## 設計
//!
//! - `channel.emit` は `ExecCtx.state_handle` 経由で `State.push_channel_datum` を呼ぶ。
//!   `state_handle` が `None`（テスト等）の場合は `on_error` を発火する。
//! - `push_channel_datum` は V1 の dispatch（`dispatch_processors_for_incoming`）を巻き起こすが、
//!   Part D で V1 processors 削除後はエコー再発火はない。ws emit / Control API emit のみが残る。
//!
//! ## ポート
//!
//! - inputs: `exec_in`, `channel` (String, required), `content` (String, required),
//!   `is_final` (Bool, default true), `source_actor` (String, default "")
//! - outputs: `exec_out` (= on success), `on_error` (Exec)
//!
//! ## エコー防止 (δ-9 Part E)
//!
//! `source_actor` 入力が空で `auto_tag_source_actor` プロパティが true（デフォルト）の場合、
//! `flowgraph:<node_id>` を自動スタンプする。これにより `flowgraph.ingress.channel_subscribe`
//! のデフォルトフィルタ (`ignore_flowgraph_echo = true`) が自ノード発の datum を安全に drop できる。

use crate::flowgraph::node::{
	EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
	PropertySpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct ChannelEmitNode;

impl NodeDescriptor for ChannelEmitNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.channel.emit".into(),
			title: "Channel Emit".into(),
			category: "channel".into(),
			description: Some(
				"State.channel_data に ChannelDatum を push。WS クライアント / browser-output に届く終端ノード。"
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("channel", "Channel", SocketType::String)
					.with_default(SocketValue::String(String::new())),
				PortSpec::input("content", "Content", SocketType::String)
					.with_default(SocketValue::String(String::new())),
				PortSpec::input("is_final", "Is Final", SocketType::Bool)
					.with_default(SocketValue::Bool(true)),
				PortSpec::input("source_actor", "Source Actor", SocketType::String)
					.with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
			],
			properties: vec![
				PropertySpec::new(
					"fallback_channel",
					"Fallback Channel",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("`channel` 入力が空のときに使うフォールバックチャンネル名。両方空なら on_error 発火。"),
				PropertySpec::new(
					"auto_tag_source_actor",
					"Auto-Tag Source Actor",
					SocketType::Bool,
					SocketValue::Bool(true),
				)
				.description(
					"`source_actor` 入力が空のとき、自動で `flowgraph:<node_id>` を stamp する。\
					 `channel.subscribe` のデフォルト echo filter と協調する。false で無効化。",
				),
			],
		}
	}
}

#[async_trait]
impl EffectfulNode for ChannelEmitNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		properties: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}

		let channel_in = inputs
			.get("channel")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.trim()
			.to_string();
		let fallback = properties
			.get("fallback_channel")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.trim()
			.to_string();
		let channel = if !channel_in.is_empty() { channel_in } else { fallback };
		if channel.is_empty() {
			ctx.log("channel.emit: channel が空のため on_error");
			return Ok(NodeOutput::new().fire_exec("on_error"));
		}

		let content = inputs
			.get("content")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.to_string();
		let is_final = inputs
			.get("is_final")
			.and_then(|v| v.as_bool().ok())
			.unwrap_or(true);
		let mut source_actor = inputs
			.get("source_actor")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.to_string();
		if source_actor.is_empty() {
			let auto_tag = properties
				.get("auto_tag_source_actor")
				.and_then(|v| v.as_bool().ok())
				.unwrap_or(true);
			if auto_tag {
				let tag = if ctx.node_id.is_empty() { "flowgraph".to_string() } else { format!("flowgraph:{}", ctx.node_id) };
				source_actor = tag;
			}
		}

		let Some(weak) = ctx.state_handle.as_ref() else {
			ctx.log("channel.emit: state_handle が None のため push スキップ (on_error)");
			return Ok(NodeOutput::new().fire_exec("on_error"));
		};
		let Some(state_arc) = weak.upgrade() else {
			ctx.log("channel.emit: State が既に drop されている (on_error)");
			return Ok(NodeOutput::new().fire_exec("on_error"));
		};

		let mut cd = crate::state::ChannelDatum::new(channel, content).with_flag_if(
			crate::state::ChannelDatum::FLAG_IS_FINAL,
			is_final,
		);
		if !source_actor.is_empty() {
			cd = cd.with_meta("source_actor", source_actor);
		}
		let id = cd.get_id();
		state_arc.read().await.push_channel_datum(cd).await;
		ctx.log(format!("channel.emit: pushed id={id}"));
		Ok(NodeOutput::new().fire_exec("exec_out"))
	}
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn emit_without_state_handle_fires_on_error() {
		let node = ChannelEmitNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let mut inputs = InputMap::new();
		inputs.insert("channel".into(), SocketValue::String("chan".into()));
		inputs.insert("content".into(), SocketValue::String("hi".into()));
		inputs.insert("is_final".into(), SocketValue::Bool(true));
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		assert!(!out.fired_exec.contains("exec_out"));
	}

	#[tokio::test]
	async fn emit_with_empty_channel_fires_on_error_before_state_lookup() {
		let node = ChannelEmitNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let mut inputs = InputMap::new();
		inputs.insert("channel".into(), SocketValue::String("   ".into()));
		inputs.insert("content".into(), SocketValue::String("hi".into()));
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[tokio::test]
	async fn emit_uses_fallback_channel_when_input_empty() {
		let node = ChannelEmitNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let mut props = InputMap::new();
		props.insert("fallback_channel".into(), SocketValue::String("fb".into()));
		let mut inputs = InputMap::new();
		inputs.insert("channel".into(), SocketValue::String(String::new()));
		inputs.insert("content".into(), SocketValue::String("hello".into()));
		// No state_handle → on_error、ただし channel 判定自体は通っている（log で確認）
		let out = node.execute(&mut ctx, &props, &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		assert!(ctx.trace.iter().any(|t| t.contains("state_handle が None")));
	}
}
