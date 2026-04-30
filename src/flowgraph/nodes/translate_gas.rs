//! `flowgraph.translate.gas`: Google Apps Script 経由の翻訳 EffectfulNode。
//!
//! V1 `GasTranslation` の Flowgraph 版。
//! - URL テンプレート: `https://script.google.com/macros/s/{script_id}/exec?trans_sourcelang={from}&target={to}&text=...`
//! - `translate_from` が空なら `whatlang` で自動推定（推定失敗は on_error）。
//! - エラーは `on_error` + `error: String` として出力する（engine 停止しない）。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec)
//!   - `text` (String): 翻訳元
//!   - `translate_to` (String): 2 文字 ISO-639-1（例: `"en"`）
//!   - `translate_from` (String, default `""`): 空なら whatlang で自動推定
//!   - `script_id` (String, default `""`): 空なら env `VAC_GAS_TRANSLATION_SCRIPT_ID` を利用
//! - 出力:
//!   - `on_success` (Exec) / `on_error` (Exec)
//!   - `translated` (String)
//!   - `detected_lang` (String): 自動推定したとき推定結果、明示指定時は入力 `translate_from` を echo
//!   - `error` (String)

use crate::flowgraph::node::{
	get_optional_string, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput,
	NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use async_trait::async_trait;

const ENV_GAS_TRANSLATION_SCRIPT_ID: &str = "VAC_GAS_TRANSLATION_SCRIPT_ID";
const URL_TEMPLATE: &str = "https://script.google.com/macros/s/{script_id}/exec?trans_sourcelang={from}&target={to}&text={text}";

pub struct TranslateGasNode;

impl NodeDescriptor for TranslateGasNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.translate.gas".into(),
			title: "Translate (GAS)".into(),
			category: "translate".into(),
			description: Some("Google Apps Script 経由の翻訳 API 呼び出し".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("text", "Text", SocketType::String),
				PortSpec::input("translate_to", "Translate To", SocketType::String),
				PortSpec::input("translate_from", "Translate From", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("script_id", "Script ID", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("translated", "Translated", SocketType::String),
				PortSpec::output("detected_lang", "Detected Lang", SocketType::String),
				PortSpec::output("error", "Error", SocketType::String),
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::String))),
			],
			properties: vec![],
		}
	}
}

fn detect_language_639_1(source: &str) -> Result<String, String> {
	let info = whatlang::detect(source).ok_or_else(|| "入力言語の推定に失敗しました".to_string())?;
	let l3 = info.lang().code();
	let l2 = isolang::Language::from_639_3(l3)
		.and_then(|l| l.to_639_1())
		.ok_or_else(|| format!("推定言語 {l3} を 639-1 に変換できませんでした"))?;
	Ok(l2.to_string())
}

/// エラーを on_error 系出力に束ねたショートカット。
fn err_output(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("translated", SocketValue::String(String::new()))
		.set_data("detected_lang", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code("translate.gas")))
		.fire_exec("on_error")
}

fn success_output(translated: String, detected_lang: String) -> NodeOutput {
	NodeOutput::new()
		.set_data("translated", SocketValue::String(translated.clone()))
		.set_data("detected_lang", SocketValue::String(detected_lang))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::String(translated))))
		.fire_exec("on_success")
}

#[async_trait]
impl EffectfulNode for TranslateGasNode {
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
		let script_id_in = get_optional_string(inputs, "script_id", "")?;

		if text.is_empty() {
			return Ok(err_output("text が空です"));
		}
		if translate_to.is_empty() {
			return Ok(err_output("translate_to が空です"));
		}

		let script_id = if script_id_in.is_empty() {
			match std::env::var(ENV_GAS_TRANSLATION_SCRIPT_ID) {
				Ok(v) if !v.is_empty() => v,
				_ => {
					return Ok(err_output(format!(
						"script_id が未指定で環境変数 {ENV_GAS_TRANSLATION_SCRIPT_ID} も空です"
					)));
				}
			}
		} else {
			script_id_in
		};

		let (translate_from, detected_lang) = if translate_from_in.is_empty() {
			match detect_language_639_1(&text) {
				Ok(l) => (l.clone(), l),
				Err(e) => return Ok(err_output(e)),
			}
		} else {
			(translate_from_in.clone(), translate_from_in)
		};

		let url = URL_TEMPLATE
			.replace("{script_id}", &script_id)
			.replace("{from}", &translate_from)
			.replace("{to}", &translate_to)
			.replace("{text}", &urlencoding::encode(&text));

		let translated = match reqwest::get(&url).await {
			Ok(resp) => {
				let status = resp.status();
				if !status.is_success() {
					let body = resp.text().await.unwrap_or_default();
					return Ok(err_output(format!("GAS HTTP {status}: {body}")));
				}
				match resp.text().await {
					Ok(t) => t,
					Err(e) => return Ok(err_output(format!("response body 読み込み失敗: {e}"))),
				}
			}
			Err(e) => return Ok(err_output(format!("GET {url}: {e}"))),
		};

		Ok(success_output(translated, detected_lang))
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
		let node = TranslateGasNode;
		let mut ctx = ExecCtx::default();
		let out = node
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
		assert!(out.data.is_empty());
	}

	#[tokio::test]
	async fn missing_script_id_yields_on_error() {
		// 環境変数を一時的に unset
		let saved = std::env::var(ENV_GAS_TRANSLATION_SCRIPT_ID).ok();
		std::env::remove_var(ENV_GAS_TRANSLATION_SCRIPT_ID);

		let node = TranslateGasNode;
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [
			("text".into(), SocketValue::String("hello".into())),
			("translate_to".into(), SocketValue::String("ja".into())),
			("translate_from".into(), SocketValue::String("en".into())),
		]
		.into_iter()
		.collect();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired()).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		match out.data.get("error").unwrap() {
			SocketValue::String(s) => assert!(s.contains("script_id")),
			_ => panic!(),
		}
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("translate.gas"));
			}
			other => panic!("expected result, got {other:?}"),
		}

		if let Some(v) = saved {
			std::env::set_var(ENV_GAS_TRANSLATION_SCRIPT_ID, v);
		}
	}

	#[test]
	fn success_output_includes_result() {
		let out = success_output("hello".into(), "ja".into());
		assert!(out.fired_exec.contains("on_success"));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert_eq!(result.value.as_deref(), Some(&SocketValue::String("hello".into())));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}

	#[tokio::test]
	async fn empty_text_yields_on_error() {
		let node = TranslateGasNode;
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [
			("text".into(), SocketValue::String(String::new())),
			("translate_to".into(), SocketValue::String("ja".into())),
		]
		.into_iter()
		.collect();
		let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired()).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[test]
	fn detect_language_returns_two_letter_code() {
		let code = detect_language_639_1("This is a simple English sentence, hopefully detectable.").unwrap();
		assert_eq!(code.len(), 2);
	}
}
