//! OpenAI Responses API (`POST /v1/responses`) の自前 client 実装。
//!
//! # 設計方針（Phase χ）
//!
//! - `reqwest` + 自前 DTO で `/v1/responses` を扱う（Option A）。
//!   `async-openai` crate は Files / Fine-tuning 用に v0.34 を温存。
//! - **`crate::*` には依存しない**: `SharedState` / `ChannelDatum` /
//!   `AiRuntime` / `ConfLayout` 等を import しない。
//!   将来 `vac-openai-responses` crate への切り出しを容易にする。
//! - エラー型は `thiserror::Error` ベースの自己完結 enum。
//!   `anyhow` への変換はモジュール境界で呼び出し側が行う。
//!
//! # モジュール構成
//!
//! - [`types`] — `CreateResponseRequest` / `Response` / `InputItem` / `OutputItem` 等の DTO。
//!   ストリーミングイベント `StreamEvent` は χ-2 で追加。
//! - [`client`] — `ResponsesClient` と non-stream `create`。`create_stream` は χ-2 で追加。
//! - [`util`] — `extract_output_text` / `truncate_error_body` 等の共用ヘルパ。
//! - [`sse`] — SSE パーサ（χ-2 で実装）。
//!
//! # 使用例（χ-1 時点）
//!
//! ```ignore
//! use virtual_avatar_connect::ai::openai_responses::{
//!     ResponsesClient, ResponsesClientConfig,
//!     types::{CreateResponseRequest, InputItem, InputContent},
//! };
//!
//! let cfg = ResponsesClientConfig {
//!     api_key: std::env::var("OPENAI_API_KEY").unwrap(),
//!     ..Default::default()
//! };
//! let client = ResponsesClient::new(cfg)?;
//!
//! let req = CreateResponseRequest {
//!     model: "gpt-4.1-mini".to_string(),
//!     input: vec![InputItem::Message {
//!         role: "user".to_string(),
//!         content: InputContent::Text("Hello".to_string()),
//!     }],
//!     max_output_tokens: Some(128),
//!     ..Default::default()
//! };
//! let response = client.create(req).await?;
//! let text = virtual_avatar_connect::ai::openai_responses::util::extract_output_text(&response);
//! ```
//!
//! 詳細仕様は `docs/roadmap/phase-chi-openai-responses.md` を参照。

pub mod client;
pub mod sse;
pub mod types;
pub mod util;

pub use client::{ResponsesClient, ResponsesClientConfig, ResponsesClientError};
pub use util::{extract_output_text, truncate_error_body};

#[cfg(test)]
mod tests;
