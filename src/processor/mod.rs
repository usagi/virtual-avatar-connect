//! Phase δ-9 D.2/D.3 (v1.0 前): V1 `Processor` trait / `ProcessorKind` / dispatch ループを除去した後の
//! **薄いモジュール群**。ここに残っているのは以下の 2 種類だけ:
//!
//! 1. **ingress 系**: まだ Flowgraph bridge が完全実装されていない voice / twitch (IRC + EventSub) を
//!    V1 `[[processors]]` 経由で起動するための spawn ロジック。bridges::voice / bridges::twitch 完成後に
//!    bridge 側から本ファイル群の `spawn_*` を呼び出すよう切り替える想定。
//! 2. **共通ユーティリティ**: `voicevox_engine` は Flowgraph の `tts.speak` ドライバ (VoiceVox) が直接呼ぶ。
//!
//! V1 変換系 (Modify/Command/翻訳/TTS/OCR/Screenshot など) は全て Flowgraph ノードに置き換わったため削除済み。
//! ただし `ocr` / `screenshot` モジュールは Flowgraph ノード実装から直接呼ばれる低レベルユーティリティ
//! として残している（Windows Media OCR バインディング、Win32 GDI キャプチャなど）。

pub(crate) mod voicevox_engine;
pub(crate) mod ingress;
pub(crate) mod ocr;
pub(crate) mod screenshot;
#[cfg(feature = "voice-vosk")]
mod voice_vosk_model;
#[cfg(feature = "voice-vosk")]
mod voice_vosk;
#[cfg(feature = "voice-whisper")]
mod voice_whisper_model;
#[cfg(feature = "voice-whisper")]
mod voice_whisper;
pub(crate) mod voice;

pub use voicevox_engine::{VoicevoxSpeaker, VoicevoxStyle};
