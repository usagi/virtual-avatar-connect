//! `CreateResponseRequest.input` 配列の要素型。
//!
//! OpenAI Responses API の `input_item` に対応。Chat Completions の
//! `ChatCompletionRequestMessage::{System,User,Assistant,Tool}` を置換する。

use serde::{Deserialize, Serialize};

/// `input[]` の 1 要素。
///
/// # Variants
///
/// - [`InputItem::Message`] — 通常メッセージ。`role` は `"system"` /
///   `"user"` / `"assistant"` / `"developer"` を想定（API は任意文字列を許容）。
/// - [`InputItem::FunctionCall`] — 過去の assistant 発話に含まれた function_call を
///   次ラウンドの input に積み直すときに使う（`previous_response_id` 非採用のため
///   client 側で会話履歴を完全再構築する方針）。
/// - [`InputItem::FunctionCallOutput`] — tool 側が返した function call の結果。
///   Chat Completions の `ChatCompletionRequestToolMessage` の置換。
///
/// シリアライズは常に `type` tag 付き。デシリアライズも `type` が必須
/// （OpenAI API は `type` 無し message も許容するが、API 自身は常に
/// `type` を付けて返してくるので実害はない）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputItem
{
 Message
 {
  role: String,
  content: InputContent,
 },
 FunctionCall
 {
  /// OpenAI がラウンド跨ぎで function call を識別するための id。
  call_id: String,
  /// 関数名（`Tool::Function.name` と一致）。
  name: String,
  /// JSON 文字列（関数引数）。
  arguments: String,
 },
 FunctionCallOutput
 {
  call_id: String,
  /// Tool 実行結果。通常は JSON 文字列化した値。
  output: String,
 },
 /// 前ラウンドの `OutputItem::Reasoning` を次ラウンドの input に詰め直すための variant。
 ///
 /// Phase ψ-α の **gpt-5 系 tool loop の round 間 pass-through** 専用。
 /// 通常の `react()` 経路（system / user / assistant メッセージ）では使わない。
 /// 詳細: [`docs/roadmap/phase-psi-alpha-encrypted-reasoning.md`](../../docs/roadmap/phase-psi-alpha-encrypted-reasoning.md)
 ///
 /// OpenAI 側の仕様: round 間で reasoning item をそのまま積み直すと、
 /// サーバ側が関連する reasoning のみ context に残してくれる。
 Reasoning
 {
  /// OpenAI が発行する reasoning item id（`rs_...`）。
  id: String,
  /// `include: ["reasoning.encrypted_content"]` 指定時のみ付与される blob。
  /// `store: false` + `include` の組み合わせで stateless に reasoning state を維持する。
  #[serde(skip_serializing_if = "Option::is_none", default)]
  encrypted_content: Option<String>,
  /// Reasoning summary（`summary_text` 等）。pass-through 時はそのまま透過する。
  #[serde(skip_serializing_if = "Option::is_none", default)]
  summary: Option<serde_json::Value>,
 },
}

/// `InputItem::Message.content` のフィールド。
///
/// OpenAI 仕様は文字列 or 配列（multipart）を許容するので `untagged` で両対応する。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum InputContent
{
 /// 単純文字列（短縮記法）。
 Text(String),
 /// マルチパート（`input_text` / `input_image` / ...）。
 Parts(Vec<InputContentPart>),
}

/// マルチパート content の 1 要素。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum InputContentPart
{
 InputText
 {
  text: String,
 },
 InputImage
 {
  image_url: String,
  #[serde(skip_serializing_if = "Option::is_none", default)]
  detail: Option<String>,
 },
}

impl From<String> for InputContent
{
 fn from(s: String) -> Self
 {
  InputContent::Text(s)
 }
}

impl From<&str> for InputContent
{
 fn from(s: &str) -> Self
 {
  InputContent::Text(s.to_string())
 }
}

impl InputItem
{
 /// 短縮コンストラクタ: `InputItem::message("user", "Hello")`。
 pub fn message(role: impl Into<String>, content: impl Into<InputContent>) -> Self
 {
  InputItem::Message {
   role: role.into(),
   content: content.into(),
  }
 }

 /// 短縮コンストラクタ: function call の結果を input に積み直す。
 pub fn function_call_output(call_id: impl Into<String>, output: impl Into<String>) -> Self
 {
  InputItem::FunctionCallOutput {
   call_id: call_id.into(),
   output: output.into(),
  }
 }
}
