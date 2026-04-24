//! Phase VI-γ-5a: `run_with` 配列のエントリを GUI から追加・削除する API。
//!
//! 主眼: **既存 `conf.toml` のコメント・書式をできる限り温存したまま**、
//! `run_with = [ ... ]` のインライン配列末尾に追加 / 指定 index を削除する。
//! そのために全体の `Conf` 往復（→ serde serialize）は避け、[`toml_edit`] の
//! `DocumentMut` で該当部分だけをピンポイント操作する。
//!
//! ## 設計
//!
//! - `GET /run_with` — 現 **メモリ上** の `Conf::run_with` を GUI 向け DTO に変換して返す。
//!   effective_id / display_label / supports_status などの派生情報を付ける。
//! - `POST /run_with` — 末尾に 1 entry 追加。書き込み前にサーバ側で `toml::from_str::<Conf>()` で
//!   全体妥当性を検証し、失敗時はファイルを一切変更しない。
//! - `DELETE /run_with/{index}` — 指定 index を削除。
//!
//! ## 後処理
//!
//! 書き込み成功後は:
//!  1. `.bak-YYYYMMDD-HHmmSS` バックアップを生成（既存 conf.toml を退避コピー）
//!  2. 新ファイルをパースし直して `ManagedAppRegistry::from_conf` で registry を丸ごと再構築
//!  3. `probe_all` で初期 statuses を埋める
//!  4. `state.managed_apps` を置換
//!
//! `state.conf` の in-memory 更新は行わない（再起動まで他の設定と混ざるとバグりやすい）。
//! `run_with` は起動時に一度だけ使われる設計なので、disk と registry の整合が取れていれば十分。

use actix_web::web::{self, Data, Json, Path};
use actix_web::{delete, get, post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use toml_edit::{DocumentMut, InlineTable, Item, Value};

use crate::conf::{Conf, RunWith};
use crate::managed_app::{self, ManagedAppRegistry};
use crate::SharedState;

// ---- DTO ---------------------------------------------------------------------

/// GUI との JSON やり取りに使う `RunWith` の DTO。内部 `RunWith` とほぼ同義だが、
/// フィールドをフラットに出すため `untagged` 版を独立に用意。
#[derive(Debug, Deserialize, Serialize, Clone)]
#[serde(untagged)]
pub enum RunWithDto {
 /// 単純な文字列形式（`"notepad"` や `"http://..."`）
 Simple(String),
 /// `{ command = "...", if_not_running = "...", ... }` 形式
 Table(RunWithTableDto),
}

#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct RunWithTableDto {
 pub command: String,
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub if_not_running: Option<String>,
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub run_as_admin: Option<bool>,
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub working_dir: Option<String>,
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub minimized: Option<bool>,
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub id: Option<String>,
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub label: Option<String>,
}

impl From<&RunWith> for RunWithDto {
 fn from(r: &RunWith) -> Self {
  match r {
   RunWith::Command(s) => RunWithDto::Simple(s.clone()),
   RunWith::CommandIfProcessIsNotRunning {
    command,
    if_not_running,
    run_as_admin,
    working_dir,
    minimized,
    id,
    label,
    // shutdown は GUI 側で編集 UI が無いので DTO には露出させない。disk ファイル上の
    // 値はバックアップ含めそのまま保持される（別のエンドポイント経由で touch しない限り）。
    shutdown: _,
   } => RunWithDto::Table(RunWithTableDto {
    command: command.clone(),
    if_not_running: if_not_running.clone(),
    run_as_admin: *run_as_admin,
    working_dir: working_dir.clone(),
    minimized: *minimized,
    id: id.clone(),
    label: label.clone(),
   }),
  }
 }
}

/// GUI が扱いやすいよう派生情報を追加した 1 件の表示形。
#[derive(Debug, Serialize, Clone)]
pub struct RunWithView {
 pub index: usize,
 /// 明示 `id` または `run-with-<index>` の自動値。
 pub effective_id: String,
 /// GUI 向け表示ラベル。
 pub display_label: String,
 /// Managed App として状態監視可能か（`if_not_running` が埋まっているか）。
 pub supports_status: bool,
 pub entry: RunWithDto,
}

#[derive(Debug, Serialize)]
pub struct RunWithListResponse {
 pub entries: Vec<RunWithView>,
 /// 書き込み先 conf ファイルのパス。GUI の UX（「このファイルに追記します」）のため。
 pub source_path: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct RunWithMutationResponse {
 pub entries: Vec<RunWithView>,
 pub source_path: Option<String>,
 pub backup: Option<String>,
 pub warning: Option<String>,
}

// ---- エラー表現 --------------------------------------------------------------

fn err_response(status: actix_web::http::StatusCode, code: &str, detail: impl std::fmt::Display) -> HttpResponse {
 HttpResponse::build(status).json(serde_json::json!({
  "error": code,
  "detail": detail.to_string(),
 }))
}

// ---- ビュー構築 --------------------------------------------------------------

fn build_views(run_with: &[RunWith]) -> Vec<RunWithView> {
 run_with
  .iter()
  .enumerate()
  .map(|(idx, r)| RunWithView {
   index: idx,
   effective_id: r
    .explicit_id()
    .map(|s| s.to_string())
    .unwrap_or_else(|| format!("run-with-{idx}")),
   display_label: r.display_label().to_string(),
   supports_status: r.process_marker().is_some(),
   entry: RunWithDto::from(r),
  })
  .collect()
}

// ---- GET ---------------------------------------------------------------------

#[get("/run_with")]
pub async fn get_run_with(state: Data<SharedState>) -> impl Responder {
 let path_opt = {
  let s = state.read().await;
  s.conf_source_path.clone()
 };
 let Some(path) = path_opt else {
  return HttpResponse::Ok().json(RunWithListResponse {
   entries: Vec::new(),
   source_path: None,
  });
 };
 match Conf::new_noop_probe(&path) {
  Ok(c) => HttpResponse::Ok().json(RunWithListResponse {
   entries: build_views(&c.run_with),
   source_path: Some(path.display().to_string()),
  }),
  Err(e) => err_response(
   actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
   "conf_load_failed",
   e,
  ),
 }
}

// ---- 書き込み共通 ------------------------------------------------------------

/// `conf.toml` を load して `toml_edit` でいじり、書き戻し + managed_apps registry を更新する。
/// クロージャ側は `&mut DocumentMut` を受け取って `run_with` 配列を編集する。
async fn mutate_conf<F>(state: &SharedState, mutate: F) -> Result<RunWithMutationResponse, HttpResponse>
where
 F: FnOnce(&mut DocumentMut) -> Result<(), HttpResponse>,
{
 let source_path = {
  let s = state.read().await;
  s.conf_source_path.clone()
 };
 let Some(source_path) = source_path else {
  return Err(err_response(
   actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
   "conf_source_path_unset",
   "conf ファイルの出自が記録されていないので、run_with の書き換えはできません。",
  ));
 };

 let original = std::fs::read_to_string(&source_path).map_err(|e| {
  err_response(
   actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
   "read_failed",
   e,
  )
 })?;

 let mut doc = original.parse::<DocumentMut>().map_err(|e| {
  err_response(
   actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
   "conf_toml_parse_failed",
   e,
  )
 })?;

 mutate(&mut doc)?;

 let updated_text = doc.to_string();

 // 書き込み前に完全な Conf としてパース検証。
 if let Err(e) = toml::from_str::<Conf>(&updated_text) {
  return Err(err_response(
   actix_web::http::StatusCode::BAD_REQUEST,
   "invalid_conf_after_edit",
   format!("編集後の conf が `Conf` としてパースできません: {e}"),
  ));
 }

 // バックアップ。
 let ts = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
 let bak_name = format!(
  "{}.bak-{}",
  source_path
   .file_name()
   .and_then(|s| s.to_str())
   .unwrap_or("conf.toml"),
  ts
 );
 let bak_path = source_path.with_file_name(&bak_name);
 if let Err(e) = std::fs::copy(&source_path, &bak_path) {
  log::warn!("《RunWithOps》 バックアップに失敗: {e}");
 }

 if let Err(e) = std::fs::write(&source_path, &updated_text) {
  return Err(err_response(
   actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
   "write_failed",
   e,
  ));
 }

 // 新 conf をパースし直して registry を再構築。
 let new_conf = match Conf::new_noop_probe(source_path.clone()) {
  Ok(c) => c,
  Err(e) => {
   return Err(err_response(
    actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
    "reload_failed",
    e,
   ));
  },
 };
 let mut new_registry = ManagedAppRegistry::from_conf(&new_conf);
 // 即時 probe で初期ステータスを埋める（monitor タスクの次 tick を待たない）。
 new_registry.statuses = managed_app::probe_all(&new_registry.specs);

 let entries = build_views(&new_conf.run_with);

 // state.managed_apps を置換。
 {
  let s = state.read().await;
  let reg = s.managed_apps.clone();
  drop(s);
  let mut w = reg.write().await;
  *w = new_registry;
 }

 log::info!(
  "《RunWithOps》 {} を更新し、managed_apps registry を再構築しました。",
  source_path.display()
 );

 Ok(RunWithMutationResponse {
  entries,
  source_path: Some(source_path.display().to_string()),
  backup: Some(bak_name),
  warning: Some(
   "run_with の変更を conf.toml に書き込みました。既に起動済みのプロセスは影響を受けません。\
     必要に応じて Managed Apps 画面から起動し直してください。"
    .to_string(),
  ),
 })
}

// ---- 配列アクセスヘルパ ------------------------------------------------------

/// `run_with` 配列（inline array）への可変参照を取得する。存在しなければ空配列を挿入。
fn run_with_array(doc: &mut DocumentMut) -> Result<&mut toml_edit::Array, HttpResponse> {
 // None なら空配列を作る。
 if !doc.contains_key("run_with") {
  doc["run_with"] = Item::Value(Value::Array(toml_edit::Array::new()));
 }

 // ArrayOfTables (`[[run_with]]` 記法) にも一応対応したいところだが、conf.example は inline array を
 // 公式サンプルにしているので、ここでは inline array のみサポート。混在は今回は非対応で 400 に倒す。
 let item = &mut doc["run_with"];
 match item {
  Item::Value(Value::Array(arr)) => Ok(arr),
  Item::ArrayOfTables(_) => Err(err_response(
   actix_web::http::StatusCode::NOT_IMPLEMENTED,
   "array_of_tables_not_supported",
   "conf.toml の run_with は `[[run_with]]` テーブル形式で書かれています。\
    現在 GUI からの編集はインライン配列形式 (`run_with = [ ... ]`) のみ対応しています。",
  )),
  _ => Err(err_response(
   actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
   "invalid_run_with_shape",
   "conf.toml の `run_with` が配列でありません。手動修正してください。",
  )),
 }
}

/// `RunWithDto` を `toml_edit` の `Value` に変換。
fn dto_to_value(dto: &RunWithDto) -> Value {
 match dto {
  RunWithDto::Simple(s) => Value::from(s.as_str()),
  RunWithDto::Table(t) => {
   let mut it = InlineTable::new();
   it.insert("command", Value::from(t.command.as_str()));
   if let Some(v) = &t.if_not_running {
    it.insert("if_not_running", Value::from(v.as_str()));
   }
   if let Some(v) = t.run_as_admin {
    it.insert("run_as_admin", Value::from(v));
   }
   if let Some(v) = &t.working_dir {
    it.insert("working_dir", Value::from(v.as_str()));
   }
   if let Some(v) = t.minimized {
    it.insert("minimized", Value::from(v));
   }
   if let Some(v) = &t.id {
    it.insert("id", Value::from(v.as_str()));
   }
   if let Some(v) = &t.label {
    it.insert("label", Value::from(v.as_str()));
   }
   Value::InlineTable(it)
  },
 }
}

// ---- POST / DELETE -----------------------------------------------------------

#[post("/run_with")]
pub async fn post_add(state: Data<SharedState>, body: Json<RunWithDto>) -> impl Responder {
 // 妥当性の軽い事前チェック: command が空だと後続の Conf パースが実用上通ってしまう恐れがある。
 if let RunWithDto::Table(t) = &*body {
  if t.command.trim().is_empty() {
   return err_response(
    actix_web::http::StatusCode::BAD_REQUEST,
    "empty_command",
    "command が空です。",
   );
  }
 } else if let RunWithDto::Simple(s) = &*body {
  if s.trim().is_empty() {
   return err_response(
    actix_web::http::StatusCode::BAD_REQUEST,
    "empty_command",
    "command が空です。",
   );
  }
 }

 let new_entry = body.into_inner();
 match mutate_conf(&state, |doc| {
  let arr = run_with_array(doc)?;
  arr.push(dto_to_value(&new_entry));
  Ok(())
 })
 .await
 {
  Ok(resp) => HttpResponse::Ok().json(resp),
  Err(r) => r,
 }
}

#[delete("/run_with/{index}")]
pub async fn delete_entry(state: Data<SharedState>, index: Path<usize>) -> impl Responder {
 let target_index = index.into_inner();
 match mutate_conf(&state, move |doc| {
  let arr = run_with_array(doc)?;
  if target_index >= arr.len() {
   return Err(err_response(
    actix_web::http::StatusCode::NOT_FOUND,
    "index_out_of_range",
    format!("index {target_index} は run_with[{}] の範囲外です。", arr.len()),
   ));
  }
  arr.remove(target_index);
  Ok(())
 })
 .await
 {
  Ok(resp) => HttpResponse::Ok().json(resp),
  Err(r) => r,
 }
}

pub fn configure(cfg: &mut web::ServiceConfig) {
 cfg.service(get_run_with).service(post_add).service(delete_entry);
}

#[cfg(test)]
mod tests {
 use super::*;

 fn doc(src: &str) -> DocumentMut {
  src.parse::<DocumentMut>().expect("parse")
 }

 #[test]
 fn push_to_empty_array_creates_it() {
  let mut d = doc("# empty file\n");
  let arr = run_with_array(&mut d).expect("get");
  arr.push(Value::from("notepad"));
  let out = d.to_string();
  assert!(out.contains("run_with"));
  assert!(out.contains("notepad"));
 }

 #[test]
 fn push_to_inline_array_preserves_others() {
  let mut d = doc(
   r#"
# keep this comment
speech_floor = 0
run_with = [
 "notepad",
]
other_key = "x"
"#,
  );
  let arr = run_with_array(&mut d).expect("get");
  arr.push(Value::from("http://example.com"));
  let out = d.to_string();
  assert!(out.contains("# keep this comment"));
  assert!(out.contains("notepad"));
  assert!(out.contains("http://example.com"));
  assert!(out.contains("other_key = \"x\""));
 }

 #[test]
 fn remove_entry_by_index() {
  let mut d = doc(
   r#"run_with = [
 "a",
 "b",
 "c",
]
"#,
  );
  let arr = run_with_array(&mut d).expect("get");
  arr.remove(1);
  let out = d.to_string();
  assert!(out.contains("\"a\""));
  assert!(!out.contains("\"b\""));
  assert!(out.contains("\"c\""));
 }

 #[test]
 fn array_of_tables_is_rejected() {
  let mut d = doc(
   r#"[[run_with]]
command = "notepad"
"#,
  );
  let err = run_with_array(&mut d);
  assert!(err.is_err());
 }

 #[test]
 fn dto_to_inline_table_only_emits_present_fields() {
  let v = dto_to_value(&RunWithDto::Table(RunWithTableDto {
   command: "foo.exe".into(),
   if_not_running: Some("foo".into()),
   run_as_admin: None,
   working_dir: None,
   minimized: Some(true),
   id: Some("foo-app".into()),
   label: None,
  }));
  let rendered = v.to_string();
  assert!(rendered.contains("command = \"foo.exe\""));
  assert!(rendered.contains("if_not_running = \"foo\""));
  assert!(rendered.contains("minimized = true"));
  assert!(rendered.contains("id = \"foo-app\""));
  assert!(!rendered.contains("run_as_admin"));
  assert!(!rendered.contains("working_dir"));
  assert!(!rendered.contains("label"));
 }
}
