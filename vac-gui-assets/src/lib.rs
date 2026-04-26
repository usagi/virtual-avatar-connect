//! Vite 成果物 `gui/dist` のビルド時埋め込み（[`include_dir`]）。
//!
//! `virtual-avatar-connect` の feature **`embed-gui`** 経由でのみ依存される。
//! このクレート単体をビルドする場合も `../gui/dist/index.html` が必要（`build.rs`）。

use include_dir::{include_dir, Dir};

/// リポジトリ直下の `gui/dist` を同梱したディレクトリツリー。
pub static GUI_DIST: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../gui/dist");
