//! `POST /v1/responses` のリクエスト本体とその構成要素。

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::input::InputItem;

/// `POST /v1/responses` の request body。
///
/// Chat Completions の `CreateChatCompletionRequest` を置換する。
/// 主な命名差:
///
/// | Chat Completions | Responses |
/// |---|---|
/// | `messages` | `input` |
/// | `max_tokens` | `max_output_tokens` |
/// | `response_format` | `text.format`（[`TextConfig`]） |
/// | （なし） | `reasoning.effort`（gpt-5 系） |
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq)]
pub struct CreateResponseRequest
{
 pub model: String,

 pub input: Vec<InputItem>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub max_output_tokens: Option<u32>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub temperature: Option<f32>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub top_p: Option<f32>,

 /// gpt-5 系の thinking token 制御。
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub reasoning: Option<Reasoning>,

 /// Structured Outputs（JSON mode / JSON schema）の指定。
 /// 旧 `response_format` の置換。
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub text: Option<TextConfig>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub tools: Option<Vec<Tool>>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub tool_choice: Option<ToolChoice>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub parallel_tool_calls: Option<bool>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub stream: Option<bool>,

 /// サーバ側に会話履歴を保存するか。VAC は client 側で全履歴を構築するため
 /// `false` を推奨。`None` の場合は API 既定（`true`）に委ねる。
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub store: Option<bool>,

 /// 過去レスポンス参照。本 Phase χ では未採用（`None` 固定運用）。
 /// 将来 Phase ψ+ で server-side conversation と組み合わせる候補。
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub previous_response_id: Option<String>,

 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub metadata: Option<HashMap<String, String>>,

 /// System prompt / persona instructions を input と別に載せるための field。
 /// VAC では通常 `InputItem::Message { role: "system"/"developer" }` を使うが、
 /// API 互換のため保持する。
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub instructions: Option<String>,
}

/// gpt-5 系の thinking token 制御。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct Reasoning
{
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub effort: Option<ReasoningEffort>,

 /// `"auto"` / `"concise"` / `"detailed"` 等、モデル固有値を許容。
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub summary: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort
{
 Minimal,
 Low,
 Medium,
 High,
}

/// Structured Outputs（JSON mode / JSON schema）の指定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TextConfig
{
 #[serde(skip_serializing_if = "Option::is_none", default)]
 pub format: Option<TextFormat>,
}

/// `text.format` の 3 種。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TextFormat
{
 /// 自由文（既定）。
 Text,
 /// JSON object を返す（JSON Schema 指定なし）。
 JsonObject,
 /// JSON Schema で strict 拘束する。
 JsonSchema
 {
  name: String,
  schema: serde_json::Value,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  strict: Option<bool>,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  description: Option<String>,
 },
}

/// Tool definition（function / custom）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Tool
{
 /// 通常の OpenAI function tool（JSON Schema 引数）。
 Function
 {
  name: String,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  description: Option<String>,
  /// JSON Schema。
  parameters: serde_json::Value,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  strict: Option<bool>,
 },
 /// gpt-5 系の freeform custom tool（context-free grammar 等）。
 Custom
 {
  name: String,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  description: Option<String>,
  /// format 指定（CFG 等）。自由 JSON。
  #[serde(skip_serializing_if = "Option::is_none", default)]
  format: Option<serde_json::Value>,
 },
}

/// Tool 選択モード。`"auto"` / `"none"` / `"required"` と特定 tool 強制の 2 系統。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ToolChoice
{
 Mode(ToolChoiceMode),
 Named(NamedToolChoice),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ToolChoiceMode
{
 Auto,
 None,
 Required,
}

/// 特定 tool を強制選択する場合の指定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum NamedToolChoice
{
 Function
 {
  name: String,
 },
 Custom
 {
  name: String,
 },
}

impl Default for ReasoningEffort
{
 fn default() -> Self
 {
  ReasoningEffort::Medium
 }
}
