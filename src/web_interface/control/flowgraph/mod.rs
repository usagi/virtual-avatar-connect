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

pub(crate) use reload::{reload_runtime, reload_runtime_with_state_snapshot_file};

use actix_web::web::{self, Data, Json};
use actix_web::{delete, get, post, put, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::flowgraph::loader::{parse_flowgraph_file, Diagnostic, FlowgraphFile};
use crate::flowgraph::quantity::{parse_unit, Quantity};
use crate::flowgraph::registry::registry;
use crate::flowgraph::{FlowgraphRuntime, PackageLockFile, PackageLockFileError, ProgramCommandError, SocketType, StateSnapshotFileError};
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

#[derive(Debug, Deserialize)]
struct ParseSocketTypeQuery {
	/// URL クエリで渡す socket type 文字列（例 `list%3Cmap%3Cjson%3E%3E`）。
	text: String,
}

#[derive(Debug, Deserialize)]
struct SocketTypeCompatibilityQuery {
	/// 上流 port の socket type 文字列。
	from: String,
	/// 下流 port の socket type 文字列。
	to: String,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ParseSocketTypeResponse {
	pub valid: bool,
	pub canonical_type: Option<String>,
	pub type_expr: Option<crate::flowgraph::SocketTypeExpr>,
	pub error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct SocketTypeCompatibilityResponse {
	pub valid: bool,
	pub compatible: Option<bool>,
	pub from_canonical_type: Option<String>,
	pub to_canonical_type: Option<String>,
	pub from_type_expr: Option<crate::flowgraph::SocketTypeExpr>,
	pub to_type_expr: Option<crate::flowgraph::SocketTypeExpr>,
	pub error: Option<String>,
}

fn parse_socket_type_response(text: &str) -> ParseSocketTypeResponse {
	match SocketType::parse(text) {
		Ok(ty) => ParseSocketTypeResponse {
			valid: true,
			canonical_type: Some(ty.to_string()),
			type_expr: Some(ty.type_expr()),
			error: None,
		},
		Err(error) => ParseSocketTypeResponse {
			valid: false,
			canonical_type: None,
			type_expr: None,
			error: Some(error.to_string()),
		},
	}
}

fn socket_type_compatibility_response(from: &str, to: &str) -> SocketTypeCompatibilityResponse {
	let from_parsed = SocketType::parse(from);
	let to_parsed = SocketType::parse(to);
	match (from_parsed, to_parsed) {
		(Ok(from_ty), Ok(to_ty)) => SocketTypeCompatibilityResponse {
			valid: true,
			compatible: Some(from_ty.compatible_with(&to_ty)),
			from_canonical_type: Some(from_ty.to_string()),
			to_canonical_type: Some(to_ty.to_string()),
			from_type_expr: Some(from_ty.type_expr()),
			to_type_expr: Some(to_ty.type_expr()),
			error: None,
		},
		(from_result, to_result) => {
			let mut errors = Vec::new();
			if let Err(error) = from_result {
				errors.push(format!("from: {error}"));
			}
			if let Err(error) = to_result {
				errors.push(format!("to: {error}"));
			}
			SocketTypeCompatibilityResponse {
				valid: false,
				compatible: None,
				from_canonical_type: None,
				to_canonical_type: None,
				from_type_expr: None,
				to_type_expr: None,
				error: Some(errors.join("; ")),
			}
		}
	}
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

/// LF-5: GUI / tooling がサーバと同じ `SocketType::parse` で generic 型文字列を検証するための軽量 API。
#[get("/flowgraph/parse-socket-type")]
pub async fn get_parse_socket_type(q: web::Query<ParseSocketTypeQuery>) -> impl Responder {
	HttpResponse::Ok().json(parse_socket_type_response(&q.text))
}

/// LF-5: GUI / tooling がサーバと同じ socket type compatibility rule を参照するための軽量 API。
#[get("/flowgraph/socket-type-compatibility")]
pub async fn get_socket_type_compatibility(q: web::Query<SocketTypeCompatibilityQuery>) -> impl Responder {
	HttpResponse::Ok().json(socket_type_compatibility_response(&q.from, &q.to))
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
	/// LF-1: graph-as-node / library signature の入口となる read-only graph signature。
	pub graph_signature: crate::flowgraph::loader::GraphSignature,
	/// LF-4: `[package]` manifest catalog。
	pub package_manifests: Vec<crate::flowgraph::loader::PackageManifestSummary>,
	/// LF-4: local package dependency DAG の dependency-first order。
	pub package_dependency_order: Vec<String>,
	/// LF-4: lockfile 生成前に観測できる read-only preview。
	pub package_lock_preview: Vec<crate::flowgraph::loader::PackageLockEntry>,
	/// LF-5: `[[types]]` named schema metadata catalog。
	pub type_schemas: Vec<crate::flowgraph::loader::TypeSchemaSummary>,
	/// LF-5: record schema dependency graph の dependency-first order。
	pub type_schema_dependency_order: Vec<String>,
	/// LF-4: package lock preview 全体の stable digest。
	pub package_lock_preview_digest: Option<String>,
	/// LF-7: ロード直後の stateful node summary。worker 実行後の live state ではない。
	pub loaded_state_summary: crate::flowgraph::ProgramStateSummary,
	/// LF-7: ロード直後に snapshot export できる state payload。worker 実行後の live state ではない。
	pub loaded_state_snapshot: crate::flowgraph::ProgramStateSnapshot,
	/// LF-7: 明示 restore 付き load の場合に、実際に restore された node の report。
	pub loaded_state_restore_report: Option<crate::flowgraph::ProgramStateRestoreReport>,
	/// LF-7: profile-local state snapshot file の予定保存先。自動 read/write はまだ行わない。
	pub state_snapshot_file_path: Option<String>,
	/// LF-7: `state_snapshot_file_path` が指す snapshot file envelope が存在するか。
	pub state_snapshot_file_exists: Option<bool>,
	/// RM-3: 各 flowgraph ファイルの mode 用メタ（`[meta].mode_groups` / `default_enabled`）。
	pub file_activation: std::collections::HashMap<String, crate::flowgraph::FlowgraphFileActivationMeta>,
	/// RM-3: exec が抑止されているノード ID。
	pub inactive_exec_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct StateSnapshotFileStatus {
	path: String,
	exists: bool,
}

fn state_snapshot_file_status(rt: &FlowgraphRuntime) -> Option<StateSnapshotFileStatus> {
	rt.state_snapshot_file_path.as_ref().map(|path| StateSnapshotFileStatus {
		path: path.display().to_string().replace('\\', "/"),
		exists: path.is_file(),
	})
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
	let state_snapshot_file = state_snapshot_file_status(rt);
	HttpResponse::Ok().json(DiagnosticsResponse {
		root_dir: rt.root_dir.display().to_string().replace('\\', "/"),
		ok: rt.ok,
		diagnostics: rt.diagnostics.clone(),
		node_meta: rt.node_meta.clone(),
		capability_summary: rt.capability_summary.clone(),
		graph_signature: rt.graph_signature.clone(),
		package_manifests: rt.package_manifests.clone(),
		package_dependency_order: rt.package_dependency_order.clone(),
		package_lock_preview: rt.package_lock_preview.clone(),
		type_schemas: rt.type_schemas.clone(),
		type_schema_dependency_order: rt.type_schema_dependency_order.clone(),
		package_lock_preview_digest: rt.package_lock_preview_digest.clone(),
		loaded_state_summary: rt.loaded_state_summary.clone(),
		loaded_state_snapshot: rt.loaded_state_snapshot.clone(),
		loaded_state_restore_report: rt.loaded_state_restore_report.clone(),
		state_snapshot_file_path: state_snapshot_file.as_ref().map(|status| status.path.clone()),
		state_snapshot_file_exists: state_snapshot_file.as_ref().map(|status| status.exists),
		file_activation: rt.file_activation.clone(),
		inactive_exec_nodes,
	})
}

#[get("/flowgraph/signature")]
pub async fn get_signature(state: Data<SharedState>) -> impl Responder {
	let fg = state.read().await.flowgraph.clone();
	let rt = fg.read().await;
	let Some(rt) = rt.as_ref() else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	HttpResponse::Ok().json(rt.graph_signature.clone())
}

// ============================================================================
// GET /flowgraph/package-lock-preview
// ============================================================================

#[derive(Debug, Serialize)]
pub struct PackageLockPreviewResponse {
	pub digest: Option<String>,
	pub entries: Vec<crate::flowgraph::loader::PackageLockEntry>,
	pub entry_count: usize,
}

fn package_lock_preview_response(rt: &FlowgraphRuntime) -> PackageLockPreviewResponse {
	let entries = rt.package_lock_preview.clone();
	PackageLockPreviewResponse {
		digest: rt.package_lock_preview_digest.clone(),
		entry_count: entries.len(),
		entries,
	}
}

#[get("/flowgraph/package-lock-preview")]
pub async fn get_package_lock_preview(state: Data<SharedState>) -> impl Responder {
	let fg = state.read().await.flowgraph.clone();
	let rt = fg.read().await;
	let Some(rt) = rt.as_ref() else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	HttpResponse::Ok().json(package_lock_preview_response(rt))
}

// ============================================================================
// POST /flowgraph/package-lock-preview/save
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SavePackageLockPreviewResponse {
	pub path: String,
	pub digest: Option<String>,
	pub entry_count: usize,
	pub written: bool,
}

#[derive(Debug)]
enum SavePackageLockPreviewError {
	BadIfMatch {
		raw: String,
	},
	OptimisticLockFailed {
		current_digest: Option<String>,
	},
	ReadCurrentFailed {
		path: std::path::PathBuf,
		error: PackageLockFileError,
	},
	WriteFailed {
		path: std::path::PathBuf,
		error: PackageLockFileError,
	},
}

fn package_lock_if_match_hex(raw: &str) -> Option<String> {
	let hex_part = raw.trim().trim_matches('"').trim_start_matches("b3:");
	if hex_part.len() == 64 && hex_part.chars().all(|ch| ch.is_ascii_hexdigit()) {
		Some(hex_part.to_ascii_lowercase())
	} else {
		None
	}
}

fn package_lock_digest_hex(digest: &str) -> Option<String> {
	let hex_part = digest.trim().trim_start_matches("b3:");
	if hex_part.len() == 64 && hex_part.chars().all(|ch| ch.is_ascii_hexdigit()) {
		Some(hex_part.to_ascii_lowercase())
	} else {
		None
	}
}

fn check_package_lock_if_match(path: &std::path::Path, if_match: Option<&str>) -> Result<(), SavePackageLockPreviewError> {
	let Some(raw) = if_match else {
		return Ok(());
	};
	let Some(expected_hex) = package_lock_if_match_hex(raw) else {
		return Err(SavePackageLockPreviewError::BadIfMatch { raw: raw.to_string() });
	};
	match crate::flowgraph::read_package_lock_file(path) {
		Ok(file) => {
			let current_digest = file.digest.clone();
			if current_digest
				.as_deref()
				.and_then(package_lock_digest_hex)
				.is_some_and(|current_hex| current_hex.eq_ignore_ascii_case(&expected_hex))
			{
				Ok(())
			} else {
				Err(SavePackageLockPreviewError::OptimisticLockFailed { current_digest })
			}
		}
		Err(PackageLockFileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => {
			Err(SavePackageLockPreviewError::OptimisticLockFailed { current_digest: None })
		}
		Err(error) => Err(SavePackageLockPreviewError::ReadCurrentFailed {
			path: path.to_path_buf(),
			error,
		}),
	}
}

fn save_package_lock_preview_response(
	rt: &FlowgraphRuntime,
	if_match: Option<&str>,
) -> Result<SavePackageLockPreviewResponse, SavePackageLockPreviewError> {
	let path = rt.root_dir.join(crate::flowgraph::FLOWGRAPH_PACKAGE_LOCK_FILE_NAME);
	check_package_lock_if_match(&path, if_match)?;
	let file = PackageLockFile::new(rt.package_lock_preview_digest.clone(), rt.package_lock_preview.clone());
	let entry_count = file.entry_count;
	let digest = file.digest.clone();
	match crate::flowgraph::write_package_lock_file(&path, &file) {
		Ok(()) => Ok(SavePackageLockPreviewResponse {
			path: path.display().to_string().replace('\\', "/"),
			digest,
			entry_count,
			written: true,
		}),
		Err(error) => Err(SavePackageLockPreviewError::WriteFailed { path, error }),
	}
}

#[post("/flowgraph/package-lock-preview/save")]
pub async fn post_save_package_lock_preview(state: Data<SharedState>, req: HttpRequest) -> impl Responder {
	let rt = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await.clone();
		rt
	};
	let Some(rt) = rt else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	let if_match = match req.headers().get("If-Match") {
		Some(value) => match value.to_str() {
			Ok(raw) => Some(raw),
			Err(_) => {
				return err_json(
					actix_web::http::StatusCode::BAD_REQUEST,
					"bad_if_match",
					"If-Match ヘッダが ASCII ではありません",
				)
			}
		},
		None => None,
	};
	match save_package_lock_preview_response(&rt, if_match) {
		Ok(response) => HttpResponse::Ok().json(response),
		Err(SavePackageLockPreviewError::BadIfMatch { raw }) => err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"bad_if_match",
			format!("If-Match は `b3:<64 hex>` 形式です。got='{raw}'"),
		),
		Err(SavePackageLockPreviewError::OptimisticLockFailed { current_digest }) => err_json(
			actix_web::http::StatusCode::CONFLICT,
			"optimistic_lock_failed",
			match current_digest {
				Some(digest) => format!("If-Match 不一致: 現在の package lock digest は {digest}"),
				None => "If-Match 不一致: package lockfile は未作成です".to_string(),
			},
		),
		Err(SavePackageLockPreviewError::ReadCurrentFailed { path, error }) => err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"package_lock_read_failed",
			format!("package lockfile の読み込みに失敗しました ({}): {error}", path.display()),
		),
		Err(SavePackageLockPreviewError::WriteFailed { path, error }) => err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"package_lock_write_failed",
			format!("package lockfile の保存に失敗しました ({}): {error}", path.display()),
		),
	}
}

// ============================================================================
// GET /flowgraph/package-lock
// ============================================================================

#[derive(Debug, Serialize)]
pub struct PackageLockStatusResponse {
	pub path: String,
	pub exists: bool,
	pub preview_digest: Option<String>,
	pub lock_digest: Option<String>,
	pub matches_preview: Option<bool>,
	pub entry_count: Option<usize>,
	pub diff: Option<PackageLockStatusDiff>,
	pub file: Option<PackageLockFile>,
	pub error: Option<String>,
}

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct PackageLockStatusDiff {
	pub added_ids: Vec<String>,
	pub removed_ids: Vec<String>,
	pub changed_ids: Vec<String>,
	pub unchanged_ids: Vec<String>,
}

fn package_lock_status_diff(
	saved: &[crate::flowgraph::loader::PackageLockEntry],
	preview: &[crate::flowgraph::loader::PackageLockEntry],
) -> PackageLockStatusDiff {
	let saved_by_id: std::collections::BTreeMap<&str, &crate::flowgraph::loader::PackageLockEntry> =
		saved.iter().map(|entry| (entry.id.as_str(), entry)).collect();
	let preview_by_id: std::collections::BTreeMap<&str, &crate::flowgraph::loader::PackageLockEntry> =
		preview.iter().map(|entry| (entry.id.as_str(), entry)).collect();
	let mut added_ids: Vec<String> = Vec::new();
	let mut removed_ids: Vec<String> = Vec::new();
	let mut changed_ids: Vec<String> = Vec::new();
	let mut unchanged_ids: Vec<String> = Vec::new();
	for (id, preview_entry) in &preview_by_id {
		match saved_by_id.get(id) {
			Some(saved_entry) if saved_entry.digest == preview_entry.digest => unchanged_ids.push((*id).to_string()),
			Some(_) => changed_ids.push((*id).to_string()),
			None => added_ids.push((*id).to_string()),
		}
	}
	for id in saved_by_id.keys() {
		if !preview_by_id.contains_key(id) {
			removed_ids.push((*id).to_string());
		}
	}
	PackageLockStatusDiff {
		added_ids,
		removed_ids,
		changed_ids,
		unchanged_ids,
	}
}

fn package_lock_status_response(rt: &FlowgraphRuntime) -> PackageLockStatusResponse {
	let path = rt.root_dir.join(crate::flowgraph::FLOWGRAPH_PACKAGE_LOCK_FILE_NAME);
	let path_text = path.display().to_string().replace('\\', "/");
	let preview_digest = rt.package_lock_preview_digest.clone();
	match crate::flowgraph::read_package_lock_file(&path) {
		Ok(file) => {
			let lock_digest = file.digest.clone();
			let matches_preview = Some(lock_digest == preview_digest);
			let diff = package_lock_status_diff(&file.entries, &rt.package_lock_preview);
			PackageLockStatusResponse {
				path: path_text,
				exists: true,
				preview_digest,
				lock_digest,
				matches_preview,
				entry_count: Some(file.entry_count),
				diff: Some(diff),
				file: Some(file),
				error: None,
			}
		}
		Err(PackageLockFileError::Io(error)) if error.kind() == std::io::ErrorKind::NotFound => PackageLockStatusResponse {
			path: path_text,
			exists: false,
			preview_digest,
			lock_digest: None,
			matches_preview: None,
			entry_count: None,
			diff: None,
			file: None,
			error: None,
		},
		Err(error) => PackageLockStatusResponse {
			path: path_text,
			exists: true,
			preview_digest,
			lock_digest: None,
			matches_preview: None,
			entry_count: None,
			diff: None,
			file: None,
			error: Some(error.to_string()),
		},
	}
}

#[get("/flowgraph/package-lock")]
pub async fn get_package_lock(state: Data<SharedState>) -> impl Responder {
	let fg = state.read().await.flowgraph.clone();
	let rt = fg.read().await;
	let Some(rt) = rt.as_ref() else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	HttpResponse::Ok().json(package_lock_status_response(rt))
}

// ============================================================================
// POST /flowgraph/state-snapshot/loaded/save
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SaveLoadedStateSnapshotResponse {
	pub path: String,
	pub snapshot_node_count: usize,
	pub written: bool,
}

#[derive(Debug)]
enum SaveLoadedStateSnapshotError {
	PathUnset,
	WriteFailed {
		path: std::path::PathBuf,
		error: StateSnapshotFileError,
	},
}

fn save_loaded_state_snapshot_response(rt: &FlowgraphRuntime) -> Result<SaveLoadedStateSnapshotResponse, SaveLoadedStateSnapshotError> {
	let Some(path) = rt.state_snapshot_file_path.clone() else {
		return Err(SaveLoadedStateSnapshotError::PathUnset);
	};
	let snapshot_node_count = rt.loaded_state_snapshot.snapshot_node_count;
	match rt.write_loaded_state_snapshot_file() {
		Ok(Some(written_path)) => Ok(SaveLoadedStateSnapshotResponse {
			path: written_path.display().to_string().replace('\\', "/"),
			snapshot_node_count,
			written: true,
		}),
		Ok(None) => Err(SaveLoadedStateSnapshotError::PathUnset),
		Err(error) => Err(SaveLoadedStateSnapshotError::WriteFailed { path, error }),
	}
}

#[post("/flowgraph/state-snapshot/loaded/save")]
pub async fn post_save_loaded_state_snapshot(state: Data<SharedState>) -> impl Responder {
	let rt = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await.clone();
		rt
	};
	let Some(rt) = rt else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	match save_loaded_state_snapshot_response(&rt) {
		Ok(response) => HttpResponse::Ok().json(response),
		Err(SaveLoadedStateSnapshotError::PathUnset) => err_json(
			actix_web::http::StatusCode::CONFLICT,
			"state_snapshot_file_path_unset",
			"state snapshot file path が未設定です。profile-local snapshot path metadata を持つ runtime が必要です。",
		),
		Err(SaveLoadedStateSnapshotError::WriteFailed { path, error }) => err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"state_snapshot_write_failed",
			format!("state snapshot file の保存に失敗しました ({}): {error}", path.display()),
		),
	}
}

// ============================================================================
// POST /flowgraph/state-snapshot/live/save
// ============================================================================

#[derive(Debug, Serialize)]
pub struct SaveLiveStateSnapshotResponse {
	pub path: String,
	pub snapshot_node_count: usize,
	pub written: bool,
}

#[derive(Debug)]
enum SaveLiveStateSnapshotError {
	PathUnset,
	WorkerUnavailable,
	CommandFailed(ProgramCommandError),
	WriteFailed {
		path: std::path::PathBuf,
		error: StateSnapshotFileError,
	},
}

async fn save_live_state_snapshot_response(rt: &FlowgraphRuntime) -> Result<SaveLiveStateSnapshotResponse, SaveLiveStateSnapshotError> {
	let Some(path) = rt.state_snapshot_file_path.clone() else {
		return Err(SaveLiveStateSnapshotError::PathUnset);
	};
	let Some(snapshot) = rt
		.export_live_state_snapshot()
		.await
		.map_err(SaveLiveStateSnapshotError::CommandFailed)?
	else {
		return Err(SaveLiveStateSnapshotError::WorkerUnavailable);
	};
	let snapshot_node_count = snapshot.snapshot_node_count;
	let file = crate::flowgraph::ProgramStateSnapshotFile::new(snapshot);
	crate::flowgraph::write_state_snapshot_file(&path, &file)
		.map_err(|error| SaveLiveStateSnapshotError::WriteFailed { path: path.clone(), error })?;
	Ok(SaveLiveStateSnapshotResponse {
		path: path.display().to_string().replace('\\', "/"),
		snapshot_node_count,
		written: true,
	})
}

#[post("/flowgraph/state-snapshot/live/save")]
pub async fn post_save_live_state_snapshot(state: Data<SharedState>) -> impl Responder {
	let rt = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await.clone();
		rt
	};
	let Some(rt) = rt else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	match save_live_state_snapshot_response(&rt).await {
		Ok(response) => HttpResponse::Ok().json(response),
		Err(SaveLiveStateSnapshotError::PathUnset) => err_json(
			actix_web::http::StatusCode::CONFLICT,
			"state_snapshot_file_path_unset",
			"state snapshot file path が未設定です。profile-local snapshot path metadata を持つ runtime が必要です。",
		),
		Err(SaveLiveStateSnapshotError::WorkerUnavailable) => err_json(
			actix_web::http::StatusCode::CONFLICT,
			"flowgraph_worker_unavailable",
			"live state snapshot を取得するには起動中の Flowgraph worker が必要です。",
		),
		Err(SaveLiveStateSnapshotError::CommandFailed(error)) => err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"state_snapshot_live_export_failed",
			format!("live state snapshot の取得に失敗しました: {error}"),
		),
		Err(SaveLiveStateSnapshotError::WriteFailed { path, error }) => err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"state_snapshot_write_failed",
			format!("state snapshot file の保存に失敗しました ({}): {error}", path.display()),
		),
	}
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

#[derive(Debug, Serialize)]
pub struct ReloadPreservingStateResponse {
	pub root_dir: String,
	pub path: String,
	pub ok: bool,
	pub diagnostics: Vec<Diagnostic>,
	pub node_count: usize,
	pub saved_node_count: usize,
	pub restored_node_count: Option<usize>,
}

#[derive(Debug, Serialize)]
pub struct RestoreStateSnapshotResponse {
	pub root_dir: String,
	pub path: String,
	pub ok: bool,
	pub diagnostics: Vec<Diagnostic>,
	pub node_count: usize,
	pub restored_node_count: Option<usize>,
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

/// 起動中 worker の live state を profile-local snapshot file へ保存してから、同じ snapshot file で reload / restore する。
/// 通常 reload の挙動は変えず、ユーザー操作または外部ツールが明示的に選ぶ state-preserving reload。
#[post("/flowgraph/reload/preserve-state")]
pub async fn post_reload_preserving_state(state: Data<SharedState>) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let rt = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await.clone();
		rt
	};
	let Some(rt) = rt else {
		return err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"flowgraph_dir_unset",
			"conf.flowgraph_dir が未設定です",
		);
	};
	let snapshot_path = match rt.state_snapshot_file_path.clone() {
		Some(path) => path,
		None => {
			return err_json(
				actix_web::http::StatusCode::CONFLICT,
				"state_snapshot_file_path_unset",
				"state snapshot file path が未設定です。profile-local snapshot path metadata を持つ runtime が必要です。",
			)
		}
	};
	let save_response = match save_live_state_snapshot_response(&rt).await {
		Ok(response) => response,
		Err(SaveLiveStateSnapshotError::PathUnset) => {
			return err_json(
				actix_web::http::StatusCode::CONFLICT,
				"state_snapshot_file_path_unset",
				"state snapshot file path が未設定です。profile-local snapshot path metadata を持つ runtime が必要です。",
			)
		}
		Err(SaveLiveStateSnapshotError::WorkerUnavailable) => {
			return err_json(
				actix_web::http::StatusCode::CONFLICT,
				"flowgraph_worker_unavailable",
				"state-preserving reload には起動中の Flowgraph worker が必要です。",
			)
		}
		Err(SaveLiveStateSnapshotError::CommandFailed(error)) => {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"state_snapshot_live_export_failed",
				format!("live state snapshot の取得に失敗しました: {error}"),
			)
		}
		Err(SaveLiveStateSnapshotError::WriteFailed { path, error }) => {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"state_snapshot_write_failed",
				format!("state snapshot file の保存に失敗しました ({}): {error}", path.display()),
			)
		}
	};
	let (ok, diagnostics) = reload_runtime_with_state_snapshot_file(&state, &root, &snapshot_path).await;
	let (node_count, restored_node_count) = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await;
		let node_count = rt.as_ref().map(|r| r.node_meta.len()).unwrap_or(0);
		let restored_node_count = rt
			.as_ref()
			.and_then(|r| r.loaded_state_restore_report.as_ref())
			.map(|report| report.restored_node_count);
		(node_count, restored_node_count)
	};
	HttpResponse::Ok().json(ReloadPreservingStateResponse {
		root_dir: root.display().to_string().replace('\\', "/"),
		path: save_response.path,
		ok,
		diagnostics,
		node_count,
		saved_node_count: save_response.snapshot_node_count,
		restored_node_count,
	})
}

// ============================================================================
// POST /flowgraph/state-snapshot/profile-local/restore
// ============================================================================

/// 現在 profile / flowgraph root に対応する snapshot file envelope を明示 restore して reload する。
/// 自動 restore ではなく、ユーザー操作または外部ツールから叩く手動 API。
#[post("/flowgraph/state-snapshot/profile-local/restore")]
pub async fn post_restore_profile_local_state_snapshot(state: Data<SharedState>) -> impl Responder {
	let (root, _) = match flowgraph_dir(&state).await {
		Ok(v) => v,
		Err(r) => return r,
	};
	let snapshot_path = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await;
		let Some(rt) = rt.as_ref() else {
			return err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"flowgraph_dir_unset",
				"conf.flowgraph_dir が未設定です",
			);
		};
		let Some(path) = rt.state_snapshot_file_path.clone() else {
			return err_json(
				actix_web::http::StatusCode::CONFLICT,
				"state_snapshot_file_path_unset",
				"state snapshot file path が未設定です。profile-local snapshot path metadata を持つ runtime が必要です。",
			);
		};
		path
	};
	let (ok, diagnostics) = reload_runtime_with_state_snapshot_file(&state, &root, &snapshot_path).await;
	let node_count = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await;
		rt.as_ref().map(|r| r.node_meta.len()).unwrap_or(0)
	};
	let restored_node_count = {
		let fg = state.read().await.flowgraph.clone();
		let rt = fg.read().await;
		rt.as_ref()
			.and_then(|r| r.loaded_state_restore_report.as_ref())
			.map(|report| report.restored_node_count)
	};
	HttpResponse::Ok().json(RestoreStateSnapshotResponse {
		root_dir: root.display().to_string().replace('\\', "/"),
		path: snapshot_path.display().to_string().replace('\\', "/"),
		ok,
		diagnostics,
		node_count,
		restored_node_count,
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
		.service(get_parse_socket_type)
		.service(get_socket_type_compatibility)
		.service(get_tree)
		.service(get_diagnostics)
		.service(get_signature)
		.service(get_package_lock_preview)
		.service(get_package_lock)
		.service(post_save_package_lock_preview)
		.service(post_save_loaded_state_snapshot)
		.service(post_save_live_state_snapshot)
		.service(post_reload_preserving_state)
		.service(post_restore_profile_local_state_snapshot)
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::loader::{GraphCapabilitySummary, GraphSignature, PackageLockEntry};
	use crate::flowgraph::{ProgramStateSnapshot, ProgramStateSnapshotNode, ProgramStateSummary, StateSnapshotFormat};
	use std::collections::{BTreeMap, HashMap};
	use std::path::PathBuf;
	use std::time::{SystemTime, UNIX_EPOCH};

	fn snapshot() -> ProgramStateSnapshot {
		ProgramStateSnapshot {
			snapshot_node_count: 1,
			nodes: vec![ProgramStateSnapshotNode {
				node: "main::counter".into(),
				feature: "flowgraph.state.int_counter".into(),
				version: 1,
				format: StateSnapshotFormat::Json,
				value: serde_json::json!({ "value": 7 }),
			}],
		}
	}

	fn temp_dir(label: &str) -> PathBuf {
		let now_ms = SystemTime::now().duration_since(UNIX_EPOCH).expect("time").as_millis();
		let dir = std::env::temp_dir().join(format!("vac-flowgraph-control-save-{label}-{}-{now_ms}", std::process::id()));
		std::fs::create_dir_all(&dir).expect("create temp dir");
		dir
	}

	fn state_counter_root() -> PathBuf {
		std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
			.join("flowgraph.example")
			.join("state-counter")
	}

	fn runtime_with_snapshot_path(path: Option<PathBuf>) -> FlowgraphRuntime {
		FlowgraphRuntime {
			root_dir: PathBuf::from("flowgraph.example"),
			ok: true,
			diagnostics: vec![],
			node_meta: HashMap::new(),
			graph_signature: GraphSignature::default(),
			package_manifests: Vec::new(),
			package_dependency_order: Vec::new(),
			package_lock_preview: Vec::new(),
			type_schemas: Vec::new(),
			type_schema_dependency_order: Vec::new(),
			package_lock_preview_digest: None,
			capability_summary: GraphCapabilitySummary::default(),
			loaded_state_summary: ProgramStateSummary::default(),
			loaded_state_snapshot: snapshot(),
			loaded_state_restore_report: None,
			state_snapshot_file_path: path,
			file_activation: HashMap::new(),
			trigger_gate: None,
			handle: None,
		}
	}

	#[test]
	fn parse_socket_type_response_reports_canonical_expr() {
		let response = parse_socket_type_response(" map<string, list<json>> ");
		assert!(response.valid);
		assert_eq!(response.canonical_type.as_deref(), Some("map<list<json>>"));
		let expr = response.type_expr.expect("type expr");
		assert_eq!(expr.name, "map");
		assert_eq!(expr.args[0].name, "string");
		assert_eq!(expr.args[1].name, "list");
		assert_eq!(expr.args[1].args[0].name, "json");
		assert!(response.error.is_none());

		let option_response = parse_socket_type_response("option<list<string>>");
		assert!(option_response.valid);
		assert_eq!(option_response.canonical_type.as_deref(), Some("option<list<string>>"));
		let option_expr = option_response.type_expr.expect("option expr");
		assert_eq!(option_expr.name, "option");
		assert_eq!(option_expr.args[0].name, "list");

		let dictionary_response = parse_socket_type_response("dictionary<string, list<json>>");
		assert!(dictionary_response.valid);
		assert_eq!(dictionary_response.canonical_type.as_deref(), Some("map<list<json>>"));
		let dictionary_expr = dictionary_response.type_expr.expect("dictionary expr");
		assert_eq!(dictionary_expr.name, "map");
		assert_eq!(dictionary_expr.args[0].name, "string");
		assert_eq!(dictionary_expr.args[1].name, "list");

		let collection_response = parse_socket_type_response("collection<option<string>>");
		assert!(collection_response.valid);
		assert_eq!(collection_response.canonical_type.as_deref(), Some("list<option<string>>"));
		let collection_expr = collection_response.type_expr.expect("collection expr");
		assert_eq!(collection_expr.name, "list");
		assert_eq!(collection_expr.args[0].name, "option");

		let record_response = parse_socket_type_response("record<twitch.event>");
		assert!(record_response.valid);
		assert_eq!(record_response.canonical_type.as_deref(), Some("record<twitch.event>"));
		let record_expr = record_response.type_expr.expect("record expr");
		assert_eq!(record_expr.name, "record");
		assert_eq!(record_expr.display, "record<twitch.event>");
	}

	#[test]
	fn parse_socket_type_response_reports_errors() {
		let response = parse_socket_type_response("map<int, string>");
		assert!(!response.valid);
		assert!(response.canonical_type.is_none());
		assert!(response.type_expr.is_none());
		assert!(response.error.as_deref().unwrap_or_default().contains("map の key 型は string"));
	}

	#[test]
	fn socket_type_compatibility_response_reports_engine_rule() {
		let response = socket_type_compatibility_response("list<float>", "list<quantity>");
		assert!(response.valid);
		assert_eq!(response.compatible, Some(true));
		assert_eq!(response.from_canonical_type.as_deref(), Some("list<float>"));
		assert_eq!(response.to_canonical_type.as_deref(), Some("list<quantity>"));
		assert_eq!(response.from_type_expr.as_ref().expect("from expr").name, "list");
		assert_eq!(response.to_type_expr.as_ref().expect("to expr").args[0].name, "quantity");
		assert!(response.error.is_none());

		let rejected = socket_type_compatibility_response("string", "quantity");
		assert!(rejected.valid);
		assert_eq!(rejected.compatible, Some(false));

		let option = socket_type_compatibility_response("option<list<float>>", "option<list<quantity>>");
		assert!(option.valid);
		assert_eq!(option.compatible, Some(true));
		assert_eq!(option.from_type_expr.as_ref().expect("from expr").name, "option");
		assert_eq!(option.to_type_expr.as_ref().expect("to expr").args[0].name, "list");

		let record = socket_type_compatibility_response("record<twitch.event>", "record<twitch.event>");
		assert!(record.valid);
		assert_eq!(record.compatible, Some(true));
		let different_record = socket_type_compatibility_response("record<twitch.event>", "record<obs.event>");
		assert!(different_record.valid);
		assert_eq!(different_record.compatible, Some(false));
		let json_to_record = socket_type_compatibility_response("json", "record<twitch.event>");
		assert!(json_to_record.valid);
		assert_eq!(json_to_record.compatible, Some(true));
	}

	#[test]
	fn socket_type_compatibility_response_reports_parse_errors() {
		let response = socket_type_compatibility_response("map<int, string>", "list<>");
		assert!(!response.valid);
		assert!(response.compatible.is_none());
		let error = response.error.as_deref().unwrap_or_default();
		assert!(error.contains("from:"), "{error}");
		assert!(error.contains("to:"), "{error}");
	}

	#[test]
	fn save_loaded_state_snapshot_response_writes_snapshot_file() {
		let path = temp_dir("ok").join("profile").join("state.snapshot.json");
		let rt = runtime_with_snapshot_path(Some(path.clone()));

		let response = save_loaded_state_snapshot_response(&rt).expect("save snapshot");

		assert!(response.path.ends_with("/profile/state.snapshot.json"));
		assert_eq!(response.snapshot_node_count, 1);
		assert!(response.written);
		let file = crate::flowgraph::read_state_snapshot_file(&path).expect("read snapshot file");
		assert_eq!(file.snapshot, rt.loaded_state_snapshot);
	}

	#[test]
	fn save_loaded_state_snapshot_response_reports_missing_path() {
		let rt = runtime_with_snapshot_path(None);
		let error = save_loaded_state_snapshot_response(&rt).expect_err("missing path");
		assert!(matches!(error, SaveLoadedStateSnapshotError::PathUnset));
	}

	#[test]
	fn save_loaded_state_snapshot_response_reports_write_failure() {
		let parent_file = temp_dir("write-failed").join("not-a-directory");
		std::fs::write(&parent_file, "occupied").expect("create parent file");
		let path = parent_file.join("state.snapshot.json");
		let rt = runtime_with_snapshot_path(Some(path.clone()));

		let error = save_loaded_state_snapshot_response(&rt).expect_err("write failure");

		match error {
			SaveLoadedStateSnapshotError::WriteFailed {
				path: actual_path,
				error: _,
			} => {
				assert_eq!(actual_path, path);
			}
			SaveLoadedStateSnapshotError::PathUnset => panic!("expected write failure"),
		}
	}

	#[test]
	fn package_lock_preview_response_reports_digest_and_count() {
		let mut rt = runtime_with_snapshot_path(None);
		rt.package_lock_preview_digest = Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into());
		rt.package_lock_preview = vec![PackageLockEntry {
			id: "example.pkg".into(),
			version: Some("1.0.0".into()),
			source_fq: "main".into(),
			source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
			digest: "b3:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".into(),
			dependencies: BTreeMap::new(),
		}];

		let response = package_lock_preview_response(&rt);

		assert_eq!(response.digest, rt.package_lock_preview_digest);
		assert_eq!(response.entry_count, 1);
		assert_eq!(response.entries[0].id, "example.pkg");
	}

	#[test]
	fn save_package_lock_preview_response_writes_lockfile() {
		let root = temp_dir("package-lock-save");
		let mut rt = runtime_with_snapshot_path(None);
		rt.root_dir = root.clone();
		rt.package_lock_preview_digest = Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into());
		rt.package_lock_preview = vec![PackageLockEntry {
			id: "example.pkg".into(),
			version: Some("1.0.0".into()),
			source_fq: "main".into(),
			source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
			digest: "b3:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".into(),
			dependencies: BTreeMap::new(),
		}];

		let response = save_package_lock_preview_response(&rt, None).expect("save package lock");

		assert!(response.path.ends_with("/flowgraph.lock.json"));
		assert_eq!(response.entry_count, 1);
		assert!(response.written);
		let decoded = crate::flowgraph::read_package_lock_file(root.join(crate::flowgraph::FLOWGRAPH_PACKAGE_LOCK_FILE_NAME))
			.expect("read package lock");
		assert_eq!(decoded.entries[0].id, "example.pkg");
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn package_lock_status_response_reports_missing_file() {
		let root = temp_dir("package-lock-missing");
		let mut rt = runtime_with_snapshot_path(None);
		rt.root_dir = root.clone();
		rt.package_lock_preview_digest = Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into());

		let response = package_lock_status_response(&rt);

		assert!(response.path.ends_with("/flowgraph.lock.json"));
		assert!(!response.exists);
		assert_eq!(response.preview_digest, rt.package_lock_preview_digest);
		assert_eq!(response.matches_preview, None);
		assert!(response.file.is_none());
		assert!(response.error.is_none());
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn package_lock_status_response_compares_saved_digest() {
		let root = temp_dir("package-lock-status");
		let mut rt = runtime_with_snapshot_path(None);
		rt.root_dir = root.clone();
		rt.package_lock_preview_digest = Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into());
		rt.package_lock_preview = vec![PackageLockEntry {
			id: "example.pkg".into(),
			version: Some("1.0.0".into()),
			source_fq: "main".into(),
			source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
			digest: "b3:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".into(),
			dependencies: BTreeMap::new(),
		}];
		save_package_lock_preview_response(&rt, None).expect("save package lock");

		let response = package_lock_status_response(&rt);

		assert!(response.exists);
		assert_eq!(response.lock_digest, rt.package_lock_preview_digest);
		assert_eq!(response.matches_preview, Some(true));
		assert_eq!(response.entry_count, Some(1));
		assert_eq!(response.file.as_ref().expect("file").entries[0].id, "example.pkg");

		rt.package_lock_preview_digest = Some("b3:1111111111111111111111111111111111111111111111111111111111111111".into());
		let stale = package_lock_status_response(&rt);
		assert_eq!(stale.matches_preview, Some(false));
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn package_lock_status_response_reports_diff_payload() {
		let root = temp_dir("package-lock-diff");
		let mut rt = runtime_with_snapshot_path(None);
		rt.root_dir = root.clone();
		rt.package_lock_preview_digest = Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into());
		rt.package_lock_preview = vec![
			PackageLockEntry {
				id: "example.changed".into(),
				version: Some("1.0.0".into()),
				source_fq: "changed".into(),
				source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
				digest: "b3:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".into(),
				dependencies: BTreeMap::new(),
			},
			PackageLockEntry {
				id: "example.removed".into(),
				version: Some("1.0.0".into()),
				source_fq: "removed".into(),
				source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
				digest: "b3:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb".into(),
				dependencies: BTreeMap::new(),
			},
			PackageLockEntry {
				id: "example.same".into(),
				version: Some("1.0.0".into()),
				source_fq: "same".into(),
				source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
				digest: "b3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into(),
				dependencies: BTreeMap::new(),
			},
		];
		save_package_lock_preview_response(&rt, None).expect("save package lock");

		rt.package_lock_preview_digest = Some("b3:1111111111111111111111111111111111111111111111111111111111111111".into());
		rt.package_lock_preview = vec![
			PackageLockEntry {
				id: "example.added".into(),
				version: Some("1.0.0".into()),
				source_fq: "added".into(),
				source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
				digest: "b3:dddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddddd".into(),
				dependencies: BTreeMap::new(),
			},
			PackageLockEntry {
				id: "example.changed".into(),
				version: Some("1.0.0".into()),
				source_fq: "changed".into(),
				source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
				digest: "b3:eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".into(),
				dependencies: BTreeMap::new(),
			},
			PackageLockEntry {
				id: "example.same".into(),
				version: Some("1.0.0".into()),
				source_fq: "same".into(),
				source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
				digest: "b3:cccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccccc".into(),
				dependencies: BTreeMap::new(),
			},
		];

		let response = package_lock_status_response(&rt);
		let diff = response.diff.expect("diff");

		assert_eq!(diff.added_ids, vec!["example.added".to_string()]);
		assert_eq!(diff.removed_ids, vec!["example.removed".to_string()]);
		assert_eq!(diff.changed_ids, vec!["example.changed".to_string()]);
		assert_eq!(diff.unchanged_ids, vec!["example.same".to_string()]);
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn save_package_lock_preview_response_honors_if_match() {
		let root = temp_dir("package-lock-if-match");
		let mut rt = runtime_with_snapshot_path(None);
		rt.root_dir = root.clone();
		rt.package_lock_preview_digest = Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef".into());
		rt.package_lock_preview = vec![PackageLockEntry {
			id: "example.pkg".into(),
			version: Some("1.0.0".into()),
			source_fq: "main".into(),
			source_digest: "b3:abcdef0123456789abcdef0123456789abcdef0123456789abcdef0123456789".into(),
			digest: "b3:fedcba9876543210fedcba9876543210fedcba9876543210fedcba9876543210".into(),
			dependencies: BTreeMap::new(),
		}];

		let missing = save_package_lock_preview_response(&rt, Some("b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef"))
			.expect_err("missing lock should conflict");
		assert!(matches!(
			missing,
			SavePackageLockPreviewError::OptimisticLockFailed { current_digest: None }
		));

		save_package_lock_preview_response(&rt, None).expect("initial save");
		save_package_lock_preview_response(&rt, Some("\"b3:0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\""))
			.expect("matching if-match");

		let stale = save_package_lock_preview_response(&rt, Some("b3:1111111111111111111111111111111111111111111111111111111111111111"))
			.expect_err("stale if-match");
		assert!(matches!(
			stale,
			SavePackageLockPreviewError::OptimisticLockFailed { current_digest: Some(_) }
		));
		let bad = save_package_lock_preview_response(&rt, Some("not-a-digest")).expect_err("bad if-match");
		assert!(matches!(bad, SavePackageLockPreviewError::BadIfMatch { .. }));
		let _ = std::fs::remove_dir_all(root);
	}

	#[tokio::test]
	async fn save_live_state_snapshot_response_reports_missing_path() {
		let rt = runtime_with_snapshot_path(None);
		let error = save_live_state_snapshot_response(&rt).await.expect_err("missing path");
		assert!(matches!(error, SaveLiveStateSnapshotError::PathUnset));
	}

	#[tokio::test]
	async fn save_live_state_snapshot_response_reports_missing_worker() {
		let path = temp_dir("live-no-worker").join("profile").join("state.snapshot.json");
		let rt = runtime_with_snapshot_path(Some(path));
		let error = save_live_state_snapshot_response(&rt).await.expect_err("missing worker");
		assert!(matches!(error, SaveLiveStateSnapshotError::WorkerUnavailable));
	}

	#[tokio::test]
	async fn save_live_state_snapshot_response_writes_worker_snapshot_file() {
		let path = temp_dir("live-ok").join("profile").join("state.snapshot.json");
		let mut rt = FlowgraphRuntime::load_and_spawn(&state_counter_root(), std::sync::Weak::new(), None, None, None, None);
		rt.state_snapshot_file_path = Some(path.clone());
		rt.trigger()
			.expect("trigger handle")
			.send_exec("main::in", "__trigger__")
			.expect("send trigger");

		let response = save_live_state_snapshot_response(&rt).await.expect("save live snapshot");

		assert!(response.path.ends_with("/profile/state.snapshot.json"));
		assert_eq!(response.snapshot_node_count, 1);
		assert!(response.written);
		let file = crate::flowgraph::read_state_snapshot_file(&path).expect("read snapshot file");
		assert_eq!(file.snapshot.nodes[0].version, 1);
		assert_eq!(file.snapshot.nodes[0].value, serde_json::json!({ "value": 1 }));
		if let Some(handle) = rt.handle.as_ref() {
			handle.shutdown().await;
		}
	}

	#[test]
	fn state_snapshot_file_status_reports_none_without_path() {
		let rt = runtime_with_snapshot_path(None);
		assert_eq!(state_snapshot_file_status(&rt), None);
	}

	#[test]
	fn state_snapshot_file_status_reports_missing_file() {
		let path = temp_dir("status-missing").join("profile").join("state.snapshot.json");
		let rt = runtime_with_snapshot_path(Some(path));

		let status = state_snapshot_file_status(&rt).expect("status");

		assert!(status.path.ends_with("/profile/state.snapshot.json"));
		assert!(!status.exists);
	}

	#[test]
	fn state_snapshot_file_status_reports_existing_file() {
		let path = temp_dir("status-existing").join("profile").join("state.snapshot.json");
		let rt = runtime_with_snapshot_path(Some(path.clone()));
		rt.write_loaded_state_snapshot_file().expect("write snapshot");

		let status = state_snapshot_file_status(&rt).expect("status");

		assert!(status.path.ends_with("/profile/state.snapshot.json"));
		assert!(status.exists);
	}
}
