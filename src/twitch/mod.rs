//! 《Twitch》モジュール（Phase ζ-1 新設）。
//!
//! 旧 `src/processor/twitch*.rs` 3 本を本モジュール配下へ集約し、Flowgraph ブリッジ /
//! AI tool / Control API / EventSub すべてがここを参照するようにする。
//!
//! - [`eventsub`]: EventSub WebSocket 接続とイベントハンドリング。ζ-1 時点では V1 起動経路
//!   （`conf.twitch.eventsub` / `[[processors]].twitch_eventsub`）から引き続き使われる。
//! - [`oauth`]: DCF / refresh / キャッシュ管理。`OAuthIdent` を label-agnostic 化したので
//!   任意の `token_key` ベースで扱える。
//!
//! なお Twitch IRC ingress は ζ-1 で `src/bridges/twitch.rs` + `flowgraph.ingress.twitch` に
//! 完全移行したため、旧 `chat` モジュールは削除した。
pub mod eventsub;
pub mod oauth;
