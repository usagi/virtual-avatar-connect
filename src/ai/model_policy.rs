//! Chat Completions へのリクエストで **モデル系列ごとに異なる既定** を一箇所にまとめる。
//! UI やプロンプト組み立て（`context`）とは分離し、将来モデルが増えてもここを拡張する。

use async_openai::types::chat::{CreateChatCompletionRequestArgs, ResponseFormat};

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

pub(crate) fn apply_model_chat_options(
 builder: &mut CreateChatCompletionRequestArgs,
 model: &str,
 max_tokens: Option<u16>,
) {
 if is_gpt5_family(Some(model)) {
  builder.response_format(ResponseFormat::Text);
 }
 if let Some(mt) = max_tokens {
  if is_gpt5_family(Some(model)) {
   builder.max_completion_tokens(mt);
  } else {
   builder.max_tokens(mt);
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
}
