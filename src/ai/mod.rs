//! 常駐 AI サービス（旧 `feature = "openai-chat"` プロセッサー）。
//!
//! Phase I (2026-04-17): プロセッサー列から切り離し、`State` の `broadcast` から流れる [`Observation`] を
//! 聞き続ける **常駐タスク**（[`service::AiService`]）へ昇格させた。
//!
//! # 構造
//!
//! - [`config`] — `[[ai.personas]]` の設定型
//! - [`observe`] — [`Observation`] と [`ObserveSet`]（triggers / include_all / include_additional / exclude）
//! - [`service`] — [`AiService`] 本体と `spawn_all` エントリ
//! - `context` / `completion` / `model_policy` / `tools` — OpenAI Chat API 周辺（テスト対象含む）
//! - [`fine_tuning`] — CLI の fine-tune / OpenAI Files 一括削除（persona_id 指定）

pub(crate) mod config;
pub(crate) mod observe;

mod completion;
mod context;
mod decision;
mod model_policy;
pub(crate) mod reload;
mod service;
mod tools;

// Phase χ: OpenAI Responses API（`/v1/responses`）の自前 `reqwest + 自前 DTO` 実装。
// `crate::*` には依存せず（`SharedState` / `ChannelDatum` 非参照）、
// 将来 `vac-openai-responses` crate 等に切り出せる境界を保つ。
// 詳細: docs/roadmap/phase-chi-openai-responses.md §3。
pub(crate) mod openai_responses;

pub mod fine_tuning;

pub use config::AiConf;
pub use observe::Observation;
pub use reload::{AiReloadHandle, AiReloadReport, AiReloadRequest};
pub use service::spawn_all;

pub(crate) const ENV_OPENAI_API_KEY: &str = "VAC_OPENAI_API_KEY";
