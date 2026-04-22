//! Phase δ-9 D.2/D.3: V1 `Screenshot` プロセッサ本体は除去。Flowgraph
//! `flowgraph.capture.screenshot_window` ノードから参照されるプラットフォーム固有のキャプチャ実装
//! のみをユーティリティモジュールとして残す。
//!
//! 当面は Windows 実装 (`windows.rs`) のみ参照対象。他 OS 向けの直接撮影は `screenshots` crate を
//! Flowgraph ノード側で使う。

#[allow(dead_code)]
mod ops;
#[cfg(target_os = "windows")]
pub(crate) mod windows;
