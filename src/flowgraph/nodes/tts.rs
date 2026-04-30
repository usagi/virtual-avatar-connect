//! `flowgraph.tts.speak`: 統合 TTS EffectfulNode（δ-4c）。
//!
//! V1 の 5 種プロセッサ（`os_tts` / `bouyomichan` / `voicevox` / `aivis_speech` / `coeiroink`）を
//! 単一ノードに統合し、**`engine` 入力で動的に切り替えられる**。
//! エンジン固有ロジックは [`crate::flowgraph::tts::TtsDriver`] 実装にカプセル化している。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec)
//!   - `engine` (String): `"os"` / `"bouyomichan"` / `"voicevox"` / `"aivis_speech"` / `"coeiroink"` / `"voicepeak"`
//!   - `text` (String)
//!   - `voice` (String, default `""`): エンジン別の意味論（各 driver docs 参照）
//!   - `speed` (Float, default 1.0)
//!   - `pitch` (Float, default 0.0)
//!   - `volume` (Float, default 1.0)
//!   - `endpoint` (String, default `""`): HTTP base URL / TCP `host:port` / **VoicePeak では `voicepeak.exe` の絶対パス**。
//!     VoicePeak で空のときは起動時に解決した `[voicepeak]` + OS 既定パスが State から注入される。
//!   - `extra` (Map<Json>, default `{}`): エンジン固有の escape hatch
//!   - `save_path` (String, default `""`): 合成 WAV 保存先。`{T}` は UTC ISO 日時（`:`、`-` を除去）に展開
//! - 出力:
//!   - `on_success` / `on_error` (Exec)
//!   - `played` (Bool): VAC 共有シンクへ append できたか
//!   - `audio_path` (String): 保存したパス（空なら未保存）
//!   - `error` (String): `on_success` 時は `""`

use crate::flowgraph::node::{
	get_optional_float, get_optional_map, get_optional_string, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap,
	NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use crate::flowgraph::tts::driver::{AudioContext, TtsRequest};
use crate::flowgraph::tts::registry::registry;
use async_trait::async_trait;
use serde_json::json;

pub struct TtsSpeakNode;

impl NodeDescriptor for TtsSpeakNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.tts.speak".into(),
			title: "TTS: Speak".into(),
			category: "tts".into(),
			description: Some(
				"engine 入力で選んだ TTS ドライバに text を合成させ、VAC 共有 audio_sink で再生する。save_path 指定時は WAV も保存。"
					.into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("engine", "Engine", SocketType::String).with_closed_string_variants(registry().names().iter().copied()),
				PortSpec::input("text", "Text", SocketType::String),
				PortSpec::input("voice", "Voice", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("speed", "Speed", SocketType::Float).with_default(SocketValue::Float(1.0)),
				PortSpec::input("pitch", "Pitch", SocketType::Float).with_default(SocketValue::Float(0.0)),
				PortSpec::input("volume", "Volume", SocketType::Float).with_default(SocketValue::Float(1.0)),
				PortSpec::input("endpoint", "Endpoint", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("extra", "Extra", SocketType::Map(Box::new(SocketType::Json)))
					.with_default(SocketValue::Map(Default::default())),
				PortSpec::input("save_path", "Save Path", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("played", "Played", SocketType::Bool),
				PortSpec::output("audio_path", "Audio Path", SocketType::String),
				PortSpec::output("error", "Error", SocketType::String),
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Json))),
			],
			properties: vec![],
		}
	}
}

fn err_output(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("played", SocketValue::Bool(false))
		.set_data("audio_path", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code("tts.speak")))
		.fire_exec("on_error")
}

fn success_output(played: bool, audio_path: String) -> NodeOutput {
	let result = json!({
		"played": played,
		"audio_path": audio_path,
	});
	NodeOutput::new()
		.set_data("played", SocketValue::Bool(played))
		.set_data("audio_path", SocketValue::String(audio_path))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Json(result))))
		.fire_exec("on_success")
}

#[async_trait]
impl EffectfulNode for TtsSpeakNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}

		let engine = get_required_string(inputs, "engine")?;
		let text = get_required_string(inputs, "text")?;
		if text.is_empty() {
			return Ok(err_output("text が空文字です"));
		}

		let voice = get_optional_string(inputs, "voice", "")?;
		let speed = get_optional_float(inputs, "speed", 1.0)?;
		let pitch = get_optional_float(inputs, "pitch", 0.0)?;
		let volume = get_optional_float(inputs, "volume", 1.0)?;
		let mut endpoint = get_optional_string(inputs, "endpoint", "")?;
		if engine.eq_ignore_ascii_case("voicepeak") && endpoint.is_empty() {
			if let Some(wk) = ctx.state_handle.as_ref().and_then(|w| w.upgrade()) {
				let st = wk.read().await;
				endpoint = st.voicepeak_fallback_exe.clone();
			}
		}
		let save_path = get_optional_string(inputs, "save_path", "")?;
		let extra_map = get_optional_map(inputs, "extra")?;

		let driver = match crate::flowgraph::tts::registry().get(&engine) {
			Some(d) => d,
			None => {
				let names = crate::flowgraph::tts::registry().names();
				return Ok(err_output(format!("未知の engine '{engine}' です。利用可能: {}", names.join(", "))));
			}
		};

		let req = TtsRequest {
			text,
			voice,
			speed,
			pitch,
			volume,
			endpoint,
			save_path,
			extra: extra_map.into_owned(),
		};
		let audio = AudioContext {
			sink: ctx.audio_sink.as_ref(),
		};

		match driver.speak(req, &audio).await {
			Ok(outcome) => Ok(success_output(outcome.played, outcome.audio_path)),
			Err(e) => Ok(err_output(e.display_with_engine(&engine))),
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::node::ExecFireSet;
	use std::collections::HashMap;

	fn inputs_with(engine: &str, text: &str) -> HashMap<String, SocketValue> {
		let mut m = HashMap::new();
		m.insert("engine".into(), SocketValue::String(engine.into()));
		m.insert("text".into(), SocketValue::String(text.into()));
		m
	}

	#[tokio::test]
	async fn no_fire_is_no_op() {
		let n = TtsSpeakNode;
		let mut ctx = ExecCtx::default();
		let inputs = inputs_with("os", "hello");
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
		assert!(out.data.is_empty());
	}

	#[tokio::test]
	async fn unknown_engine_fires_on_error() {
		let n = TtsSpeakNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let out = n
			.execute(&mut ctx, &InputMap::new(), &inputs_with("nonexistent", "hi"), &fired)
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out
			.data
			.get("error")
			.and_then(|v| v.as_str().ok().map(str::to_owned))
			.unwrap_or_default();
		assert!(err.contains("未知の engine"));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("tts.speak"));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}

	#[test]
	fn success_output_includes_result() {
		let out = success_output(true, "out.wav".into());
		assert!(out.fired_exec.contains("on_success"));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert_eq!(result.value.as_deref(), Some(&SocketValue::Json(json!({ "played": true, "audio_path": "out.wav" }))));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}

	#[tokio::test]
	async fn empty_text_fires_on_error() {
		let n = TtsSpeakNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs_with("os", ""), &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[tokio::test]
	async fn voicevox_unreachable_endpoint_fires_on_error() {
		let n = TtsSpeakNode;
		let mut ctx = ExecCtx::default();
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		let mut inputs = inputs_with("voicevox", "こんにちは");
		inputs.insert("voice".into(), SocketValue::String("1".into()));
		inputs.insert("endpoint".into(), SocketValue::String("http://127.0.0.1:1".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out
			.data
			.get("error")
			.and_then(|v| v.as_str().ok().map(str::to_owned))
			.unwrap_or_default();
		assert!(err.contains("[tts.voicevox]"));
	}
}
