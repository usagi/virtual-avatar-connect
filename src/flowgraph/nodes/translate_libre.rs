//! `flowgraph.translate.libre`: LibreTranslate API 呼び出し EffectfulNode。
//!
//! V1 `LibreTranslation` の Flowgraph 版。埋め込みサーバの起動/停止（`state.libretranslate`）は
//! ノードのスコープ外とし、`base_url` を必須入力として受け取る（外部 / ローカル両対応）。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec)
//!   - `text` (String): 翻訳元
//!   - `translate_to` (String)
//!   - `translate_from` (String, default `""`): 空なら whatlang 自動推定
//!   - `base_url` (String): 例 `http://127.0.0.1:5000`
//! - 出力:
//!   - `on_success` / `on_error` (Exec)
//!   - `translated` (String)
//!   - `detected_lang` (String)
//!   - `error` (String)

use crate::flowgraph::node::{
	get_optional_string, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput,
	NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct TranslateLibreNode;

impl NodeDescriptor for TranslateLibreNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.translate.libre".into(),
			title: "Translate (LibreTranslate)".into(),
			category: "translate".into(),
			description: Some("LibreTranslate REST API (/translate) を叩く".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("text", "Text", SocketType::String),
				PortSpec::input("translate_to", "Translate To", SocketType::String),
				PortSpec::input("translate_from", "Translate From", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("base_url", "Base URL", SocketType::String),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("translated", "Translated", SocketType::String),
				PortSpec::output("detected_lang", "Detected Lang", SocketType::String),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

fn err_output(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("translated", SocketValue::String(String::new()))
		.set_data("detected_lang", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

fn detect_language_639_1(source: &str) -> Result<String, String> {
	let info = whatlang::detect(source).ok_or_else(|| "入力言語の推定に失敗しました".to_string())?;
	let l3 = info.lang().code();
	let l2 = isolang::Language::from_639_3(l3)
		.and_then(|l| l.to_639_1())
		.ok_or_else(|| format!("推定言語 {l3} を 639-1 に変換できませんでした"))?;
	Ok(l2.to_string())
}

#[async_trait]
impl EffectfulNode for TranslateLibreNode {
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
		let text = get_required_string(inputs, "text")?;
		let translate_to = get_required_string(inputs, "translate_to")?;
		let translate_from_in = get_optional_string(inputs, "translate_from", "")?;
		let base_url = get_required_string(inputs, "base_url")?;

		if text.is_empty() {
			return Ok(err_output("text が空です"));
		}
		if translate_to.is_empty() {
			return Ok(err_output("translate_to が空です"));
		}
		if base_url.is_empty() {
			return Ok(err_output("base_url が空です"));
		}

		let (translate_from, detected_lang) = if translate_from_in.is_empty() {
			match detect_language_639_1(&text) {
				Ok(l) => (l.clone(), l),
				Err(e) => return Ok(err_output(e)),
			}
		} else {
			(translate_from_in.clone(), translate_from_in)
		};

		match crate::libretranslate::translate(&base_url, &translate_from, &translate_to, &text).await {
			Ok(translated) => Ok(NodeOutput::new()
				.set_data("translated", SocketValue::String(translated))
				.set_data("detected_lang", SocketValue::String(detected_lang))
				.set_data("error", SocketValue::String(String::new()))
				.fire_exec("on_success")),
			Err(e) => Ok(err_output(e)),
		}
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
	async fn no_fire_is_noop() {
		let node = TranslateLibreNode;
		let mut ctx = ExecCtx::default();
		let out = node
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[tokio::test]
	async fn empty_base_url_yields_on_error() {
		let node = TranslateLibreNode;
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [
			("text".into(), SocketValue::String("hi".into())),
			("translate_to".into(), SocketValue::String("ja".into())),
			("translate_from".into(), SocketValue::String("en".into())),
			("base_url".into(), SocketValue::String(String::new())),
		]
		.into_iter()
		.collect();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired()).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[tokio::test]
	async fn unreachable_base_url_is_reported_not_fatal() {
		let node = TranslateLibreNode;
		let mut ctx = ExecCtx::default();
		// 127.0.0.1:1 は常に拒否される想定
		let inputs: InputMap = [
			("text".into(), SocketValue::String("hi".into())),
			("translate_to".into(), SocketValue::String("ja".into())),
			("translate_from".into(), SocketValue::String("en".into())),
			("base_url".into(), SocketValue::String("http://127.0.0.1:1".into())),
		]
		.into_iter()
		.collect();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired()).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		match out.data.get("error").unwrap() {
			SocketValue::String(s) => assert!(!s.is_empty()),
			_ => panic!(),
		}
	}
}
