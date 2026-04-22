//! Phase VI-γ-1: 自己再起動 / プロファイル切替 API。
//!
//! ### 設計の狙い
//!
//! VAC は現状、`AI ペルソナ` と `modify ファイル` 以外の設定変更は **プロセス再起動** が必要。
//! 完全ホットリロード（ε 目標）への途中段階として、GUI からワンクリックで再起動できる形を用意する。
//!
//! 同一バイナリを新しい引数（= 新しい conf path）で `spawn` し、現プロセスを `graceful_ms` 後に
//! `exit(0)` する。GUI 側は WS の自動再接続で新インスタンスの Control API にそのまま吸い付く。
//!
//! ### エンドポイント
//!
//! - `GET  /api/v1/control/profiles` … 現 conf と同ディレクトリの `*.toml` を列挙し、どれが現行かを示す
//! - `POST /api/v1/control/restart`  … 自分自身を spawn → graceful_ms 後に exit
//!
//! ### スコープ外
//!
//! - ε 目標（in-process での完全 hot reload）は別タスク。ここはあくまで「**再起動をワンボタン化**」
//! - ノード位置などレイアウトは conf.toml に触れない（別ファイル `.layout.json` を γ-4a で導入）
//! - 書き戻し（toml_edit）も γ-4a 側のスコープ。ここは **読み取りと再起動** のみ

use std::path::{Path, PathBuf};

use actix_web::web::{self, Data, Json};
use actix_web::{get, post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use super::events::ControlEvent;
use crate::SharedState;

/// `POST /api/v1/control/restart` の body。すべて optional。
#[derive(Debug, Clone, Deserialize)]
pub struct RestartRequest {
 /// 新 conf への切替を指定するときだけ渡す。省略時は現 conf をそのまま引き継ぐ。
 ///
 /// 指定値は現 conf と同ディレクトリの相対パスか絶対パスを受け付けるが、`..` による
 /// ディレクトリ脱出や別ドライブ指定は弾く（= 現 conf の親ディレクトリの直下、または直接絶対）。
 #[serde(default)]
 pub conf: Option<String>,

 /// 現プロセスを落とすまでの猶予 ms。既定 800ms。
 /// actix-web の 200 応答と `ControlEvent::Restarting` の WS 伝播が完了するまで少し待つ。
 #[serde(default)]
 pub graceful_ms: Option<u64>,
}

const DEFAULT_GRACEFUL_MS: u64 = 800;
const MAX_GRACEFUL_MS: u64 = 10_000;

/// `POST /api/v1/control/restart` のレスポンス。
#[derive(Debug, Serialize)]
pub struct RestartResponse {
 /// 実際に spawn した新プロセスの引数（先頭要素を除く = conf path）。確認用。
 pub new_conf_path: String,
 /// 新プロセスの PID。Windows では spawn 直後で ≠ 実体のこともあるのであくまで参考値。
 pub new_pid: u32,
 /// 現プロセスが exit を試みるまでの猶予 ms。
 pub graceful_ms: u64,
 /// 現プロセス (= 呼び出し先) の PID。
 pub current_pid: u32,
}

/// `GET /api/v1/control/profiles` の応答エントリ。
#[derive(Debug, Serialize)]
pub struct ProfileEntry {
 /// 絶対パス文字列（表示/識別用）。
 pub path: String,
 /// ファイル名（拡張子付き）。
 pub filename: String,
 /// 人間向け表示名 — ファイル名から拡張子を落として先頭を大文字化した程度。
 pub label: String,
 /// バイト数。
 pub size: u64,
 /// 最終更新時刻（RFC3339、取得できなければ `None`）。
 pub modified: Option<String>,
 /// 現在読み込まれている conf と一致するか。
 pub is_current: bool,
}

#[derive(Debug, Serialize)]
pub struct ProfilesResponse {
 /// ディレクトリを走査した起点（絶対パス）。
 pub directory: Option<String>,
 /// 列挙された `*.toml` 群（モダンなソート: `conf.toml` を先頭、他は名前昇順）。
 pub entries: Vec<ProfileEntry>,
 /// 現 conf のパス（絶対）。`directory` 配下に居ないケースもあり得るので `entries` とは独立に返す。
 pub current: Option<String>,
}

#[get("/profiles")]
pub async fn get_profiles(state: Data<SharedState>) -> impl Responder {
 let current_path = {
  let s = state.read().await;
  s.conf_source_path.clone()
 };

 let Some(current) = current_path.clone() else {
  // Conf.source_path が未設定の状況は実運用では起きにくいが、起きても 200 で空応答にしておく。
  return HttpResponse::Ok().json(ProfilesResponse {
   directory: None,
   entries: Vec::new(),
   current: None,
  });
 };

 let dir = current.parent().map(|p| p.to_path_buf());
 let mut entries: Vec<ProfileEntry> = Vec::new();
 if let Some(ref d) = dir {
  match std::fs::read_dir(d) {
   Ok(iter) => {
    for ent in iter.flatten() {
     let p = ent.path();
     if !p.is_file() {
      continue;
     }
     if p.extension().and_then(|e| e.to_str()) != Some("toml") {
      continue;
     }
     let filename = p
      .file_name()
      .and_then(|s| s.to_str())
      .unwrap_or("")
      .to_string();
     let label = label_from_filename(&filename);
     let meta = ent.metadata().ok();
     let size = meta.as_ref().map(|m| m.len()).unwrap_or(0);
     let modified = meta
      .as_ref()
      .and_then(|m| m.modified().ok())
      .map(|t| chrono::DateTime::<chrono::Utc>::from(t).to_rfc3339());
     let is_current = paths_equivalent(&p, &current);
     entries.push(ProfileEntry {
      path: p.display().to_string(),
      filename,
      label,
      size,
      modified,
      is_current,
     });
    }
   },
   Err(e) => {
    log::warn!("《Restart》 profiles: ディレクトリ走査に失敗: {} ({})", d.display(), e);
   },
  }
 }

 entries.sort_by(|a, b| {
  // conf.toml を最優先、次にアルファベット昇順。
  let a_is_canonical = a.filename.eq_ignore_ascii_case("conf.toml");
  let b_is_canonical = b.filename.eq_ignore_ascii_case("conf.toml");
  match (a_is_canonical, b_is_canonical) {
   (true, false) => std::cmp::Ordering::Less,
   (false, true) => std::cmp::Ordering::Greater,
   _ => a.filename.to_ascii_lowercase().cmp(&b.filename.to_ascii_lowercase()),
  }
 });

 HttpResponse::Ok().json(ProfilesResponse {
  directory: dir.map(|d| d.display().to_string()),
  entries,
  current: Some(current.display().to_string()),
 })
}

#[post("/restart")]
pub async fn post_restart(
 state: Data<SharedState>,
 body: Json<RestartRequest>,
) -> impl Responder {
 let req = body.into_inner();
 let graceful_ms = req
  .graceful_ms
  .unwrap_or(DEFAULT_GRACEFUL_MS)
  .min(MAX_GRACEFUL_MS);

 let current_conf_path = {
  let s = state.read().await;
  s.conf_source_path.clone()
 };
 let Some(current_conf_path) = current_conf_path else {
  return HttpResponse::InternalServerError().json(serde_json::json!({
   "error": "current conf path is unknown (source_path is None)",
  }));
 };

 // 新 conf の解決。省略時は現 conf をそのまま引き継ぐ（再起動のみ）。
 let new_conf_path = match &req.conf {
  Some(p) if !p.trim().is_empty() => match resolve_new_conf_path(&current_conf_path, p) {
   Ok(resolved) => resolved,
   Err(msg) => {
    return HttpResponse::BadRequest()
     .json(serde_json::json!({ "error": msg }));
   },
  },
  _ => current_conf_path.clone(),
 };

 // Load で軽く検証（deserialize 成功しなければ再起動させない）。
 // Phase VI-γ-1 時点では ε（hot reload）は未実装なので、ここで弾かないと
 // 起動できない conf に切り替えた VAC を GUI から手動起動し直す羽目になる。
 if let Err(e) = crate::Conf::new_noop_probe(&new_conf_path) {
  return HttpResponse::BadRequest().json(serde_json::json!({
   "error": format!("conf load failed: {e}"),
   "path": new_conf_path.display().to_string(),
  }));
 }

 let current_exe = match std::env::current_exe() {
  Ok(p) => p,
  Err(e) => {
   return HttpResponse::InternalServerError()
    .json(serde_json::json!({ "error": format!("current_exe() failed: {e}") }));
  },
 };

 let new_conf_arg = new_conf_path.display().to_string();
 // 現プロセスの CLI 引数から conf パスを除いた他のフラグを引き継ぐ。`args().next()` は自プロセス名なので捨てる。
 let mut extra_args: Vec<String> = std::env::args().skip(1).collect();
 // 旧 conf (最初の非フラグ引数) を除去し、新 conf を先頭に置く（clap の DEFAULT_CONF_PATH も上書き）。
 if let Some(pos) = extra_args.iter().position(|a| !a.starts_with('-')) {
  extra_args.remove(pos);
 }

 let mut cmd = std::process::Command::new(&current_exe);
 cmd.arg(&new_conf_arg);
 for a in extra_args.iter() {
  cmd.arg(a);
 }

 // Windows では CREATE_NEW_PROCESS_GROUP を付ける。
 //
 // 狙い:
 //   - 親のコンソール（= 起動元 PowerShell / cmd）を **継承** させ、同じターミナルに
 //     新プロセスのログが続けて出るようにする（stdio はデフォルトで親継承）。
 //   - ただしプロセスグループは分離しておく。これにより親プロセスに届いた Ctrl+C や
 //     WM_CLOSE が子に波及せず、親の `exit()` だけで子が道連れになることもない。
 //
 // 以前は `DETACHED_PROCESS` を指定していたが、それだと Windows のコンソール
 // サブシステムアプリには新しいコンソールが自動割り当てされ、
 // 「謎の黒いターミナルウィンドウが出現し、閉じてもプロセスは生きている」という
 // 迷子状態になっていた（= ユーザーが制御を失う）。
 //
 // stdio の継承は `std::process::Command` の既定なので明示不要。
 #[cfg(windows)]
 {
  use std::os::windows::process::CommandExt;
  const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
  cmd.creation_flags(CREATE_NEW_PROCESS_GROUP);
 }

 let child = match cmd.spawn() {
  Ok(c) => c,
  Err(e) => {
   return HttpResponse::InternalServerError().json(serde_json::json!({
    "error": format!("spawn failed: {e}"),
    "exe": current_exe.display().to_string(),
    "conf": new_conf_arg,
   }));
  },
 };
 let new_pid = child.id();

 // GUI への先出し通知。WS が配信完了する前に exit してしまうと届かないことがあるので、
 // `graceful_ms` を確保する意味でも送ってから寝る。
 let current_pid = std::process::id();
 let ev = ControlEvent::Restarting {
  new_conf_path: new_conf_arg.clone(),
  new_pid,
  graceful_ms,
  current_pid,
 };
 {
  let s = state.read().await;
  if let Err(e) = s.control_event_tx.send(ev) {
   log::trace!("《Restart》 control_event_tx.send(Restarting) に失敗（受信者 0 件の可能性）: {e}");
  }
 }

 log::info!(
  "《Restart》 新プロセス spawn 完了 (pid={new_pid}) conf={new_conf_arg}。{graceful_ms}ms 後に現プロセス(pid={current_pid}) を exit します。"
 );

 tokio::spawn(async move {
  tokio::time::sleep(std::time::Duration::from_millis(graceful_ms)).await;
  log::info!("《Restart》 graceful_ms 経過。現プロセスを終了します。");
  std::process::exit(0);
 });

 HttpResponse::Ok().json(RestartResponse {
  new_conf_path: new_conf_arg,
  new_pid,
  graceful_ms,
  current_pid,
 })
}

/// 新 conf path の安全性を検証したうえで絶対パスに解決する。
///
/// 受け入れるパターン:
///   - 絶対パス: そのまま（存在確認する）
///   - 相対: 現 conf の親ディレクトリからの相対（パス区切りを含むなら親ディレクトリ制約のみ）
///     - `..` で親を辿って現ディレクトリ外に脱出する指定は弾く
fn resolve_new_conf_path(current_conf: &Path, requested: &str) -> Result<PathBuf, String> {
 let p = Path::new(requested);
 let resolved = if p.is_absolute() {
  p.to_path_buf()
 } else {
  let parent = current_conf
   .parent()
   .ok_or_else(|| "current conf has no parent directory".to_string())?;
  parent.join(p)
 };

 // 正規化して脱出チェック。canonicalize は存在チェック付き。
 let canon = std::fs::canonicalize(&resolved)
  .map_err(|e| format!("cannot resolve conf path {}: {e}", resolved.display()))?;

 let parent_canon = current_conf
  .parent()
  .and_then(|p| std::fs::canonicalize(p).ok());
 if let Some(parent_canon) = parent_canon {
  if !canon.starts_with(&parent_canon) {
   return Err(format!(
    "conf path {} is outside the current conf directory ({})",
    canon.display(),
    parent_canon.display()
   ));
  }
 }

 if canon.extension().and_then(|e| e.to_str()) != Some("toml") {
  return Err(format!("conf path {} is not a .toml file", canon.display()));
 }
 if !canon.is_file() {
  return Err(format!("conf path {} is not a regular file", canon.display()));
 }

 Ok(canon)
}

/// `conf.example-twitch.toml` → "Conf Example Twitch" 程度。過度に凝らない。
fn label_from_filename(filename: &str) -> String {
 let stem = filename.trim_end_matches(".toml");
 // ドット/ダッシュ/アンダースコアで区切って単語化し、先頭大文字化。
 stem
  .split(['.', '-', '_'])
  .filter(|s| !s.is_empty())
  .map(|s| {
   let mut chars = s.chars();
   match chars.next() {
    Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
    None => String::new(),
   }
  })
  .collect::<Vec<_>>()
  .join(" ")
}

/// ディレクトリ/シンボリックリンク/大文字小文字を正規化したうえで 2 つのパスを比較する。
/// 失敗時は素の比較にフォールバック。
fn paths_equivalent(a: &Path, b: &Path) -> bool {
 match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
  (Ok(aa), Ok(bb)) => aa == bb,
  _ => a == b,
 }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
 cfg.service(get_profiles);
 cfg.service(post_restart);
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn label_from_filename_basic() {
  assert_eq!(label_from_filename("conf.toml"), "Conf");
  assert_eq!(label_from_filename("conf.example-twitch.toml"), "Conf Example Twitch");
  assert_eq!(label_from_filename("streaming_kaltsit.toml"), "Streaming Kaltsit");
  assert_eq!(label_from_filename(".toml"), "");
 }

 #[test]
 fn restart_request_accepts_empty_body() {
  let parsed: RestartRequest = serde_json::from_str("{}").unwrap();
  assert!(parsed.conf.is_none());
  assert!(parsed.graceful_ms.is_none());
 }

 #[test]
 fn restart_request_partial() {
  let parsed: RestartRequest =
   serde_json::from_str(r#"{"conf":"conf.example-twitch.toml"}"#).unwrap();
  assert_eq!(parsed.conf.as_deref(), Some("conf.example-twitch.toml"));
  assert!(parsed.graceful_ms.is_none());
 }
}
