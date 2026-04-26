//! Responses API 連携の共用ヘルパ。

use super::types::response::{MessageContent, OutputItem, Response};

/// エラー body を truncate する上限（文字数）。
///
/// OpenAI が返す HTML エラー / JSON error body を丸ごとログ/例外メッセージに載せると
/// 巨大になる場合があるので、末尾に `…（truncated）` を付けて切る。
pub const ERROR_BODY_TRUNCATE_CHARS: usize = 2048;

/// エラー body を [`ERROR_BODY_TRUNCATE_CHARS`] で truncate する。
///
/// 文字単位（char）で切るため UTF-8 境界を壊さない。
pub fn truncate_error_body(body: &str) -> String {
	if body.chars().count() <= ERROR_BODY_TRUNCATE_CHARS {
		return body.to_string();
	}
	let truncated: String = body.chars().take(ERROR_BODY_TRUNCATE_CHARS).collect();
	format!("{truncated}…（truncated）")
}

/// Responses レスポンスから assistant の出力テキストを抽出する。
///
/// # 優先順位
///
/// 1. [`Response::output_text`] が `Some(non-empty)` ならそれを返す
/// 2. 無ければ [`Response::output`] を走査して `OutputItem::Message` の
///    `MessageContent::OutputText` を `\n` で連結する
/// 3. どちらも空なら `None`
///
/// tool-call 中心のレスポンス（function_call のみで message 無し）では `None` を返す。
pub fn extract_output_text(res: &Response) -> Option<String> {
	if let Some(t) = &res.output_text {
		if !t.is_empty() {
			return Some(t.clone());
		}
	}
	let mut acc = String::new();
	for item in &res.output {
		if let OutputItem::Message { content, .. } = item {
			for part in content {
				if let MessageContent::OutputText { text, .. } = part {
					if !acc.is_empty() {
						acc.push('\n');
					}
					acc.push_str(text);
				}
			}
		}
	}
	if acc.is_empty() {
		None
	} else {
		Some(acc)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn truncate_error_body_passes_short_body_through() {
		let body = "short error";
		assert_eq!(truncate_error_body(body), body);
	}

	#[test]
	fn truncate_error_body_cuts_long_body_with_marker() {
		let body = "a".repeat(ERROR_BODY_TRUNCATE_CHARS + 500);
		let out = truncate_error_body(&body);
		assert!(out.len() > ERROR_BODY_TRUNCATE_CHARS);
		assert!(out.ends_with("…（truncated）"));
		let prefix: String = out.chars().take(ERROR_BODY_TRUNCATE_CHARS).collect();
		assert_eq!(prefix, "a".repeat(ERROR_BODY_TRUNCATE_CHARS));
	}

	#[test]
	fn truncate_error_body_respects_multibyte_chars() {
		let body = "あ".repeat(ERROR_BODY_TRUNCATE_CHARS + 10);
		let out = truncate_error_body(&body);
		assert!(out.ends_with("…（truncated）"));
		let head_chars = out.chars().filter(|c| *c == 'あ').count();
		assert_eq!(head_chars, ERROR_BODY_TRUNCATE_CHARS);
	}

	#[test]
	fn extract_output_text_prefers_convenience_field() {
		let mut res = Response::default();
		res.output_text = Some("hello".to_string());
		assert_eq!(extract_output_text(&res), Some("hello".to_string()));
	}

	#[test]
	fn extract_output_text_falls_back_to_output_items() {
		let mut res = Response::default();
		res.output_text = None;
		res.output = vec![OutputItem::Message {
			id: "msg_1".to_string(),
			status: Some("completed".to_string()),
			role: "assistant".to_string(),
			content: vec![
				MessageContent::OutputText {
					text: "line one".to_string(),
					annotations: None,
				},
				MessageContent::OutputText {
					text: "line two".to_string(),
					annotations: None,
				},
			],
		}];
		assert_eq!(extract_output_text(&res), Some("line one\nline two".to_string()));
	}

	#[test]
	fn extract_output_text_returns_none_for_empty_response() {
		let res = Response::default();
		assert_eq!(extract_output_text(&res), None);
	}

	#[test]
	fn extract_output_text_returns_none_when_only_function_call() {
		let mut res = Response::default();
		res.output = vec![OutputItem::FunctionCall {
			id: "fc_1".to_string(),
			status: Some("completed".to_string()),
			call_id: "call_abc".to_string(),
			name: "vac_ping".to_string(),
			arguments: "{}".to_string(),
		}];
		assert_eq!(extract_output_text(&res), None);
	}

	#[test]
	fn extract_output_text_empty_convenience_falls_back() {
		let mut res = Response::default();
		res.output_text = Some("".to_string());
		res.output = vec![OutputItem::Message {
			id: "msg_1".to_string(),
			status: None,
			role: "assistant".to_string(),
			content: vec![MessageContent::OutputText {
				text: "assembled".to_string(),
				annotations: None,
			}],
		}];
		assert_eq!(extract_output_text(&res), Some("assembled".to_string()));
	}
}
