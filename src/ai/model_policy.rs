//! Responses API リクエストで **モデル系列ごとに異なる既定** を一箇所にまとめる。
//! UI やプロンプト組み立て（`context`）とは分離し、将来モデルが増えてもここを拡張する。

use crate::ai::openai_responses::types::request::{CreateResponseRequest, Reasoning, ReasoningEffort, TextConfig, TextFormat};

#[inline]
pub(crate) fn is_gpt5_family(model_id: Option<&str>) -> bool {
	model_id.is_some_and(|m| m.starts_with("gpt-5"))
}

#[inline]
pub(crate) fn needs_default_system_when_missing(model_id: Option<&str>) -> bool {
	is_gpt5_family(model_id)
}

#[inline]
pub(crate) fn should_retry_on_empty_assistant(model_id: Option<&str>) -> bool {
	is_gpt5_family(model_id)
}

/// Responses API へのリクエストに、モデル族に応じた既定値を適用する。
///
/// - `max_output_tokens`: モデル族を問わず `request.max_output_tokens` に代入する
///   （Chat Completions の `max_tokens` / `max_completion_tokens` の分岐は Responses では不要）。
/// - `reasoning.effort`: gpt-5 系のみ意味を持つ。その他モデルに指定しても API 側は
///   無視するか `invalid_request_error` を返す可能性があるので、ここでは設定しない。
/// - `text.format`: 旧 Chat Completions `ResponseFormat::Text` の Responses 等価。Responses の既定は
///   そもそも自由文なので明示は不要だが、gpt-5 系で structured outputs の誤起動を防ぐため
///   `TextFormat::Text` を明示する（Chat Completions 時代の挙動を維持）。
///
/// いずれも persona 設定の値で **常に** 上書きする（既存値は保持しない）。
pub(crate) fn apply_model_responses_options(
	request: &mut CreateResponseRequest,
	model: &str,
	max_output_tokens: Option<u32>,
	reasoning_effort: Option<ReasoningEffort>,
) {
	if is_gpt5_family(Some(model)) {
		let text_cfg = request.text.get_or_insert_with(TextConfig::default);
		if text_cfg.format.is_none() {
			text_cfg.format = Some(TextFormat::Text);
		}
	}

	if let Some(mt) = max_output_tokens {
		request.max_output_tokens = Some(mt);
	}

	if let Some(effort) = reasoning_effort {
		if is_gpt5_family(Some(model)) {
			let r = request.reasoning.get_or_insert_with(Reasoning::default);
			r.effort = Some(effort);
		} else {
			log::warn!("openai_reasoning_effort は gpt-5 系モデル専用です（指定モデル: {model}）。無視します。");
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn gpt5_family_detection() {
		assert!(is_gpt5_family(Some("gpt-5")));
		assert!(is_gpt5_family(Some("gpt-5-mini")));
		assert!(is_gpt5_family(Some("gpt-5.4-nano")));
		assert!(is_gpt5_family(Some("gpt-5.4")));
		assert!(!is_gpt5_family(Some("gpt-4o")));
		assert!(!is_gpt5_family(Some("gpt-4o-mini")));
		assert!(!is_gpt5_family(None));
	}

	#[test]
	fn policy_aliases_track_gpt5_family() {
		for id in ["gpt-5", "gpt-5.4-nano", "gpt-5.4"] {
			let m = Some(id);
			assert_eq!(needs_default_system_when_missing(m), is_gpt5_family(m));
			assert_eq!(should_retry_on_empty_assistant(m), is_gpt5_family(m));
		}
		assert!(!should_retry_on_empty_assistant(Some("gpt-4o-mini")));
	}

	#[test]
	fn responses_options_apply_max_output_tokens_for_any_model() {
		let mut req = CreateResponseRequest {
			model: "gpt-4o-mini".to_string(),
			..Default::default()
		};
		apply_model_responses_options(&mut req, "gpt-4o-mini", Some(512), None);
		assert_eq!(req.max_output_tokens, Some(512));
		assert!(req.reasoning.is_none());
		assert!(req.text.is_none(), "non-gpt5 は text.format を埋めない");
	}

	#[test]
	fn responses_options_apply_reasoning_effort_for_gpt5() {
		let mut req = CreateResponseRequest {
			model: "gpt-5-mini".to_string(),
			..Default::default()
		};
		apply_model_responses_options(&mut req, "gpt-5-mini", None, Some(ReasoningEffort::High));
		assert_eq!(req.reasoning.as_ref().and_then(|r| r.effort), Some(ReasoningEffort::High));
		assert!(matches!(req.text.as_ref().and_then(|t| t.format.clone()), Some(TextFormat::Text)));
	}

	#[test]
	fn responses_options_ignore_reasoning_effort_for_non_gpt5() {
		let mut req = CreateResponseRequest {
			model: "gpt-4o-mini".to_string(),
			..Default::default()
		};
		apply_model_responses_options(&mut req, "gpt-4o-mini", None, Some(ReasoningEffort::Low));
		assert!(req.reasoning.is_none());
	}

	#[test]
	fn responses_options_preserve_existing_text_format() {
		let mut req = CreateResponseRequest {
			model: "gpt-5-mini".to_string(),
			text: Some(TextConfig {
				format: Some(TextFormat::JsonObject),
			}),
			..Default::default()
		};
		apply_model_responses_options(&mut req, "gpt-5-mini", None, None);
		assert!(matches!(
			req.text.as_ref().and_then(|t| t.format.clone()),
			Some(TextFormat::JsonObject)
		));
	}
}
