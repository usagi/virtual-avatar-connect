//! 同梱 TTS ドライバ群。新規追加は `<name>.rs` を作成し、`registry::default_registry` に 1 行足す。

pub mod aivis_speech;
pub mod bouyomichan;
pub mod coeiroink;
pub mod os;
pub mod voicepeak;
pub mod voicevox;
