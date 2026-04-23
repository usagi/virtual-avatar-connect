//! Phase χ 単体テストのルート。
//!
//! - [`golden`] — OpenAI 公式サンプル風の payload で DTO の serde roundtrip を検証（χ-1）
//! - `stream` — SSE parser / StreamEvent の検証（χ-2 で追加予定）

mod golden;
