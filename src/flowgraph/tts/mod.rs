//! VAC Flowgraph 統合 TTS レイヤ（δ-4c）。
//!
//! V1 の 5 種 TTS プロセッサ（`os_tts` / `bouyomichan` / `voicevox` / `coeiroink` /
//! `aivis_speech`）を **単一 EffectfulNode `flowgraph.tts.speak`** に統合し、
//! エンジンごとの差分は `TtsDriver` trait として差替え可能にしたもの。
//!
//! - ノード: `flowgraph::nodes::tts::TtsSpeakNode`
//! - 基盤トレイト: [`driver::TtsDriver`]
//! - レジストリ: [`registry::registry`]（プロセス全体で 1 インスタンス）
//!
//! ### 追加エンジンの登録
//!
//! 1. `drivers/<name>.rs` に `TtsDriver` を実装した unit struct を作成。
//! 2. `drivers/mod.rs` で `pub mod <name>;` を追加。
//! 3. [`registry::default_registry`] の初期化リストに追記。
//!
//! `flowgraph.tts.speak` ノード側は **一切触らずに** 新エンジンを増やせる。
//! VoicePeak（δ-4c.1）はこの追加パス 3 ステップで実装済み（`drivers::voicepeak`）。

pub mod driver;
pub mod drivers;
pub mod registry;

pub use driver::{TtsDriver, TtsError, TtsOutcome, TtsRequest};
pub use registry::{registry, TtsRegistry};
