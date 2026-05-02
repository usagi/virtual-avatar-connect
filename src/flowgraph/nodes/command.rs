//! `flowgraph.command.match`: 「prefix 付きコマンド文字列」をパースして
//! verb + args に分解する PureNode。V1 の `Command` processor の「どの命令か判定する部分」を
//! Flowgraph ネイティブ化したもの。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec)
//!   - `content` (String): 入力文字列
//!   - `prefix` (String, optional default `/`): コマンド接頭辞
//! - 出力:
//!   - `on_command` (Exec): prefix にマッチしたとき発火
//!   - `on_other` (Exec): マッチしなかったとき発火
//!   - `command` (String): prefix を剥がしたあとの最初のトークン。非コマンド時は空文字
//!   - `args` (List<String>): 残りのトークン。非コマンド時は空リスト
//!   - `original` (String): 元の content 全文
//!
//! ## 設計メモ
//!
//! V1 の `Command` は quit/disable/enable/reload/set の 5 コマンドを内部ハードコードしていたが、
//! Flowgraph ではディスパッチ自体を **ユーザ側のグラフ**（`compare.eq` + `branch` の連鎖 等）に委ねる。
//! この方が拡張も可読性も高く、Pure に保てる。

use crate::flowgraph::node::{
	get_optional_string, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput,
	NodeSpec, PortSpec, PropertySpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::Value as JsonValue;

pub struct CommandMatchNode;

impl NodeDescriptor for CommandMatchNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.command.match".into(),
			title: "Command Match".into(),
			category: "command".into(),
			description: Some("prefix 付きコマンド文字列を verb + args にパースして exec を分岐".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("content", "Content", SocketType::String),
				PortSpec::input("prefix", "Prefix", SocketType::String).with_default(SocketValue::String("/".into())),
			],
			outputs: vec![
				PortSpec::exec_output("on_command", "On Command"),
				PortSpec::exec_output("on_other", "On Other"),
				PortSpec::output("command", "Command", SocketType::String),
				PortSpec::output("args", "Args", SocketType::List(Box::new(SocketType::String))),
				PortSpec::output("original", "Original", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for CommandMatchNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let content = get_required_string(inputs, "content")?;
		let prefix = get_optional_string(inputs, "prefix", "/")?;

		let mut out = NodeOutput::new().set_data("original", SocketValue::String(content.clone()));

		if !prefix.is_empty() && content.starts_with(&prefix) {
			let body = content.trim_start_matches(&prefix);
			let mut tokens = body.split_whitespace();
			let verb = tokens.next().unwrap_or("").to_string();
			let args: Vec<SocketValue> = tokens.map(|t| SocketValue::String(t.to_string())).collect();
			out = out
				.set_data("command", SocketValue::String(verb))
				.set_data("args", SocketValue::List(args))
				.fire_exec("on_command");
		} else {
			out = out
				.set_data("command", SocketValue::String(String::new()))
				.set_data("args", SocketValue::List(vec![]))
				.fire_exec("on_other");
		}
		Ok(out)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn fired() -> ExecFireSet {
		let mut f = ExecFireSet::new();
		f.insert("exec_in");
		f
	}

	#[tokio::test]
	async fn matches_slash_command_and_splits_args() {
		let node = CommandMatchNode;
		let inputs: InputMap = [("content".into(), SocketValue::String("/set foo bar".into()))]
			.into_iter()
			.collect();
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&fired(),
			)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_command"));
		assert!(!out.fired_exec.contains("on_other"));
		assert_eq!(out.data.get("command"), Some(&SocketValue::String("set".into())));
		let args = out.data.get("args").cloned().unwrap();
		assert_eq!(
			args,
			SocketValue::List(vec![SocketValue::String("foo".into()), SocketValue::String("bar".into())])
		);
	}

	#[tokio::test]
	async fn non_command_triggers_on_other() {
		let node = CommandMatchNode;
		let inputs: InputMap = [("content".into(), SocketValue::String("hello world".into()))]
			.into_iter()
			.collect();
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&fired(),
			)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_other"));
		assert!(!out.fired_exec.contains("on_command"));
		assert_eq!(out.data.get("command"), Some(&SocketValue::String("".into())));
		assert_eq!(out.data.get("args"), Some(&SocketValue::List(vec![])));
		assert_eq!(out.data.get("original"), Some(&SocketValue::String("hello world".into())));
	}

	#[tokio::test]
	async fn custom_prefix_works() {
		let node = CommandMatchNode;
		let inputs: InputMap = [
			("content".into(), SocketValue::String("!quit".into())),
			("prefix".into(), SocketValue::String("!".into())),
		]
		.into_iter()
		.collect();
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&fired(),
			)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_command"));
		assert_eq!(out.data.get("command"), Some(&SocketValue::String("quit".into())));
	}

	#[tokio::test]
	async fn no_exec_firing_is_noop() {
		let node = CommandMatchNode;
		let inputs: InputMap = [("content".into(), SocketValue::String("/x".into()))].into_iter().collect();
		let out = node
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
		assert!(out.data.is_empty());
	}
}

// ---------------------------------------------------------------------
// CommandSetNode (δ-X): シーン切替用の「コマンド名 → 発話セット」ディスパッチ
// ---------------------------------------------------------------------

/// `flowgraph.command.set`: V1 `command` processor の scene switcher 相当。
///
/// プロパティ `sets` に JSON の配列を与え、入力 `command_name` と一致する set を
/// 引いて `pre` / `channel_contents` / `post` を順に `State.channel_data` へ push する。
///
/// ## `sets` スキーマ
///
/// ```json
/// [
///   {
///     "name": "morning",
///     "pre":  ["おはよう", "今日は配信します"],
///     "post": ["よろしく"],
///     "channel_contents": [
///       { "channel": "obs_scene", "content": "Scene-Morning" },
///       { "channel": "vmc_pose",  "content": "stand_01" }
///     ]
///   }
/// ]
/// ```
///
/// ## ポート
/// - 入力: `exec_in`, `command_name: String`
/// - 出力: `on_set` (Exec), `on_none` (Exec), `matched_name: String`, `entry_count: Int`
///
/// ## プロパティ
/// - `sets: Json` — 上記スキーマ。未設定時は空配列扱い。
/// - `pre_post_channel: String` — `pre` / `post` を流すチャンネル名。空のときは pre/post を無視する。
/// - `source_actor: String` — ChannelDatum の `source_actor` メタ。空なら `flowgraph:<node_id>` を自動スタンプ。
pub struct CommandSetNode;

impl NodeDescriptor for CommandSetNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.command.set".into(),
			title: "Command Set".into(),
			category: "command".into(),
			description: Some(
				"command_name に一致する set を sets プロパティから引き、\
				 pre → channel_contents → post の順でチャンネルへ push する"
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("command_name", "Command Name", SocketType::String),
			],
			outputs: vec![
				PortSpec::exec_output("on_set", "On Set"),
				PortSpec::exec_output("on_none", "On None"),
				PortSpec::output("matched_name", "Matched Name", SocketType::String),
				PortSpec::output("entry_count", "Entry Count", SocketType::Int),
			],
			properties: vec![
				PropertySpec::new("sets", "Sets", SocketType::Json, SocketValue::Json(JsonValue::Array(Vec::new())))
					.description("コマンドセット配列。各要素は `{ name, pre?, post?, channel_contents? }` を持つ JSON。"),
				PropertySpec::new(
					"pre_post_channel",
					"Pre/Post Channel",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("`pre` / `post` を流すチャンネル名。空なら pre/post を無視する。"),
				PropertySpec::new(
					"source_actor",
					"Source Actor",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description("ChannelDatum の source_actor。空なら `flowgraph:<node_id>` を自動スタンプ。"),
			],
		}
	}
}

#[async_trait]
impl EffectfulNode for CommandSetNode {
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

		let name = inputs
			.get("command_name")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.trim()
			.to_string();
		if name.is_empty() {
			ctx.log("command.set: command_name が空 → on_none");
			return Ok(NodeOutput::new().fire_exec("on_none"));
		}

		let sets_json = match properties.get("sets") {
			Some(SocketValue::Json(v)) => v.clone(),
			_ => JsonValue::Array(Vec::new()),
		};
		let arr = sets_json.as_array().cloned().unwrap_or_default();
		let found = arr.iter().find(|entry| {
			entry
				.as_object()
				.and_then(|m| m.get("name"))
				.and_then(|v| v.as_str())
				.map(|s| s == name.as_str())
				.unwrap_or(false)
		});
		let Some(entry) = found else {
			ctx.log(format!("command.set: name={name} に一致する set 無し → on_none"));
			return Ok(NodeOutput::new()
				.set_data("matched_name", SocketValue::String(String::new()))
				.set_data("entry_count", SocketValue::Int(0))
				.fire_exec("on_none"));
		};

		let source_actor = {
			let from_prop = properties
				.get("source_actor")
				.and_then(|v| v.as_str().ok())
				.unwrap_or("")
				.trim()
				.to_string();
			if from_prop.is_empty() {
				if ctx.node_id.is_empty() {
					"flowgraph".to_string()
				} else {
					format!("flowgraph:{}", ctx.node_id)
				}
			} else {
				from_prop
			}
		};

		let pre_post_channel = properties
			.get("pre_post_channel")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.trim()
			.to_string();

		let Some(weak) = ctx.state_handle.as_ref() else {
			ctx.log("command.set: state_handle が None → on_none");
			return Ok(NodeOutput::new()
				.set_data("matched_name", SocketValue::String(name))
				.set_data("entry_count", SocketValue::Int(0))
				.fire_exec("on_none"));
		};
		let Some(state_arc) = weak.upgrade() else {
			ctx.log("command.set: State が drop 済 → on_none");
			return Ok(NodeOutput::new()
				.set_data("matched_name", SocketValue::String(name))
				.set_data("entry_count", SocketValue::Int(0))
				.fire_exec("on_none"));
		};

		let mut emitted: i64 = 0;

		// pre
		if !pre_post_channel.is_empty() {
			if let Some(pre) = entry.get("pre").and_then(|v| v.as_array()) {
				for line in pre.iter().filter_map(|v| v.as_str()) {
					push_one(&state_arc, &pre_post_channel, line, &source_actor).await;
					emitted += 1;
				}
			}
		}

		// channel_contents
		if let Some(items) = entry.get("channel_contents").and_then(|v| v.as_array()) {
			for item in items {
				let ch = item.get("channel").and_then(|v| v.as_str()).unwrap_or("").trim();
				let content = item.get("content").and_then(|v| v.as_str()).unwrap_or("");
				if ch.is_empty() {
					continue;
				}
				push_one(&state_arc, ch, content, &source_actor).await;
				emitted += 1;
			}
		}

		// post
		if !pre_post_channel.is_empty() {
			if let Some(post) = entry.get("post").and_then(|v| v.as_array()) {
				for line in post.iter().filter_map(|v| v.as_str()) {
					push_one(&state_arc, &pre_post_channel, line, &source_actor).await;
					emitted += 1;
				}
			}
		}

		ctx.log(format!("command.set: name={name} emitted={emitted}"));
		Ok(NodeOutput::new()
			.set_data("matched_name", SocketValue::String(name))
			.set_data("entry_count", SocketValue::Int(emitted))
			.fire_exec("on_set"))
	}
}

async fn push_one(state_arc: &crate::SharedState, channel: &str, content: &str, source_actor: &str) {
	let mut cd = crate::state::ChannelDatum::new(channel.to_string(), content.to_string())
		.with_flag_if(crate::state::ChannelDatum::FLAG_IS_FINAL, true);
	if !source_actor.is_empty() {
		cd = cd.with_meta("source_actor", source_actor.to_string());
	}
	state_arc.read().await.push_channel_datum(cd).await;
}

#[cfg(test)]
mod set_tests {
	use super::*;

	fn fired() -> ExecFireSet {
		let mut f = ExecFireSet::new();
		f.insert("exec_in");
		f
	}

	#[tokio::test]
	async fn missing_command_name_fires_on_none() {
		let node = CommandSetNode;
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [("command_name".into(), SocketValue::String("".into()))].into_iter().collect();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired()).await.unwrap();
		assert!(out.fired_exec.contains("on_none"));
	}

	#[tokio::test]
	async fn unknown_name_fires_on_none() {
		let node = CommandSetNode;
		let mut ctx = ExecCtx::default();
		let sets = serde_json::json!([{"name": "a", "channel_contents": [{"channel": "c", "content": "x"}]}]);
		let mut props = InputMap::new();
		props.insert("sets".into(), SocketValue::Json(sets));
		let inputs: InputMap = [("command_name".into(), SocketValue::String("b".into()))].into_iter().collect();
		let out = node.execute(&mut ctx, &props, &inputs, &fired()).await.unwrap();
		assert!(out.fired_exec.contains("on_none"));
		assert_eq!(out.data.get("entry_count"), Some(&SocketValue::Int(0)));
	}

	#[tokio::test]
	async fn known_name_without_state_handle_fires_on_none() {
		let node = CommandSetNode;
		let mut ctx = ExecCtx::default();
		let sets = serde_json::json!([{"name": "a", "channel_contents": [{"channel": "c", "content": "x"}]}]);
		let mut props = InputMap::new();
		props.insert("sets".into(), SocketValue::Json(sets));
		let inputs: InputMap = [("command_name".into(), SocketValue::String("a".into()))].into_iter().collect();
		let out = node.execute(&mut ctx, &props, &inputs, &fired()).await.unwrap();
		// state_handle が None のため push 不能 → on_none、matched_name は返す
		assert!(out.fired_exec.contains("on_none"));
		assert_eq!(out.data.get("matched_name"), Some(&SocketValue::String("a".into())));
	}
}
