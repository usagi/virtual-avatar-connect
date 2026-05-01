//! Phase δ-6: Flowgraph 編集 GUI のための Control API。
//!
//! spec §9.5 に相当する read / write エンドポイント群。共通処理は [`util`]、ランタイム再構築は [`reload`]、
//! トリガーは [`trigger`]、フラグメント/ZIP は [`fragment_zip`]。
//!
//! ## パス表現
//!
//! URL 上は `/flowgraph/file/{fq:.*}` の形を採用し、`fq` は拡張子を除いた root 相対パス（`/` 区切り）。
//! 例: `chat-echo/main` ↔ `<flowgraph_dir>/chat-echo/main.flowgraph.toml`。
//! fq は `parse_port_ref` と同じ命名体系なので、ノード参照 (`chat-echo/main::node_id`) と共通化される。
//!
//! ## 安全設計
//!
//! [`util::fq_to_file_path`] で `<flowgraph_dir>/<fq>.flowgraph.toml` を組み立てた後、
//! 親ディレクトリを canonicalize して `<flowgraph_dir>` の配下であることを確認する（path traversal 防御）。
//! `..` や絶対パスは fq パース段階で拒否。

mod fragment_zip;
mod reload;
mod trigger;
mod util;

pub(crate) use reload::reload_runtime;

use actix_web::web::{self, Data, Json};
use actix_web::{delete, get, post, put, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::flowgraph::loader::{parse_flowgraph_file, Diagnostic, FlowgraphFile};
use crate::flowgraph::quantity::{parse_unit, Quantity};
use crate::flowgraph::registry::registry;
use crate::SharedState;

use util::{
	enrich_node_catalog_spec_json, ensure_within_root, err_json, flowgraph_dir, fq_to_file_path, make_backup, root_relative,
	spawn_os_opener,
};

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
	// Phase φ-6: Trigger 表示用 `control_triggerable` を JSON に注入。
	// Phase ξ-5: Quantity ポートに `quantity_dim` / `quantity_unit_*` を付与（default から復元できる場合のみ）。
	let reg = registry();
	let specs = reg.all_specs();
	let values: Vec<serde_json::Value> = specs
		.iter()
		.map(|s| {
			let mut v = serde_json::to_value(s).unwrap();
			enrich_node_catalog_spec_json(reg, &mut v);
			v
		})
		.collect();
	HttpResponse::Ok().json(NodeCatalogResponse {
		count: values.len(),
		specs: values,
	})
}

#[derive(Debug, Deserialize)]
struct ParseUnitQuery {
	/// URL クエリで渡す単位文字列（例 `m%2Fs%5E2`）。空は dimensionless として valid。
	text: String,
}

/// Phase ξ-5: プロパティエディタが単位文字列をサーバと同じ `parse_unit` で検証するための軽量 API。
#[get("/flowgraph/parse-unit")]
pub async fn get_parse_unit(q: web::Query<ParseUnitQuery>) -> impl Responder {
	let t = q.text.trim();
	if t.is_empty() {
		return HttpResponse::Ok().json(serde_json::json!({
			"valid": true,
			"dimension": serde_json::Value::Null,
			"canonical_unit": "",
			"error": serde_json::Value::Null,
		}));
	}
	match parse_unit(t) {
		Ok(u) => {
			let q = Quantity::of(1.0, u);
			let dim = q.dimension();
			HttpResponse::Ok().json(serde_json::json!({
				"valid": true,
				"dimension": if dim.is_dimensionless() {
					serde_json::Value::Null
				} else {
					serde_json::json!(dim.canonical())
				},
				"canonical_unit": q.unit.canonical(),
				"error": serde_json::Value::Null,
			}))
		}
		Err(e) => HttpResponse::Ok().json(serde_json::json!({
			"valid": false,
			"dimension": serde_json::Value::Null,
			"canonical_unit": serde_json::Value::Null,
			"error": e.to_string(),
		})),
	}
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
	/// LF-2: graph 全体が要求する capability summary。
	pub capability_summary: crate::flowgraph::loader::GraphCapabilitySummary,
	/// LF-7: ロード直後の stateful node summary。worker 実行後の live state ではない。
	pub loaded_state_summary: crate::flowgraph::ProgramStateSummary,
	/// LF-7: ロード直後に snapshot export できる state payload。worker 実行後の live state ではない。
	pub loaded_state_snapshot: crate::flowgraph::ProgramStateSnapshot,
	/// RM-3: 各 flowgraph ファイルの mode 用メタ（`[meta].mode_groups` / `default_enabled`）。
	pub file_activation: std::collections::HashMap<String, crate::flowgraph::FlowgraphFileActivationMeta>,
	/// RM-3: exec が抑止されているノード ID。
	pub inactive_exec_nodes: Vec<String>,
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
	let inactive_exec_nodes = rt.trigger_gate.as_ref().map(|g| g.inactive_node_ids()).unwrap_or_default();
	HttpResponse::Ok().json(DiagnosticsResponse {
		root_dir: rt.root_dir.display().to_string().replace('\\', "/"),
		ok: rt.ok,
		diagnostics: rt.diagnostics.clone(),
		node_meta: rt.node_meta.clone(),
		capability_summary: rt.capability_summary.clone(),
		loaded_state_summary: rt.loaded_state_summary.clone(),
		loaded_state_snapshot: rt.loaded_state_snapshot.clone(),
		file_activation: rt.file_activation.clone(),
		inactive_exec_nodes,
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
pub async fn put_file(state: Data<SharedState>, fq_path: web::Path<String>, body: Json<PutFileRequest>) -> impl Responder {
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
	log::info!(
		"《Flowgraph》 open-external: {} (cmd={command}, spawned={spawned})",
		file_path.display()
	);
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
		.service(get_parse_unit)
		.service(get_tree)
		.service(get_diagnostics)
		.service(post_reload)
		.service(fragment_zip::post_fragment_copy)
		.service(fragment_zip::post_fragment_paste)
		.service(fragment_zip::post_export_zip)
		.service(fragment_zip::post_import_zip)
		.service(trigger::post_trigger_node)
		// open-external を file GET より先に登録しないと `{fq:.*}` が open-external も食うため。
		.service(post_open_external)
		.service(post_file)
		.service(put_file)
		.service(delete_file)
		.service(get_file);
}
