//! Chat Completions 呼び出しと gpt-5 系の空応答再試行。

use super::{model_policy, tools};
use super::tools::ToolContext;
use anyhow::{bail, Context, Result};
use async_openai::{
 config::OpenAIConfig,
 types::chat::{
  ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs, ChatCompletionRequestToolMessage,
  ChatCompletionRequestToolMessageContent, ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequest,
  CreateChatCompletionRequestArgs, CreateChatCompletionResponse, ResponseFormat,
 },
 Client,
};

pub(crate) async fn create_chat_completion(
 client: &Client<OpenAIConfig>,
 request: CreateChatCompletionRequest,
) -> Result<CreateChatCompletionResponse> {
 client.chat().create(request).await.map_err(Into::into)
}

pub(crate) async fn create_chat_completion_resolve_tools(
 client: &Client<OpenAIConfig>,
 mut request: CreateChatCompletionRequest,
 tool_ctx: &ToolContext,
) -> Result<String> {
 let declared = request
  .tools
  .as_deref()
  .map(tools::declared_tool_names)
  .unwrap_or_default();
 const MAX_TOOL_ROUNDS: usize = 8;
 for _ in 0..MAX_TOOL_ROUNDS {
  let response = create_chat_completion(client, request.clone()).await?;
  let choice = response.choices.first().context("AI からの応答 choice がありませんでした。")?;
  let msg = &choice.message;
  if let Some(tcs) = &msg.tool_calls {
   if !tcs.is_empty() {
    let assistant = tools::response_message_to_assistant_request(msg);
    request
     .messages
     .push(ChatCompletionRequestMessage::Assistant(assistant));
    for tc in tcs {
     if let Some((id, out)) = tools::dispatch_tool_call(tc, &declared, tool_ctx).await {
      request.messages.push(ChatCompletionRequestMessage::Tool(ChatCompletionRequestToolMessage {
       content: ChatCompletionRequestToolMessageContent::Text(out),
       tool_call_id: id,
      }));
     }
    }
    continue;
   }
  }
  return Ok(msg.content.clone().unwrap_or_default());
 }
 bail!("OpenAI ツール呼び出しのラウンド上限（{}）に達しました。", MAX_TOOL_ROUNDS);
}

pub(crate) async fn retry_if_gpt5_empty_content(
 client: &Client<OpenAIConfig>,
 model_id: &str,
 latest_user: &str,
 orig_max_tokens: Option<u16>,
) -> Option<String> {
 if !model_policy::should_retry_on_empty_assistant(Some(model_id)) {
  return None;
 }
 let mut retry_builder = CreateChatCompletionRequestArgs::default();
 retry_builder.model(model_id.to_string());
 retry_builder.response_format(ResponseFormat::Text);
 if let Some(orig_max) = orig_max_tokens {
  let reduced = std::cmp::min(orig_max, 128);
  retry_builder.max_completion_tokens(reduced);
 }
 let sys = ChatCompletionRequestSystemMessageArgs::default()
  .content(
   "You are a helpful assistant. Provide a concise final answer. If reasoning consumed tokens, output the answer briefly now. 日本語入力には日本語で返答して下さい。",
  )
  .build()
  .ok()
  .map(ChatCompletionRequestMessage::System)?;
 let user = ChatCompletionRequestUserMessageArgs::default()
  .content(latest_user.to_string())
  .build()
  .ok()
  .map(ChatCompletionRequestMessage::User)?;
 retry_builder.messages(vec![sys, user]);
 let retry_req = retry_builder.build().ok()?;
 let retry_res = client.chat().create(retry_req).await.ok()?;
 retry_res
  .choices
  .first()?
  .message
  .content
  .clone()
  .filter(|c| !c.trim().is_empty())
}

pub(crate) fn extract_assistant_text(response: &CreateChatCompletionResponse) -> Result<String> {
 let content = response
  .choices
  .first()
  .context("AI からの応答はありましたが回答がありませんでした。")?
  .message
  .content
  .clone()
  .context("AI からの応答はありましたが無言の回答でした。")?;
 Ok(content)
}

const OVERFLOW_SUMMARY_SYSTEM: &str = "\
あなたは会話ログの要約係です。与えられたログは、メモリ窓の上限で直近の対話から落とされた古い発話です。\
後続の対話と矛盾しないよう、事実・話題・必要なら固有名詞だけを残し、日本語で 2〜5 文以内に要約してください。\
メタ発言や挨拶は省き、内容のみ。";

pub(crate) async fn summarize_overflow_turns(
 client: &Client<OpenAIConfig>,
 model: &str,
 max_completion_tokens: Option<u16>,
 user_payload: &str,
) -> Result<String> {
 let mut builder = CreateChatCompletionRequestArgs::default();
 builder.model(model.to_string());
 let mt = max_completion_tokens.or(Some(256));
 model_policy::apply_model_chat_options(&mut builder, model, mt);
 builder.temperature(0.2);
 let sys = ChatCompletionRequestSystemMessageArgs::default()
  .content(OVERFLOW_SUMMARY_SYSTEM.to_string())
  .build()
  .ok()
  .map(ChatCompletionRequestMessage::System)
  .context("overflow summary: system message")?;
 let user = ChatCompletionRequestUserMessageArgs::default()
  .content(user_payload.to_string())
  .build()
  .ok()
  .map(ChatCompletionRequestMessage::User)
  .context("overflow summary: user message")?;
 builder.messages(vec![sys, user]);
 let req = builder.build().context("overflow summary: build request")?;
 let res = create_chat_completion(client, req).await?;
 extract_assistant_text(&res)
}
