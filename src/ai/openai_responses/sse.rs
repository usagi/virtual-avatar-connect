//! Server-Sent Events (SSE) パーサ。
//!
//! **χ-2 で実装予定**（χ-1 時点ではプレースホルダ）。
//!
//! `eventsource-stream` crate で `reqwest::Response` の bytes stream を SSE イベント単位に
//! 割り、各 `data:` ペイロードを `serde_json` で parse して
//! [`crate::ai::openai_responses::types::stream::StreamEvent`] に変換する。
//!
//! 詳細: `docs/roadmap/phase-chi-openai-responses.md` §5。
