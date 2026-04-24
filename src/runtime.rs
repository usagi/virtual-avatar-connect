//! ランタイム一時ディレクトリの生成と管理。
//!
//! - ルート: 既定で `dirs::data_local_dir()/virtual-avatar-connect/runtime`（Windows では `%LOCALAPPDATA%`）。
//!   設定で `runtime_dir` が指定されていれば優先する。
//! - セッションごとにサブディレクトリ `<root>/<session_id>/` を切る。session_id はプロセス起動時に生成。
//! - 起動時に、**現在のセッション以外**のサブディレクトリのうち最終更新が 24 時間を超えるものを削除する（ベストエフォート）。
//! - Phase 3 時点では「基盤だけ」を用意し、実際にファイルを置く Producer は Phase 4 以降で順次対応する。

use crate::Conf;
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const DEFAULT_APP_SUBDIR: &str = "virtual-avatar-connect";
const DEFAULT_RUNTIME_SUBDIR: &str = "runtime";
/// 既定のインライン添付の最大バイト数。`Conf::attachment_inline_max_bytes` で上書き可能。
pub const DEFAULT_INLINE_MAX_BYTES: u64 = 32 * 1024;
const STALE_SESSION_SECS: u64 = 24 * 60 * 60;

/// プロセス生存期間中に共有する一時ディレクトリ情報。
///
/// `session_dir` 配下に添付（音声・画像など）を書き出し、`href` ベースで Web UI から配信する想定（Phase 4 以降）。
#[derive(Debug, Clone)]
pub struct RuntimePaths {
 /// 全セッション共通のルート（例: `%LOCALAPPDATA%/virtual-avatar-connect/runtime`）。
 pub root: PathBuf,
 /// 本プロセス専用のセッションディレクトリ（`root/<session_id>`）。
 pub session_dir: PathBuf,
 /// 本プロセスのセッション ID（`YYYYMMDDTHHMMSSZ-<rand8>`）。
 pub session_id: String,
 /// インライン添付の最大バイト数。これを超えるバイナリは自動で `File` 添付に降格する運用を想定。
 pub inline_max_bytes: u64,
}

impl RuntimePaths {
 /// 設定から一時ディレクトリ情報を初期化し、セッションディレクトリを作成して返す。
 ///
 /// 既定ルートが取得できないプラットフォームでは `./.vac-runtime/` にフォールバックする。
 pub fn init(conf: &Conf) -> Result<Self> {
  let root = resolve_root(conf)?;
  std::fs::create_dir_all(&root)?;

  let session_id = new_session_id();
  let session_dir = root.join(&session_id);
  std::fs::create_dir_all(&session_dir)?;

  let inline_max_bytes = conf.attachment_inline_max_bytes.unwrap_or(DEFAULT_INLINE_MAX_BYTES);

  cleanup_stale_sessions(&root, &session_id);

  log::info!(
   "《Runtime》 一時ディレクトリを初期化しました: root={:?} session_id={} inline_max_bytes={}",
   root,
   session_id,
   inline_max_bytes
  );

  Ok(Self {
   root,
   session_dir,
   session_id,
   inline_max_bytes,
  })
 }

 /// セッションディレクトリ配下に相対パスを解決する（ファイル自体は作成しない）。
 pub fn session_path<P: AsRef<Path>>(&self, relative: P) -> PathBuf {
  self.session_dir.join(relative)
 }
}

fn resolve_root(conf: &Conf) -> Result<PathBuf> {
 if let Some(p) = conf.runtime_dir.as_ref() {
  return Ok(p.clone());
 }
 if let Some(base) = dirs::data_local_dir() {
  return Ok(base.join(DEFAULT_APP_SUBDIR).join(DEFAULT_RUNTIME_SUBDIR));
 }
 // プラットフォーム固有 API が無ければ cwd にフォールバック。
 Ok(PathBuf::from(".vac-runtime"))
}

fn new_session_id() -> String {
 let ts = jiff::Timestamp::now().strftime("%Y%m%dT%H%M%SZ").to_string();
 let rand: u32 = rand::random();
 format!("{}-{:08x}", ts, rand)
}

fn cleanup_stale_sessions(root: &Path, current_session_id: &str) {
 let Ok(entries) = std::fs::read_dir(root) else {
  return;
 };
 let now = SystemTime::now();
 let threshold = Duration::from_secs(STALE_SESSION_SECS);
 for ent in entries.flatten() {
  let path = ent.path();
  if !path.is_dir() {
   continue;
  }
  let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
   continue;
  };
  if name == current_session_id {
   continue;
  }
  let Ok(meta) = ent.metadata() else { continue };
  let modified = meta.modified().ok().unwrap_or(SystemTime::UNIX_EPOCH);
  let age = now.duration_since(modified).unwrap_or_default();
  if age < threshold {
   continue;
  }
  match std::fs::remove_dir_all(&path) {
   Ok(()) => log::debug!("《Runtime》 古いセッションディレクトリを削除しました: {:?}（age {}s）", path, age.as_secs()),
   Err(e) => log::debug!("《Runtime》 セッションディレクトリの削除に失敗（スキップ）: {:?} — {}", path, e),
  }
 }
}
