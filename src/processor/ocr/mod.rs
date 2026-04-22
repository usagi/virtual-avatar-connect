//! Phase δ-9 D.2/D.3: V1 `Ocr` プロセッサ本体は除去。Flowgraph
//! `flowgraph.ocr.recognize` ノードから参照されるヘルパー（ソース解決・Windows Media OCR 呼び出し）
//! のみをユーティリティモジュールとして残す。

pub(crate) mod data_urls;
pub(crate) mod local;
pub(crate) mod web;

use anyhow::Result;
use async_tempfile::TempFile;
use std::path::Path;

/// OCR 対象となる画像ソースを表す。`data_urls::get` / `local::get` / `web::get` の戻り値型。
pub enum PathOrTempFileWithMime {
 Path(String, String),
 TempFile(TempFile, String),
}

/// 画像ファイルを Windows Media OCR で認識し、テキストを返す。
///
/// - `path`: 認識対象ファイルのパス
/// - `mime`: MIME タイプ
/// - `lang`: BCP-47 の言語タグ（例: `"ja-JP"`, `"en-US"`）
/// - `lines`: true で改行単位の結果を返す、false で全文を 1 行に連結
///
/// **Note**: 本実装は `windows` crate 0.62 以降で必要な `windows-future` 経由の同期 API へ追従中。
/// v0.9.x 系の過渡期ではプレースホルダとしてエラーを返し、Flowgraph `ocr.recognize` ノード側で
/// `on_error` が発火する。δ-9.3 の GUI / Flowgraph 仕上げと同時に本実装を復活させる予定。
#[cfg(target_os = "windows")]
pub fn recognize<P: AsRef<Path>, M: AsRef<str>>(path: P, mime: M, lang: &str, lines: bool) -> Result<String> {
 let _ = (path.as_ref(), mime.as_ref(), lang, lines);
 anyhow::bail!(
  "ocr::recognize: Windows Media OCR バインディングは windows crate 0.62 migration のため一時停止中。δ-9.3 で再実装予定。"
 )
}
