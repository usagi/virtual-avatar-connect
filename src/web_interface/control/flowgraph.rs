//! Phase δ-6: Flowgraph 編集 GUI のための Control API。
//!
//! spec §9.5 に相当する read / write エンドポイント群。
//!
//! ## パス表現
//!
//! URL 上は `/flowgraph/file/{fq:.*}` の形を採用し、`fq` は拡張子を除いた root 相対パス（`/` 区切り）。
//! 例: `chat-echo/main` ↔ `<flowgraph_dir>/chat-echo/main.flowgraph.toml`。
//! fq は `parse_port_ref` と同じ命名体系なので、ノード参照 (`chat-echo/main::node_id`) と共通化される。
//!
//! ## 安全設計
//!
//! `fq_to_file_path` で `<flowgraph_dir>/<fq>.flowgraph.toml` を組み立てた後、
//! 親ディレクトリを canonicalize して `<flowgraph_dir>` の配下であることを確認する（path traversal 防御）。
//! `..` や絶対パスは fq パース段階で拒否。

use std::path::{Path, PathBuf};

use actix_web::web::{self, Data, Json};
use actix_web::{delete, get, post, put, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::flowgraph::fragment::paste::{
	paste_fragment, PasteError, PasteReport, PasteRequest,
};
use crate::flowgraph::fragment::zip_codec::{
	export_zip, import_zip, ZipExportError, ZipImportError, ZipImportOptions, ZipImportOutcome,
};
use crate::flowgraph::fragment::{copy_targets, serialize_fragment, CopyError, CopyRequest, Fragment};
use crate::flowgraph::loader::{parse_flowgraph_file, Diagnostic, FlowgraphFile, Severity};
use crate::flowgraph::node::{
	json_to_socket_value, InputMap, PortDirection, PortSpec, TriggerEvent,
};
use crate::flowgraph::{registry, FlowgraphRuntime};
use crate::web_interface::control::events::ControlEvent;
use crate::SharedState;

// ============================================================================
// Shared helpers / DTO
// ============================================================================

fn err_json(status: actix_web::http::StatusCode, code: &str, detail: impl std::fmt::Display) -> HttpResponse {
	HttpResponse::build(status).json(serde_json::json!({
		"error": code,
		"detail": detail.to_string(),
	}))
}

/// `state.flowgraph` と `conf.flowgraph_dir` を取り出す。dir 未設定なら 500。
async fn flowgraph_dir(state: &SharedState) -> Result<(PathBuf, Option<FlowgraphRuntime>), HttpResponse> {
	let fg = state.read().await.flowgraph.clone();
	let rt_guard = fg.read().await;
	let Some(rt) = rt_guard.as_ref() else {
		return Err(err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定のため Flowgraph API は無効です。",
		));
	};
	Ok((rt.root_dir.clone(), Some(rt.clone())))
}

/// fq 文字列を検証し、`<root>/<fq>.flowgraph.toml` の絶対パスを返す。
///
/// - 空 / `..` / 絶対パスは拒否。
/// - 最終的に root 配下であることを確認。root 自体は存在しなくても fq 解決は通す
///   （ファイル新規作成時に root も作るため）。
fn fq_to_file_path(root: &Path, fq: &str) -> Result<PathBuf, HttpResponse> {
	let fq = fq.trim_matches('/').trim();
	if fq.is_empty() {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"empty_fq",
			"fq が空です",
		));
	}
	for seg in fq.split('/') {
		if seg.is_empty() || seg == "." || seg == ".." {
			return Err(err_json(
				actix_web::http::StatusCode::BAD_REQUEST,
				"invalid_fq",
				format!("fq に不正なセグメントが含まれます: '{seg}' in '{fq}'"),
			));
		}
	}
	if fq.contains('\\') {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_fq",
			"fq にバックスラッシュは使えません",
		));
	}
	// Windows のドライブ文字プレフィックスなども Path::new で絶対判定される。
	let rel = Path::new(fq);
	if rel.is_absolute() {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_fq",
			"fq は相対パスでなければなりません",
		));
	}
	let filename = format!("{}.flowgraph.toml", rel.file_name().and_then(|s| s.to_str()).unwrap_or(""));
	let file_path = match rel.parent() {
		Some(p) if !p.as_os_str().is_empty() => root.join(p).join(filename),
		_ => root.join(filename),
	};
	Ok(file_path)
}

/// 「確定したパスが root 配下であること」を canonicalize 後に検査。
///
/// 存在しないファイル（新規作成予定）でも動くよう、親ディレクトリを基準に判定する。
fn ensure_within_root(root: &Path, file_path: &Path) -> Result<(), HttpResponse> {
	// 親の canonicalize 値が root の canonicalize 値の接頭辞であること。
	let root_canon = match std::fs::canonicalize(root) {
		Ok(p) => p,
		Err(_) => {
			// root 自体が未作成（opt-in 未使用 dir）のケース。ここでは `ancestors()` の lexical 比較で妥協。
			return lexical_within(root, file_path);
		}
	};
	let parent = file_path.parent().unwrap_or(file_path);
	let parent_canon = match std::fs::canonicalize(parent) {
		Ok(p) => p,
		Err(_) => {
			// 親がまだ無い（深いサブディレクトリ新規）ケース。root と合流するまで遡る。
			let mut cur: &Path = parent;
			loop {
				match std::fs::canonicalize(cur) {
					Ok(abs) => {
						if !abs.starts_with(&root_canon) {
							return Err(err_json(
								actix_web::http::StatusCode::BAD_REQUEST,
								"path_traversal",
								format!("fq が flowgraph_dir の外を指します: {}", file_path.display()),
							));
						}
						return Ok(());
					}
					Err(_) => {
						cur = match cur.parent() {
							Some(p) => p,
							None => {
								return Err(err_json(
									actix_web::http::StatusCode::BAD_REQUEST,
									"path_unresolvable",
									format!("パスを正規化できません: {}", file_path.display()),
								))
							}
						};
					}
				}
			}
		}
	};
	if !parent_canon.starts_with(&root_canon) {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"path_traversal",
			format!("fq が flowgraph_dir の外を指します: {}", file_path.display()),
		));
	}
	Ok(())
}

/// `canonicalize` 使えないケース向けの lexical 比較。`file_path` が `root` 直下の構造かをざっくり見る。
fn lexical_within(root: &Path, file_path: &Path) -> Result<(), HttpResponse> {
	if file_path.starts_with(root) {
		Ok(())
	} else {
		Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"path_traversal",
			format!("fq が flowgraph_dir の外を指します: {}", file_path.display()),
		))
	}
}

// ============================================================================
// GET /flowgraph/node-catalog
// ============================================================================

#[derive(Debug, Serialize)]
pub struct NodeCatalogResponse {
	/// 登録されている全 feature の `NodeSpec`。feature 名でソート済み。
	pub specs: Vec<serde_json::Value>,
	/// 総件数（GUI のページング / 表示ヘルパ用）。
	pub count: usize,
}

#[get("/flowgraph/node-catalog")]
pub async fn get_node_catalog() -> impl Responder {
	// Phase φ-6: Flowgraph Editor の「Trigger」ボタン表示判定のため、
	// `control_triggerable` を NodeSpec JSON に差し込んで返す。NodeSpec 自体に
	// field を増やすと既存リテラルが全て壊れるので JSON 層で注入する。
	let reg = registry();
	let specs = reg.all_specs();
	let values: Vec<serde_json::Value> = specs
		.iter()
		.map(|s| {
			let mut v = serde_json::to_value(s).unwrap();
			if let Some(obj) = v.as_object_mut() {
				obj.insert(
					"control_triggerable".to_string(),
					serde_json::Value::Bool(reg.is_control_triggerable(&s.feature)),
				);
			}
			v
		})
		.collect();
	HttpResponse::Ok().json(NodeCatalogResponse {
		count: values.len(),
		specs: values,
	})
}

// ============================================================================
// GET /flowgraph/tree
// ============================================================================

#[derive(Debug, Serialize)]
pub struct TreeFileEntry {
	/// `<root>/<fq>.flowgraph.toml` の fq 部分。拡張子なし、`/` 区切り。
	pub fq: String,
	/// root 相対の実パス（Windows でも `/` 区切りに正規化）。
	pub path: String,
	/// ファイルの `[meta] title`（解析できれば）。
	pub title: Option<String>,
	/// ノード件数（解析できれば）。
	pub node_count: Option<usize>,
	/// エッジ件数。
	pub edge_count: Option<usize>,
	/// パース失敗時の短いエラー文字列。成功時 None。
	pub parse_error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TreeResponse {
	pub root_dir: String,
	pub exists: bool,
	pub files: Vec<TreeFileEntry>,
}

#[get("/flowgraph/tree")]
pub async fn get_tree(state: Data<SharedState>) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let exists = root.exists() && root.is_dir();
	let root_disp = root.display().to_string().replace('\\', "/");

	if !exists {
		return HttpResponse::Ok().json(TreeResponse {
			root_dir: root_disp,
			exists: false,
			files: vec![],
		});
	}

	let files = match crate::flowgraph::loader::walk_flowgraph_dir(&root) {
		Ok(v) => v,
		Err(e) => {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"walk_failed",
				format!("ディレクトリ走査に失敗: {e}"),
			)
		}
	};

	let mut entries = Vec::with_capacity(files.len());
	for file_path in files {
		let fq = crate::flowgraph::loader::fq_path_of_file(&root, &file_path).unwrap_or_default();
		let rel = file_path
			.strip_prefix(&root)
			.map(|p| p.to_string_lossy().replace('\\', "/"))
			.unwrap_or_default();
		match std::fs::read_to_string(&file_path) {
			Ok(src) => match parse_flowgraph_file(&src, Some(&file_path)) {
				Ok(file) => entries.push(TreeFileEntry {
					fq,
					path: rel,
					title: file.meta.as_ref().and_then(|m| m.title.clone()),
					node_count: Some(file.nodes.len()),
					edge_count: Some(file.edges.len()),
					parse_error: None,
				}),
				Err(e) => entries.push(TreeFileEntry {
					fq,
					path: rel,
					title: None,
					node_count: None,
					edge_count: None,
					parse_error: Some(format!("{}", e)),
				}),
			},
			Err(e) => entries.push(TreeFileEntry {
				fq,
				path: rel,
				title: None,
				node_count: None,
				edge_count: None,
				parse_error: Some(format!("read failed: {e}")),
			}),
		}
	}

	HttpResponse::Ok().json(TreeResponse {
		root_dir: root_disp,
		exists: true,
		files: entries,
	})
}

// ============================================================================
// GET /flowgraph/file/{fq:.*}
// ============================================================================

#[derive(Debug, Serialize)]
pub struct FileResponse {
	pub fq: String,
	/// root 相対のファイルパス（`/` 区切り）。
	pub path: String,
	/// ファイルそのものの TOML ソース。GUI がテキストエディタビューに流す用。
	pub raw_toml: String,
	/// パース済み構造化データ（meta/nodes/edges）。parse_error が None のときだけ有効。
	pub parsed: Option<FlowgraphFile>,
	/// パース失敗時のメッセージ（Diagnostic リスト）。
	pub parse_diagnostics: Vec<Diagnostic>,
}

#[get("/flowgraph/file/{fq:.*}")]
pub async fn get_file(state: Data<SharedState>, fq_path: web::Path<String>) -> impl Responder {
	let fq = fq_path.into_inner();
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let file_path = match fq_to_file_path(&root, &fq) {
		Ok(v) => v,
		Err(r) => return r,
	};
	if let Err(r) = ensure_within_root(&root, &file_path) {
		return r;
	}

	if !file_path.is_file() {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("ファイルが存在しません: {}", file_path.display()),
		);
	}
	let raw = match std::fs::read_to_string(&file_path) {
		Ok(v) => v,
		Err(e) => {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"read_failed",
				format!("読み込み失敗: {e}"),
			)
		}
	};
	let rel = file_path
		.strip_prefix(&root)
		.map(|p| p.to_string_lossy().replace('\\', "/"))
		.unwrap_or_default();

	let (parsed, parse_diagnostics) = match parse_flowgraph_file(&raw, Some(&file_path)) {
		Ok(f) => (Some(f), vec![]),
		Err(e) => (None, e.diagnostics),
	};

	HttpResponse::Ok().json(FileResponse {
		fq: fq.trim_matches('/').to_string(),
		path: rel,
		raw_toml: raw,
		parsed,
		parse_diagnostics,
	})
}

// ============================================================================
// GET /flowgraph/diagnostics
// ============================================================================

#[derive(Debug, Serialize)]
pub struct DiagnosticsResponse {
	pub root_dir: String,
	pub ok: bool,
	pub diagnostics: Vec<Diagnostic>,
	pub node_meta: std::collections::HashMap<String, crate::flowgraph::loader::LoadedNodeMeta>,
}

#[get("/flowgraph/diagnostics")]
pub async fn get_diagnostics(state: Data<SharedState>) -> impl Responder {
	let fg = state.read().await.flowgraph.clone();
	let rt = fg.read().await;
	let Some(rt) = rt.as_ref() else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	HttpResponse::Ok().json(DiagnosticsResponse {
		root_dir: rt.root_dir.display().to_string().replace('\\', "/"),
		ok: rt.ok,
		diagnostics: rt.diagnostics.clone(),
		node_meta: rt.node_meta.clone(),
	})
}

// ============================================================================
// POST /flowgraph/file  （新規作成）
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct CreateFileRequest {
	/// 新規作成ファイルの fq。例: `chat-echo/main`。
	pub fq: String,
	/// 省略時は空ヘッダテンプレート（spec §14.1 δ-0 合意）。
	#[serde(default)]
	pub initial_toml: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct WriteFileResponse {
	pub fq: String,
	pub path: String,
	/// Write 操作後に再ロードした結果のダイジェスト。
	pub diagnostics: Vec<Diagnostic>,
	/// ロードが成功したか（error 診断がなかったか）。
	pub ok: bool,
	/// 書き込み直前の `.bak` ファイル名（新規作成時は None）。
	pub backup: Option<String>,
}

fn default_initial_toml(fq: &str) -> String {
	format!(
		"# {fq}\n\n[meta]\ntitle = \"{fq}\"\n\n# --- nodes ---\n# [[nodes]]\n# id = \"example\"\n# feature = \"flowgraph.literal.string\"\n# properties.value = \"hello\"\n\n# --- edges ---\n# [[edges]]\n# from = \"example:value\"\n# to = \"logger:value\"\n"
	)
}

/// 書き込み前の妥当性検証。`raw_toml` が Flowgraph schema としてパースできることを確認する。
fn validate_toml_for_write(raw: &str) -> Result<(), HttpResponse> {
	match parse_flowgraph_file(raw, None) {
		Ok(_) => Ok(()),
		Err(e) => Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_toml",
			format!("書き込み内容が Flowgraph TOML として不正: {:#?}", e.diagnostics),
		)),
	}
}

#[post("/flowgraph/file")]
pub async fn post_file(state: Data<SharedState>, body: Json<CreateFileRequest>) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let file_path = match fq_to_file_path(&root, &body.fq) {
		Ok(v) => v,
		Err(r) => return r,
	};
	if let Err(r) = ensure_within_root(&root, &file_path) {
		return r;
	}
	if file_path.exists() {
		return err_json(
			actix_web::http::StatusCode::CONFLICT,
			"already_exists",
			format!("既に存在します: {}", file_path.display()),
		);
	}
	let content = body.initial_toml.clone().unwrap_or_else(|| default_initial_toml(&body.fq));
	if let Err(r) = validate_toml_for_write(&content) {
		return r;
	}

	if let Some(parent) = file_path.parent() {
		if let Err(e) = std::fs::create_dir_all(parent) {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"create_dir_failed",
				format!("親ディレクトリ作成に失敗: {e}"),
			);
		}
	}
	if let Err(e) = std::fs::write(&file_path, &content) {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"write_failed",
			format!("書き込み失敗: {e}"),
		);
	}

	let (ok, diagnostics) = reload_runtime(&state, &root).await;
	log::info!("《Flowgraph》 作成: {}", file_path.display());
	HttpResponse::Created().json(WriteFileResponse {
		fq: body.fq.trim_matches('/').to_string(),
		path: root_relative(&root, &file_path),
		diagnostics,
		ok,
		backup: None,
	})
}

// ============================================================================
// PUT /flowgraph/file/{fq:.*}  （丸ごと更新）
// ============================================================================

#[derive(Debug, Deserialize)]
pub struct PutFileRequest {
	/// ファイル全体を置き換える TOML テキスト。GUI 側が `toml_edit` 等でフォーマットを保ったまま
	/// 編集した結果を送ることを想定。backend では schema 検証のみ行う。
	pub content: String,
}

#[put("/flowgraph/file/{fq:.*}")]
pub async fn put_file(
	state: Data<SharedState>,
	fq_path: web::Path<String>,
	body: Json<PutFileRequest>,
) -> impl Responder {
	let fq = fq_path.into_inner();
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let file_path = match fq_to_file_path(&root, &fq) {
		Ok(v) => v,
		Err(r) => return r,
	};
	if let Err(r) = ensure_within_root(&root, &file_path) {
		return r;
	}
	if !file_path.is_file() {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("ファイルが存在しません: {}", file_path.display()),
		);
	}
	if let Err(r) = validate_toml_for_write(&body.content) {
		return r;
	}

	let backup = match make_backup(&file_path) {
		Ok(name) => Some(name),
		Err(e) => {
			log::warn!("《Flowgraph》 backup 作成に失敗: {e}");
			None
		}
	};
	if let Err(e) = std::fs::write(&file_path, &body.content) {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"write_failed",
			format!("書き込み失敗: {e}"),
		);
	}
	let (ok, diagnostics) = reload_runtime(&state, &root).await;
	log::info!("《Flowgraph》 更新: {} (backup={:?})", file_path.display(), backup);
	HttpResponse::Ok().json(WriteFileResponse {
		fq: fq.trim_matches('/').to_string(),
		path: root_relative(&root, &file_path),
		diagnostics,
		ok,
		backup,
	})
}

// ============================================================================
// DELETE /flowgraph/file/{fq:.*}
// ============================================================================

#[delete("/flowgraph/file/{fq:.*}")]
pub async fn delete_file(state: Data<SharedState>, fq_path: web::Path<String>) -> impl Responder {
	let fq = fq_path.into_inner();
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let file_path = match fq_to_file_path(&root, &fq) {
		Ok(v) => v,
		Err(r) => return r,
	};
	if let Err(r) = ensure_within_root(&root, &file_path) {
		return r;
	}
	if !file_path.is_file() {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("ファイルが存在しません: {}", file_path.display()),
		);
	}

	// 削除はまず .bak 保存してから本体削除 → 誤操作復元が容易。
	let backup = match make_backup(&file_path) {
		Ok(n) => n,
		Err(e) => {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"backup_failed",
				format!("バックアップ作成に失敗: {e}"),
			)
		}
	};
	if let Err(e) = std::fs::remove_file(&file_path) {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"delete_failed",
			format!("削除失敗: {e}"),
		);
	}

	let (ok, diagnostics) = reload_runtime(&state, &root).await;
	log::info!("《Flowgraph》 削除: {} (backup={})", file_path.display(), backup);
	HttpResponse::Ok().json(WriteFileResponse {
		fq: fq.trim_matches('/').to_string(),
		path: root_relative(&root, &file_path),
		diagnostics,
		ok,
		backup: Some(backup),
	})
}

// ============================================================================
// POST /flowgraph/file/{fq:.*}/open-external
// ============================================================================

#[derive(Debug, Serialize)]
pub struct OpenExternalResponse {
	pub fq: String,
	pub path: String,
	pub command: String,
	pub spawned: bool,
}

#[post("/flowgraph/file/{fq:.*}/open-external")]
pub async fn post_open_external(state: Data<SharedState>, fq_path: web::Path<String>) -> impl Responder {
	let fq = fq_path.into_inner();
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let file_path = match fq_to_file_path(&root, &fq) {
		Ok(v) => v,
		Err(r) => return r,
	};
	if let Err(r) = ensure_within_root(&root, &file_path) {
		return r;
	}
	if !file_path.is_file() {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("ファイルが存在しません: {}", file_path.display()),
		);
	}
	let (command, spawned) = spawn_os_opener(&file_path);
	log::info!("《Flowgraph》 open-external: {} (cmd={command}, spawned={spawned})", file_path.display());
	HttpResponse::Ok().json(OpenExternalResponse {
		fq: fq.trim_matches('/').to_string(),
		path: root_relative(&root, &file_path),
		command,
		spawned,
	})
}

// ============================================================================
// POST /flowgraph/reload
// ============================================================================

#[derive(Debug, Serialize)]
pub struct ReloadResponse {
	pub root_dir: String,
	pub ok: bool,
	pub diagnostics: Vec<Diagnostic>,
	pub node_count: usize,
}

/// ディスクから Flowgraph を再ロードし、結果を保存 + `FlowgraphReloaded` イベント配信。
///
/// 外部エディタで `.flowgraph.toml` を直接編集した後に GUI から叩く用途を想定。
/// `notify` crate 経由の自動ウォッチャーは δ-7 以降で検討（spec §10.3）。
#[post("/flowgraph/reload")]
pub async fn post_reload(state: Data<SharedState>) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let (ok, diagnostics) = reload_runtime(&state, &root).await;
	let node_count = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await;
		rt.as_ref().map(|r| r.node_meta.len()).unwrap_or(0)
	};
	HttpResponse::Ok().json(ReloadResponse {
		root_dir: root.display().to_string().replace('\\', "/"),
		ok,
		diagnostics,
		node_count,
	})
}

// ============================================================================
// Write helpers
// ============================================================================

fn root_relative(root: &Path, file: &Path) -> String {
	file.strip_prefix(root)
		.map(|p| p.to_string_lossy().replace('\\', "/"))
		.unwrap_or_else(|_| file.display().to_string())
}

fn make_backup(file: &Path) -> std::io::Result<String> {
	let ts = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
	let name = file.file_name().and_then(|s| s.to_str()).unwrap_or("unknown.flowgraph.toml");
	let bak_name = format!("{name}.bak-{ts}");
	let bak_path = file.with_file_name(&bak_name);
	std::fs::copy(file, &bak_path)?;
	Ok(bak_name)
}

/// 書き込み・削除後に `FlowgraphRuntime` を再構築して保存し、`ControlEvent::FlowgraphReloaded` を
/// ブロードキャストする。戻り値は (ok, diagnostics) のサマリ。
///
/// ζ-3: 旧ランタイムの worker と bridges をまず停止し、新 runtime 上で bridges を再 spawn する。
/// `flowgraph.ingress.web_input` は actix-web の route が起動時固定なので hot-swap 不可。
/// 差分があれば `ControlEvent::RestartRecommended` で GUI にトースト。
async fn reload_runtime(state: &SharedState, root: &Path) -> (bool, Vec<Diagnostic>) {
	// ζ-3: 新 runtime は worker 付きで spawn する。state_weak / audio_sink は初回と同じ経路。
	let (state_weak, audio_sink, fg, tx, channel_datum_tx, bridge_handles_arc) = {
		let s = state.read().await;
		(
			std::sync::Arc::downgrade(state),
			Some(s.audio_sink.clone()),
			s.flowgraph.clone(),
			s.control_event_tx.clone(),
			s.channel_datum_tx.clone(),
			s.bridge_handles.clone(),
		)
	};

	// 1. 旧 bridges を落とす。新 runtime の worker を立ち上げる前に切ることで、
	//    旧 trigger に死後 send する事故を防ぐ。
	let old_web_input_snapshot = {
		let mut slot = bridge_handles_arc.lock().await;
		let taken = std::mem::replace(&mut *slot, crate::bridges::BridgeHandles::empty());
		let snap = taken.web_input_snapshot.clone();
		taken.finish_all().await;
		snap
	};

	// 2. 旧 runtime の worker を shutdown（RuntimeHandle 経由）。持っていなければ no-op。
	{
		let old_rt = fg.read().await.clone();
		if let Some(rt) = old_rt {
			if let Some(handle) = rt.handle.as_ref() {
				handle.shutdown().await;
			}
		}
	}

	// 3. 新 runtime を worker 付きで立ち上げる。
	let rt = FlowgraphRuntime::load_and_spawn(root, state_weak, audio_sink);
	let (ok, diags) = (rt.ok, rt.diagnostics.clone());
	let error_count = diags.iter().filter(|d| d.severity == Severity::Error).count();
	let warning_count = diags.iter().filter(|d| d.severity == Severity::Warning).count();
	let node_count = rt.node_meta.len();
	let root_str = rt.root_dir.display().to_string().replace('\\', "/");
	*fg.write().await = Some(rt);

	// 4. 新 runtime 上で bridges を再 spawn し、State に差し戻す。
	let new_handles = crate::bridges::spawn_all_from_state(state, &channel_datum_tx).await;
	let new_web_input_snapshot = new_handles.web_input_snapshot.clone();
	{
		let mut slot = bridge_handles_arc.lock().await;
		*slot = new_handles;
	}

	// 5. web_input の差分を検出したら、actix route は hot-swap できない旨を通知する。
	if crate::bridges::web_input_changed(&old_web_input_snapshot, &new_web_input_snapshot) {
		log::warn!(
			"《Flowgraph/Bridges》 web_input endpoints が変化しました（旧={}, 新={}）。actix route は起動時固定のため、反映には本プロセスの再起動が必要です。",
			old_web_input_snapshot.len(),
			new_web_input_snapshot.len()
		);
		let _ = tx.send(ControlEvent::RestartRecommended {
			reason: "web_input endpoints changed after flowgraph reload".into(),
			details: serde_json::json!({
				"old_web_input_count": old_web_input_snapshot.len(),
				"new_web_input_count": new_web_input_snapshot.len(),
			}),
		});
	}

	// 6. FlowgraphReloaded は最後に送る（受信者 0 でも成功扱い）。
	let _ = tx.send(ControlEvent::FlowgraphReloaded {
		root_dir: root_str,
		ok,
		error_count,
		warning_count,
		node_count,
	});

	(ok, diags)
}

/// プラットフォーム既定のエディタ/ビューアで file を開く（fire-and-forget）。
fn spawn_os_opener(file: &Path) -> (String, bool) {
	let path = file.display().to_string();
	#[cfg(target_os = "windows")]
	{
		let cmd = format!("cmd /C start \"\" {path:?}");
		let spawned = std::process::Command::new("cmd")
			.args(["/C", "start", "", &path])
			.spawn()
			.is_ok();
		(cmd, spawned)
	}
	#[cfg(target_os = "macos")]
	{
		let cmd = format!("open {path:?}");
		let spawned = std::process::Command::new("open").arg(&path).spawn().is_ok();
		(cmd, spawned)
	}
	#[cfg(all(unix, not(target_os = "macos")))]
	{
		let cmd = format!("xdg-open {path:?}");
		let spawned = std::process::Command::new("xdg-open").arg(&path).spawn().is_ok();
		(cmd, spawned)
	}
}

// ============================================================================
// POST /flowgraph/{instance_id}/trigger/{node_id}  （Phase φ-2）
// ============================================================================
//
// spec §4.1 参照。外部から Quick-Add や Dictionary Editor 等が `dictionary.learn` /
// `dictionary.forget` ノードを 1-shot 発火するための安全弁付きトリガ。
//
// ## 安全レイヤ
//
// 1. node_id（fq）が `LoadedNodeMeta` に存在するか → 404 `node_not_found`
// 2. feature が `NodeDescriptor::control_triggerable()` で opt-in 済か → 403 `feature_not_triggerable`
// 3. `exec` 入力ポートが 1 本以上あるか → 400 `no_exec_input`
// 4. `inputs` に指定したポートが実在するか / exec でないか → 400 `unknown_port` / `invalid_port`
// 5. 値が `json_to_socket_value` で socket 型に coerce できるか → 422 `input_type_mismatch`
// 6. RuntimeHandle が生きているか → 503 `worker_not_running`
//
// ## instance_id
//
// V2 は単一 engine のみ（spec §8.1）。`default` のみ受け付け、それ以外は 404 を返す。
// 将来のマルチプロファイル同時実行で profile 名 → engine 解決テーブルを張る余地を残すため
// path 上に残している。

/// リクエスト: `{"inputs": {"source": ..., ...}, "exec_port": "exec_in"?}`
///
/// `inputs` の value は以下いずれかを受け付ける:
///   - 生 JSON: `"ドクターウサギ"` / `42` / `true` / `["a","b"]`
///   - 型注釈版: `{"type":"string","value":"ドクターウサギ"}` — `type` は診断用で実際には
///     socket 側の `PortSpec.ty` が正とする。`value` だけを抽出して生 JSON 版と同じ経路に流す。
#[derive(Debug, Deserialize)]
pub struct TriggerNodeRequest {
	#[serde(default)]
	pub inputs: serde_json::Map<String, serde_json::Value>,
	/// 複数の exec 入力を持つノードで「どのポートを発火したか」を明示する場合に指定。
	/// 省略時は最初の exec 入力ポートを選ぶ（通常 `exec_in`）。
	#[serde(default)]
	pub exec_port: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct TriggerNodeResponse {
	/// 処理受理（engine への送信まで）を表す。実行完了ではない点に注意。
	pub accepted: bool,
	pub instance_id: String,
	pub node_id: String,
	pub feature: String,
	pub exec_port: String,
	/// 実際に coerce して送った data override のキー名一覧。debug 用。
	pub overridden_inputs: Vec<String>,
}

/// `{"type":"T","value":V}` 形式の薄皮を剥がす。見つからなければそのまま返す。
fn unwrap_typed_value(v: &serde_json::Value) -> &serde_json::Value {
	if let serde_json::Value::Object(obj) = v {
		if obj.len() == 2 && obj.contains_key("type") && obj.contains_key("value") {
			if let Some(inner) = obj.get("value") {
				return inner;
			}
		}
	}
	v
}

/// 検証フェーズのエラー。HTTP status / error code / detail のタプルに落として返す。
///
/// 純粋関数部だけを切り出して `#[cfg(test)]` からも直接叩けるようにしている。
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TriggerValidationError {
	NoExecInput,
	ExecPortNotFound(String),
	ExecPortNotExec(String),
	UnknownPort(String),
	ExecPortInData(String),
	InputTypeMismatch { port: String, ty: String },
}

impl TriggerValidationError {
	pub(crate) fn status(&self) -> actix_web::http::StatusCode {
		use actix_web::http::StatusCode;
		match self {
			Self::NoExecInput | Self::ExecPortNotExec(_) | Self::UnknownPort(_) | Self::ExecPortInData(_) => {
				StatusCode::BAD_REQUEST
			}
			Self::ExecPortNotFound(_) => StatusCode::NOT_FOUND,
			Self::InputTypeMismatch { .. } => StatusCode::UNPROCESSABLE_ENTITY,
		}
	}

	pub(crate) fn code(&self) -> &'static str {
		match self {
			Self::NoExecInput => "no_exec_input",
			Self::ExecPortNotFound(_) => "exec_port_not_found",
			Self::ExecPortNotExec(_) => "invalid_port",
			Self::UnknownPort(_) => "unknown_port",
			Self::ExecPortInData(_) => "invalid_port",
			Self::InputTypeMismatch { .. } => "input_type_mismatch",
		}
	}

	fn detail(&self) -> String {
		match self {
			Self::NoExecInput => "ノードに exec 入力ポートがありません".into(),
			Self::ExecPortNotFound(n) => format!("ノードに exec_port '{n}' がありません"),
			Self::ExecPortNotExec(n) => {
				format!("指定された exec_port '{n}' は exec 入力ではありません")
			}
			Self::UnknownPort(n) => format!("ノードに入力ポート '{n}' が存在しません"),
			Self::ExecPortInData(n) => {
				format!("exec 入力 '{n}' に data 値は送れません（exec_port で指定）")
			}
			Self::InputTypeMismatch { port, ty } => {
				format!("入力 '{port}' の値を socket 型 {ty} に変換できません")
			}
		}
	}

	fn to_response(&self) -> HttpResponse {
		err_json(self.status(), self.code(), self.detail())
	}
}

fn select_exec_port<'a>(
	spec: &'a crate::flowgraph::node::NodeSpec,
	override_name: Option<&str>,
) -> Result<&'a PortSpec, TriggerValidationError> {
	if let Some(name) = override_name {
		match spec.inputs.iter().find(|p| p.name == name) {
			Some(p) if p.direction == PortDirection::Input && p.is_exec => Ok(p),
			Some(_) => Err(TriggerValidationError::ExecPortNotExec(name.to_string())),
			None => Err(TriggerValidationError::ExecPortNotFound(name.to_string())),
		}
	} else {
		spec.inputs
			.iter()
			.find(|p| p.direction == PortDirection::Input && p.is_exec)
			.ok_or(TriggerValidationError::NoExecInput)
	}
}

/// body.inputs を NodeSpec.inputs に沿って coerce する。exec ポートへの data 送信は拒否。
///
/// 純粋関数。HTTP / SharedState に依存しない。
pub(crate) fn coerce_input_overrides(
	spec: &crate::flowgraph::node::NodeSpec,
	inputs: &serde_json::Map<String, serde_json::Value>,
) -> Result<(InputMap, Vec<String>), TriggerValidationError> {
	let mut overrides = InputMap::new();
	let mut names: Vec<String> = Vec::with_capacity(inputs.len());
	for (name, raw_value) in inputs.iter() {
		let Some(port) = spec.inputs.iter().find(|p| &p.name == name) else {
			return Err(TriggerValidationError::UnknownPort(name.clone()));
		};
		if port.is_exec {
			return Err(TriggerValidationError::ExecPortInData(name.clone()));
		}
		let unwrapped = unwrap_typed_value(raw_value);
		let Some(sv) = json_to_socket_value(&port.ty, unwrapped) else {
			return Err(TriggerValidationError::InputTypeMismatch {
				port: name.clone(),
				ty: port.ty.to_string(),
			});
		};
		overrides.insert(name.clone(), sv);
		names.push(name.clone());
	}
	Ok((overrides, names))
}

#[post("/flowgraph/{instance_id}/trigger/{node_id}")]
pub async fn post_trigger_node(
	state: Data<SharedState>,
	path: web::Path<(String, String)>,
	body: Json<TriggerNodeRequest>,
) -> impl Responder {
	let (instance_id, node_id) = path.into_inner();

	if instance_id != "default" {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"instance_not_found",
			format!(
				"instance_id '{instance_id}' は存在しません（V2 は 'default' のみ対応）"
			),
		);
	}

	let fg = state.read().await.flowgraph.clone();
	let rt_guard = fg.read().await;
	let Some(rt) = rt_guard.as_ref() else {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"instance_not_found",
			"flowgraph_dir が未設定または未ロードです",
		);
	};

	let Some(meta) = rt.node_meta.get(&node_id) else {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"node_not_found",
			format!("node_id '{node_id}' は存在しません"),
		);
	};
	let feature = meta.feature.clone();

	let reg = registry();
	let Some(spec) = reg.spec(&feature) else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"feature_not_registered",
			format!("feature '{feature}' が registry に見つかりません（loader と不整合）"),
		);
	};

	if !reg.is_control_triggerable(&feature) {
		return err_json(
			actix_web::http::StatusCode::FORBIDDEN,
			"feature_not_triggerable",
			format!(
				"feature '{feature}' は control API からの外部トリガに opt-in していません"
			),
		);
	}

	let exec_port = match select_exec_port(&spec, body.exec_port.as_deref()) {
		Ok(p) => p.name.clone(),
		Err(e) => return e.to_response(),
	};

	let (overrides, overridden_names) = match coerce_input_overrides(&spec, &body.inputs) {
		Ok(v) => v,
		Err(e) => return e.to_response(),
	};

	let Some(handle) = rt.trigger() else {
		return err_json(
			actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
			"worker_not_running",
			"Flowgraph worker が停止中です（ロードエラー等）",
		);
	};

	let mut event = TriggerEvent::new(node_id.clone()).with_exec(exec_port.clone());
	event.data_overrides = overrides;

	if let Err(e) = handle.send(event) {
		return err_json(
			actix_web::http::StatusCode::SERVICE_UNAVAILABLE,
			"trigger_send_failed",
			format!("trigger 送信失敗: {e}"),
		);
	}

	log::info!(
		"《Flowgraph/Trigger》 {} :: {} (feature={}, exec={}, overrides={})",
		instance_id,
		node_id,
		feature,
		exec_port,
		overridden_names.len()
	);

	HttpResponse::Accepted().json(TriggerNodeResponse {
		accepted: true,
		instance_id,
		node_id,
		feature,
		exec_port,
		overridden_inputs: overridden_names,
	})
}

// ============================================================================
// Route registration
// ============================================================================

/// Control API から Flowgraph routes を登録する。
///
/// `/flowgraph/import/zip` が raw ZIP body（`web::Bytes`）を受けるため、actix-web 既定の
/// `PayloadConfig`（256 KiB）では不足。spec §14.4 に従い **64 MiB** に拡張する。
/// `app_data` は scope 内 route 全体に効くが、他の control route は通常 JSON 数 KiB で
/// 上限を跨がないため影響しない。
pub fn configure(cfg: &mut web::ServiceConfig) {
	const ZIP_BODY_LIMIT_BYTES: usize = 64 * 1024 * 1024;
	cfg.app_data(web::PayloadConfig::new(ZIP_BODY_LIMIT_BYTES))
		.service(get_node_catalog)
		.service(get_tree)
		.service(get_diagnostics)
		.service(post_reload)
		.service(post_fragment_copy)
		.service(post_fragment_paste)
		.service(post_export_zip)
		.service(post_import_zip)
		.service(post_trigger_node)
		// open-external を file GET より先に登録しないと `{fq:.*}` が open-external も食うため。
		.service(post_open_external)
		.service(post_file)
		.service(put_file)
		.service(delete_file)
		.service(get_file);
}

// ============================================================================
// Fragment copy / paste（Phase δ-7c）
// ============================================================================

/// copy レスポンス。spec §9.4 に従い `fragment_toml` に生 TOML を詰める。
///
/// `fragment` フィールドは GUI が TOML を再パースしなくても済むように **構造化コピー**
/// を重ねて返す（TOML 側が真の持ち物、structured 側は優先度低い参考情報）。
#[derive(Debug, Serialize)]
pub struct CopyResponse {
	pub fragment_toml: String,
	pub fragment: Fragment,
	pub file_count: usize,
	pub dangling_count: usize,
}

/// paste レスポンス。δ-7b の [`PasteReport`] に reload サマリを添えたもの。
#[derive(Debug, Serialize)]
pub struct PasteResponse {
	pub report: PasteReport,
	pub reload_ok: bool,
	pub diagnostics_count: usize,
	pub node_count: usize,
}

/// 指定した範囲を fragment TOML に切り出す。spec §9.4 `POST /fragment/copy`。
#[post("/flowgraph/fragment/copy")]
pub async fn post_fragment_copy(state: Data<SharedState>, req: Json<CopyRequest>) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let fragment = match copy_targets(&root, &req) {
		Ok(f) => f,
		Err(e) => return copy_error_to_response(e),
	};
	let fragment_toml = match serialize_fragment(&fragment) {
		Ok(s) => s,
		Err(e) => {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"serialize_failed",
				e,
			);
		}
	};
	let file_count = fragment.files.len();
	let dangling_count = fragment.danglings.len();
	HttpResponse::Ok().json(CopyResponse {
		fragment_toml,
		fragment,
		file_count,
		dangling_count,
	})
}

fn copy_error_to_response(e: CopyError) -> HttpResponse {
	use actix_web::http::StatusCode;
	match e {
		CopyError::RootMissing(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "flowgraph_dir_missing", e),
		CopyError::NoTargets => err_json(StatusCode::BAD_REQUEST, "no_targets", e),
		CopyError::FileNotFound(_) => err_json(StatusCode::NOT_FOUND, "file_not_found", e),
		CopyError::NodeNotFound { .. } => err_json(StatusCode::NOT_FOUND, "node_not_found", e),
		CopyError::InvalidFq(_) => err_json(StatusCode::BAD_REQUEST, "invalid_fq", e),
		CopyError::Io { .. } => err_json(StatusCode::INTERNAL_SERVER_ERROR, "io", e),
		CopyError::Parse(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "toml_parse", e),
	}
}

/// fragment を対象ディレクトリ配下に書き戻す。spec §9.4 `POST /fragment/paste`。
///
/// 書き込み後は `reload_runtime` で runtime を再構築 + `FlowgraphReloaded` WS push。
#[post("/flowgraph/fragment/paste")]
pub async fn post_fragment_paste(
	state: Data<SharedState>,
	req: Json<PasteRequest>,
) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let report = match paste_fragment(&root, &req) {
		Ok(r) => r,
		Err(e) => return paste_error_to_response(e),
	};
	// 書き込みが発生した場合のみ reload + push。write_files が空でも念のため reload する
	// （不整合な状態を避ける）。
	let (reload_ok, diagnostics) = reload_runtime(&state, &root).await;
	let node_count = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await;
		rt.as_ref().map(|r| r.node_meta.len()).unwrap_or(0)
	};
	HttpResponse::Ok().json(PasteResponse {
		report,
		reload_ok,
		diagnostics_count: diagnostics.len(),
		node_count,
	})
}

fn paste_error_to_response(e: PasteError) -> HttpResponse {
	use actix_web::http::StatusCode;
	match e {
		PasteError::RootMissing(_) => {
			err_json(StatusCode::INTERNAL_SERVER_ERROR, "flowgraph_dir_missing", e)
		}
		PasteError::Parse(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "fragment_parse", e),
		PasteError::InvalidTarget(_) => err_json(StatusCode::BAD_REQUEST, "invalid_target", e),
		PasteError::InvalidFragmentPath(_) => {
			err_json(StatusCode::BAD_REQUEST, "invalid_fragment_path", e)
		}
		PasteError::Io { .. } => err_json(StatusCode::INTERNAL_SERVER_ERROR, "io", e),
		PasteError::ExistingParse { .. } => {
			err_json(StatusCode::CONFLICT, "existing_parse_failed", e)
		}
	}
}

// ============================================================================
// Export / Import ZIP（Phase δ-7d, spec §9.3 / §9.4）
// ============================================================================

/// export 用 query. `GET` でバイナリを返すが、payload が CopyRequest なので POST + JSON body
/// で受ける（spec は GET を想定するが、GUI が targets を body で送れる方が現実的なため **POST** 採用）。
#[post("/flowgraph/export/zip")]
pub async fn post_export_zip(state: Data<SharedState>, req: Json<CopyRequest>) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	match export_zip(&root, &req) {
		Ok(bytes) => HttpResponse::Ok()
			.content_type("application/zip")
			.append_header((
				actix_web::http::header::CONTENT_DISPOSITION,
				"attachment; filename=\"fragment.flowgraph.zip\"",
			))
			.body(bytes),
		Err(e) => export_zip_error_to_response(e),
	}
}

fn export_zip_error_to_response(e: ZipExportError) -> HttpResponse {
	use actix_web::http::StatusCode;
	match e {
		ZipExportError::Copy(c) => copy_error_to_response(c),
		ZipExportError::Serialize(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "serialize", e),
		ZipExportError::Zip(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "zip", e),
		ZipExportError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "io", e),
	}
}

/// `POST /flowgraph/import/zip`
///
/// Body: **raw ZIP バイナリ**（`Content-Type: application/zip`）。
/// Query string で `dry_run`（bool）と `target_prefix`（`imported` デフォルト）を受け取る。
///
/// `paste` オプション（衝突戦略・remap 等）は spec §9.4 では body 内で JSON を期待するが、
/// actix multipart 依存を避けるため **query で絞った簡易版**（`on_conflict_node=suffix|skip|overwrite`）のみサポート。
/// 詳細 remap を指定したい場合は、ZIP を copy API で取り出した fragment に対して paste API を直接叩く
/// （ZIP はそのバルク輸送用途）。
#[derive(Debug, Deserialize)]
pub struct ImportZipQuery {
	#[serde(default)]
	pub dry_run: bool,
	#[serde(default)]
	pub target_prefix: Option<String>,
	#[serde(default)]
	pub on_conflict_node: Option<String>,
	#[serde(default)]
	pub on_conflict_file: Option<String>,
}

#[post("/flowgraph/import/zip")]
pub async fn post_import_zip(
	state: Data<SharedState>,
	query: web::Query<ImportZipQuery>,
	body: web::Bytes,
) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};

	let mut opts = ZipImportOptions {
		dry_run: query.dry_run,
		target_prefix: query.target_prefix.clone().unwrap_or_else(|| "imported".into()),
		..Default::default()
	};
	if let Some(s) = &query.on_conflict_node {
		opts.paste.on_conflict_node = match s.as_str() {
			"suffix" => crate::flowgraph::fragment::paste::OnConflict::Suffix,
			"skip" => crate::flowgraph::fragment::paste::OnConflict::Skip,
			"overwrite" => crate::flowgraph::fragment::paste::OnConflict::Overwrite,
			other => {
				return err_json(
					actix_web::http::StatusCode::BAD_REQUEST,
					"invalid_on_conflict_node",
					format!("不明: '{other}'"),
				);
			}
		};
	}
	if let Some(s) = &query.on_conflict_file {
		opts.paste.on_conflict_file = match s.as_str() {
			"overwrite" => crate::flowgraph::fragment::paste::OnConflictFile::Overwrite,
			"skip" => crate::flowgraph::fragment::paste::OnConflictFile::Skip,
			"rename" => crate::flowgraph::fragment::paste::OnConflictFile::Rename,
			"merge" => crate::flowgraph::fragment::paste::OnConflictFile::Merge,
			other => {
				return err_json(
					actix_web::http::StatusCode::BAD_REQUEST,
					"invalid_on_conflict_file",
					format!("不明: '{other}'"),
				);
			}
		};
	}

	match import_zip(&root, &body, &opts) {
		Ok(ZipImportOutcome::Preview(p)) => HttpResponse::Ok().json(ZipImportOutcome::Preview(p)),
		Ok(ZipImportOutcome::Report(r)) => {
			// 本番書き込みが起きたので reload + WS push。
			let (_ok, _diags) = reload_runtime(&state, &root).await;
			HttpResponse::Ok().json(ZipImportOutcome::Report(r))
		}
		Err(e) => import_zip_error_to_response(e),
	}
}

fn import_zip_error_to_response(e: ZipImportError) -> HttpResponse {
	use actix_web::http::StatusCode;
	match e {
		ZipImportError::UnsafeEntry(_) => err_json(StatusCode::BAD_REQUEST, "unsafe_entry", e),
		ZipImportError::ManifestMissing => {
			err_json(StatusCode::UNPROCESSABLE_ENTITY, "manifest_missing", e)
		}
		ZipImportError::ManifestParse(_) => {
			err_json(StatusCode::UNPROCESSABLE_ENTITY, "manifest_parse", e)
		}
		ZipImportError::Zip(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "zip_parse", e),
		ZipImportError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "io", e),
		ZipImportError::Paste(p) => paste_error_to_response(p),
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::Path;

	#[test]
	fn fq_to_file_path_basic() {
		let root = Path::new("/abs/flowgraph");
		assert_eq!(
			fq_to_file_path(root, "main").unwrap(),
			Path::new("/abs/flowgraph/main.flowgraph.toml")
		);
		assert_eq!(
			fq_to_file_path(root, "chat-echo/main").unwrap(),
			Path::new("/abs/flowgraph/chat-echo/main.flowgraph.toml")
		);
		// 末尾 / も trim
		assert_eq!(
			fq_to_file_path(root, "/main/").unwrap(),
			Path::new("/abs/flowgraph/main.flowgraph.toml")
		);
	}

	#[test]
	fn fq_to_file_path_rejects_traversal() {
		let root = Path::new("/abs/flowgraph");
		assert!(fq_to_file_path(root, "").is_err());
		assert!(fq_to_file_path(root, "../sneak").is_err());
		assert!(fq_to_file_path(root, "dir/../sneak").is_err());
		assert!(fq_to_file_path(root, "././a").is_err());
		assert!(fq_to_file_path(root, "dir\\sneak").is_err());
	}

	#[test]
	fn ensure_within_root_lexical_when_root_missing() {
		// root が存在しないときは lexical 比較にフォールバック。
		let root = Path::new("C:/nonexistent-vac-test-xyz");
		let inside = root.join("main.flowgraph.toml");
		assert!(ensure_within_root(root, &inside).is_ok());

		let outside = Path::new("C:/other/main.flowgraph.toml");
		assert!(ensure_within_root(root, outside).is_err());
	}

	// =========================================================================
	// φ-2: Flowgraph Trigger Endpoint の検証ロジック単体テスト
	// =========================================================================

	use crate::flowgraph::socket::SocketValue;
	use serde_json::json;

	fn learn_spec() -> crate::flowgraph::node::NodeSpec {
		registry().spec("flowgraph.dictionary.learn").expect("learn が registry に必要")
	}

	#[test]
	fn unwrap_typed_value_strips_wrapper() {
		let wrapped = json!({ "type": "string", "value": "DR.USAGI" });
		assert_eq!(unwrap_typed_value(&wrapped), &json!("DR.USAGI"));

		let bare = json!("DR.USAGI");
		assert_eq!(unwrap_typed_value(&bare), &bare);

		// 余計なキーが付いていたらそのまま（Json ポート向け）。
		let extra = json!({ "type": "string", "value": "x", "extra": 1 });
		assert_eq!(unwrap_typed_value(&extra), &extra);
	}

	#[test]
	fn registry_marks_dictionary_nodes_triggerable() {
		let r = registry();
		assert!(r.is_control_triggerable("flowgraph.dictionary.learn"));
		assert!(r.is_control_triggerable("flowgraph.dictionary.forget"));
		// 既定はオプトインされていないはず。
		assert!(!r.is_control_triggerable("flowgraph.literal.string"));
		assert!(!r.is_control_triggerable("flowgraph.util.log"));
		assert!(!r.is_control_triggerable("flowgraph.channel.emit"));
		// 未登録は無条件 false。
		assert!(!r.is_control_triggerable("flowgraph.does.not.exist"));
	}

	/// φ-6: node-catalog JSON に `control_triggerable` が混ざっているかの検証。
	/// Flowgraph Editor の Trigger ボタン表示判定が GUI 側の主キーになるため、
	/// dictionary.learn / .forget が true、その他が false になっていることを固定化する。
	#[test]
	fn node_catalog_json_injects_control_triggerable_flag() {
		let reg = registry();
		let specs = reg.all_specs();
		let values: Vec<serde_json::Value> = specs
			.iter()
			.map(|s| {
				let mut v = serde_json::to_value(s).unwrap();
				if let Some(obj) = v.as_object_mut() {
					obj.insert(
						"control_triggerable".to_string(),
						serde_json::Value::Bool(reg.is_control_triggerable(&s.feature)),
					);
				}
				v
			})
			.collect();

		let by_feature: std::collections::HashMap<&str, &serde_json::Value> = values
			.iter()
			.map(|v| (v["feature"].as_str().unwrap(), v))
			.collect();

		// opt-in 済み: dictionary.learn / .forget
		for f in ["flowgraph.dictionary.learn", "flowgraph.dictionary.forget"] {
			let spec = by_feature.get(f).unwrap_or_else(|| panic!("{f} が node-catalog にいない"));
			assert_eq!(
				spec["control_triggerable"].as_bool(),
				Some(true),
				"{f} は control_triggerable=true であるべき"
			);
		}

		// 典型的な non-opt-in: literal / log / tts / dictionary.replace
		for f in [
			"flowgraph.literal.string",
			"flowgraph.util.log",
			"flowgraph.tts.speak",
			"flowgraph.dictionary.replace",
		] {
			let spec = by_feature.get(f).unwrap_or_else(|| panic!("{f} が node-catalog にいない"));
			assert_eq!(
				spec["control_triggerable"].as_bool(),
				Some(false),
				"{f} は control_triggerable=false であるべき"
			);
		}
	}

	#[test]
	fn select_exec_port_picks_first_by_default() {
		let spec = learn_spec();
		let p = select_exec_port(&spec, None).expect("learn は exec_in を持つ");
		assert_eq!(p.name, "exec_in");
		assert!(p.is_exec);
	}

	#[test]
	fn select_exec_port_accepts_matching_override() {
		let spec = learn_spec();
		let p = select_exec_port(&spec, Some("exec_in")).unwrap();
		assert_eq!(p.name, "exec_in");
	}

	#[test]
	fn select_exec_port_rejects_non_exec_override() {
		let spec = learn_spec();
		// `source` は String 入力であって exec ではない。
		let err = select_exec_port(&spec, Some("source")).unwrap_err();
		assert!(matches!(err, TriggerValidationError::ExecPortNotExec(ref n) if n == "source"));
		assert_eq!(err.status(), actix_web::http::StatusCode::BAD_REQUEST);
		assert_eq!(err.code(), "invalid_port");
	}

	#[test]
	fn select_exec_port_rejects_missing_override() {
		let spec = learn_spec();
		let err = select_exec_port(&spec, Some("ghost")).unwrap_err();
		assert!(matches!(err, TriggerValidationError::ExecPortNotFound(_)));
		assert_eq!(err.status(), actix_web::http::StatusCode::NOT_FOUND);
	}

	#[test]
	fn coerce_input_overrides_happy_path() {
		let spec = learn_spec();
		let inputs = serde_json::Map::from_iter([
			("source".into(), json!("ドクターウサギ")),
			("replacement".into(), json!({"type":"string","value":"DR.USAGI"})),
			("priority".into(), json!(3)),
		]);
		let (map, names) = coerce_input_overrides(&spec, &inputs).unwrap();
		assert_eq!(names.len(), 3);
		assert!(matches!(map.get("source"), Some(SocketValue::String(s)) if s == "ドクターウサギ"));
		assert!(matches!(map.get("replacement"), Some(SocketValue::String(s)) if s == "DR.USAGI"));
		assert!(matches!(map.get("priority"), Some(SocketValue::Int(3))));
	}

	#[test]
	fn coerce_input_overrides_unknown_port() {
		let spec = learn_spec();
		let inputs = serde_json::Map::from_iter([("ghost".into(), json!("x"))]);
		let err = coerce_input_overrides(&spec, &inputs).unwrap_err();
		assert!(matches!(err, TriggerValidationError::UnknownPort(ref n) if n == "ghost"));
		assert_eq!(err.status(), actix_web::http::StatusCode::BAD_REQUEST);
		assert_eq!(err.code(), "unknown_port");
	}

	#[test]
	fn coerce_input_overrides_exec_port_rejected() {
		let spec = learn_spec();
		let inputs = serde_json::Map::from_iter([("exec_in".into(), json!(true))]);
		let err = coerce_input_overrides(&spec, &inputs).unwrap_err();
		assert!(matches!(err, TriggerValidationError::ExecPortInData(_)));
	}

	#[test]
	fn coerce_input_overrides_type_mismatch() {
		let spec = learn_spec();
		// priority は int 要求だが string を投げる → 422。
		let inputs = serde_json::Map::from_iter([("priority".into(), json!("not-a-number"))]);
		let err = coerce_input_overrides(&spec, &inputs).unwrap_err();
		assert!(matches!(err, TriggerValidationError::InputTypeMismatch { .. }));
		assert_eq!(err.status(), actix_web::http::StatusCode::UNPROCESSABLE_ENTITY);
		assert_eq!(err.code(), "input_type_mismatch");
	}

	#[test]
	fn coerce_input_overrides_empty_is_ok() {
		let spec = learn_spec();
		let (map, names) = coerce_input_overrides(&spec, &serde_json::Map::new()).unwrap();
		assert!(map.is_empty());
		assert!(names.is_empty());
	}
}
