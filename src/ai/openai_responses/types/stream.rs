//! SSE ストリームイベント型。
//!
//! **χ-2 で実装予定**（χ-1 時点ではプレースホルダ）。
//!
//! 実装予定の `StreamEvent` enum は以下 7 変種をカバーする:
//!
//! - `Created { response_id, model }`
//! - `InProgress`
//! - `OutputItemAdded { index, item_kind, call_id, name }`
//! - `OutputTextDelta { item_id, delta }`
//! - `FunctionCallArgumentsDelta { call_id, delta }`
//! - `OutputItemDone { index, item }`
//! - `Completed { response_id, usage }`
//! - `Failed { error }`
//! - `Other { raw_type }`（未知イベント catch-all）
//!
//! 詳細: `docs/roadmap/phase-chi-openai-responses.md` §5。
