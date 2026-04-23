//! Server-Sent Events (SSE) パーサ。
//!
//! # 二層設計
//!
//! - [`parse_event_data`] — イベント type 文字列と JSON データを受け取って [`StreamEvent`]
//!   に変換する。**純関数**。入出力が小さいので unit test しやすい。
//! - [`stream_events`] — `reqwest::Response` の bytes_stream を受け取り、
//!   `eventsource_stream::Eventsource` で SSE Event 単位に割り、`[DONE]` を stream 終端、
//!   `ping` / `retry` 等のノイズをフィルタしつつ [`StreamEvent`] のストリームに落とす。
//!
//! # 仕様メモ
//!
//! - OpenAI Responses API は各イベントに `event:` ヘッダと `data:` JSON を送る
//! - `data:` JSON 内に `type` field があり、これがイベント種別の正本
//! - 稀に `event:` ヘッダの無い生 JSON が来ても、`data:` 内 `type` で分岐できる
//! - `[DONE]` を送るのは Chat Completions の慣習だが、Responses でも互換受信する
//!
//! 詳細: `docs/roadmap/phase-chi-openai-responses.md` §5。

use eventsource_stream::Eventsource;
use futures::{Stream, StreamExt};

use super::types::stream::{
 ErrorEnvelope, FunctionCallArgumentsDeltaPayload, FunctionCallArgumentsDonePayload,
 OutputItemEnvelope, OutputTextDeltaPayload, OutputTextDonePayload, ResponseEnvelope, StreamEvent,
};
use super::util::truncate_error_body;

/// SSE レイヤーで起きうる失敗。
#[derive(Debug, thiserror::Error)]
pub enum SseError
{
 #[error("SSE JSON parse error ({kind}): {error} — body: {body}")]
 Json
 {
  /// イベント種別 or "unknown"。
  kind: String,
  error: String,
  body: String,
 },
 #[error("SSE transport error: {0}")]
 Transport(String),
}

/// JSON string を [`StreamEvent`] に変換する純関数。
///
/// # 引数
///
/// - `event_type`: SSE `event:` ヘッダで受け取った文字列（例: `"response.output_text.delta"`）。
///   空でも OK（その場合は `data` 内 `type` field にフォールバック）。
/// - `data`: SSE `data:` で受け取った JSON payload 全文。
///
/// # 戻り値
///
/// 未知イベントは [`StreamEvent::Other`] に落とす（error にしない）。
/// JSON parse エラーや必須 field 欠落は [`SseError::Json`]。
pub fn parse_event_data(event_type: &str, data: &str) -> Result<StreamEvent, SseError>
{
 let value: serde_json::Value = serde_json::from_str(data).map_err(|e| SseError::Json {
  kind: event_type.to_string(),
  error: e.to_string(),
  body: truncate_error_body(data),
 })?;

 let effective_type = if !event_type.is_empty() {
  event_type.to_string()
 } else {
  value
   .get("type")
   .and_then(|v| v.as_str())
   .map(ToString::to_string)
   .unwrap_or_default()
 };

 parse_typed_value(&effective_type, value, data)
}

fn parse_typed_value(
 event_type: &str,
 value: serde_json::Value,
 raw_data: &str,
) -> Result<StreamEvent, SseError>
{
 let err = |kind: &str, e: serde_json::Error| SseError::Json {
  kind: kind.to_string(),
  error: e.to_string(),
  body: truncate_error_body(raw_data),
 };

 Ok(match event_type {
  "response.created" => {
   let env: ResponseEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::Created { response: env.response }
  }
  "response.in_progress" => {
   let env: ResponseEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::InProgress { response: env.response }
  }
  "response.completed" => {
   let env: ResponseEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::Completed { response: env.response }
  }
  "response.incomplete" => {
   let env: ResponseEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::Incomplete { response: env.response }
  }
  "response.failed" => {
   let env: ResponseEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::Failed { response: env.response }
  }
  "response.output_item.added" => {
   let env: OutputItemEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::OutputItemAdded {
    output_index: env.output_index,
    item: env.item,
   }
  }
  "response.output_item.done" => {
   let env: OutputItemEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::OutputItemDone {
    output_index: env.output_index,
    item: env.item,
   }
  }
  "response.output_text.delta" => {
   let p: OutputTextDeltaPayload =
    serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::OutputTextDelta {
    item_id: p.item_id,
    output_index: p.output_index,
    content_index: p.content_index,
    delta: p.delta,
   }
  }
  "response.output_text.done" => {
   let p: OutputTextDonePayload =
    serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::OutputTextDone {
    item_id: p.item_id,
    output_index: p.output_index,
    content_index: p.content_index,
    text: p.text,
   }
  }
  "response.function_call_arguments.delta" => {
   let p: FunctionCallArgumentsDeltaPayload =
    serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::FunctionCallArgumentsDelta {
    item_id: p.item_id,
    output_index: p.output_index,
    delta: p.delta,
   }
  }
  "response.function_call_arguments.done" => {
   let p: FunctionCallArgumentsDonePayload =
    serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::FunctionCallArgumentsDone {
    item_id: p.item_id,
    output_index: p.output_index,
    arguments: p.arguments,
   }
  }
  "error" => {
   let env: ErrorEnvelope = serde_json::from_value(value).map_err(|e| err(event_type, e))?;
   StreamEvent::Error { error: env.error }
  }
  other => StreamEvent::Other {
   raw_type: other.to_string(),
  },
 })
}

/// `reqwest::Response` の bytes stream を受け取り、`Result<StreamEvent, SseError>`
/// の stream を返す。
///
/// # フィルタリング
///
/// - `[DONE]` 受信で stream 終了（`None` を返す）
/// - 空 data / `ping` / `retry` 通知はスキップ
pub fn stream_events<S, E>(bytes: S) -> impl Stream<Item = Result<StreamEvent, SseError>>
where
 S: Stream<Item = Result<bytes::Bytes, E>> + Unpin + Send + 'static,
 E: std::error::Error + Send + Sync + 'static,
{
 bytes.eventsource().filter_map(|event| async move {
  match event {
   Err(e) => Some(Err(SseError::Transport(e.to_string()))),
   Ok(ev) => {
    let data = ev.data.trim();
    if data.is_empty() || data == "[DONE]" {
     return None;
    }
    Some(parse_event_data(&ev.event, data))
   }
  }
 })
}
