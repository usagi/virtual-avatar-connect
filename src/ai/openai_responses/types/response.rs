//! `POST /v1/responses` のレスポンス本体（non-stream / stream 完了時）の DTO。

use serde::{Deserialize, Serialize};

/// Responses API の non-stream レスポンス本体。
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Response
{
 pub id: String,

 /// 期待値 `"response"`。他値は無視する想定。
 #[serde(default)]
 pub object: Option<String>,

 /// Unix epoch 秒。
 #[serde(default)]
 pub created_at: Option<i64>,

 #[serde(default)]
 pub model: Option<String>,

 /// 完了状況。
 #[serde(default)]
 pub status: Option<ResponseStatus>,

 #[serde(default)]
 pub error: Option<ErrorObject>,

 /// Responses API の convenience field（全 `message.output_text` item を連結した文字列）。
 /// 未返却の場合は `None`。その場合は [`crate::ai::openai_responses::util::extract_output_text`]
 /// が `output[]` を走査して組み立てる。
 #[serde(default)]
 pub output_text: Option<String>,

 /// 出力 item 配列。message / function_call / reasoning / 未知タイプ。
 #[serde(default)]
 pub output: Vec<OutputItem>,

 #[serde(default)]
 pub usage: Option<Usage>,

 #[serde(default)]
 pub incomplete_details: Option<IncompleteDetails>,

 /// サーバが採番した previous_response_id（`store=true` のときのみ）。
 #[serde(default)]
 pub previous_response_id: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ResponseStatus
{
 Queued,
 InProgress,
 Completed,
 Incomplete,
 Failed,
 Cancelled,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct IncompleteDetails
{
 #[serde(default)]
 pub reason: Option<String>,
}

/// `output[]` の 1 要素。
///
/// 未知タイプは [`OutputItem::Other`] に落ちる（forward compat）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum OutputItem
{
 Message
 {
  id: String,
  #[serde(default)]
  status: Option<String>,
  #[serde(default = "default_assistant_role")]
  role: String,
  content: Vec<MessageContent>,
 },
 FunctionCall
 {
  id: String,
  #[serde(default)]
  status: Option<String>,
  call_id: String,
  name: String,
  /// JSON 文字列。途中で parse しない（tool loop で必要な時にまとめて parse する）。
  arguments: String,
 },
 Reasoning
 {
  id: String,
  #[serde(default)]
  status: Option<String>,
  #[serde(default)]
  summary: Option<serde_json::Value>,
 },
 /// 未知タイプを受け取った場合の catch-all（web_search_call / file_search_call /
 /// code_interpreter_call / mcp_tool_call 等）。
 /// 将来フェーズで個別 variant に昇格予定。
 #[serde(other)]
 Other,
}

fn default_assistant_role() -> String
{
 "assistant".to_string()
}

/// `OutputItem::Message.content[]` の 1 要素。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum MessageContent
{
 OutputText
 {
  text: String,
  #[serde(default)]
  annotations: Option<Vec<serde_json::Value>>,
 },
 Refusal
 {
  refusal: String,
 },
 /// 未知の content part。
 #[serde(other)]
 Other,
}

impl OutputItem
{
 /// `FunctionCall` variant なら参照を返す（tool loop で抽出するときに使う）。
 pub fn as_function_call(&self) -> Option<FunctionCallView<'_>>
 {
  if let OutputItem::FunctionCall {
   id,
   call_id,
   name,
   arguments,
   status,
  } = self
  {
   Some(FunctionCallView {
    id,
    call_id,
    name,
    arguments,
    status: status.as_deref(),
   })
  }
  else
  {
   None
  }
 }
}

/// `OutputItem::FunctionCall` への borrowed view。
#[derive(Debug, Clone, Copy)]
pub struct FunctionCallView<'a>
{
 pub id: &'a str,
 pub call_id: &'a str,
 pub name: &'a str,
 pub arguments: &'a str,
 pub status: Option<&'a str>,
}

/// usage 情報。
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct Usage
{
 #[serde(default)]
 pub input_tokens: u32,
 #[serde(default)]
 pub output_tokens: u32,
 #[serde(default)]
 pub total_tokens: u32,
 #[serde(default)]
 pub input_tokens_details: Option<UsageInputDetails>,
 #[serde(default)]
 pub output_tokens_details: Option<UsageOutputDetails>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UsageInputDetails
{
 #[serde(default)]
 pub cached_tokens: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct UsageOutputDetails
{
 #[serde(default)]
 pub reasoning_tokens: u32,
}

/// エラーレスポンス / response.error / stream 失敗イベントで返る error 本体。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct ErrorObject
{
 #[serde(default)]
 pub code: Option<String>,
 #[serde(default)]
 pub message: Option<String>,
 #[serde(default, rename = "type")]
 pub kind: Option<String>,
 #[serde(default)]
 pub param: Option<String>,
}
