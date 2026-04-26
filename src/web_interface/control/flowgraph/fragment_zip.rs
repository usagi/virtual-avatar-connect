//! Fragment copy/paste と ZIP import/export（Phase δ-7c / δ-7d）。

use actix_web::web::{self, Data, Json};
use actix_web::{post, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::flowgraph::fragment::paste::{paste_fragment, PasteError, PasteReport, PasteRequest};
use crate::flowgraph::fragment::zip_codec::{export_zip, import_zip, ZipExportError, ZipImportError, ZipImportOptions, ZipImportOutcome};
use crate::flowgraph::fragment::{copy_targets, serialize_fragment, CopyError, CopyRequest, Fragment};
use crate::SharedState;

use super::reload::reload_runtime;
use super::util::{err_json, flowgraph_dir};

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
			return err_json(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "serialize_failed", e);
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
pub async fn post_fragment_paste(state: Data<SharedState>, req: Json<PasteRequest>) -> impl Responder {
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
		PasteError::RootMissing(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "flowgraph_dir_missing", e),
		PasteError::Parse(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "fragment_parse", e),
		PasteError::InvalidTarget(_) => err_json(StatusCode::BAD_REQUEST, "invalid_target", e),
		PasteError::InvalidFragmentPath(_) => err_json(StatusCode::BAD_REQUEST, "invalid_fragment_path", e),
		PasteError::Io { .. } => err_json(StatusCode::INTERNAL_SERVER_ERROR, "io", e),
		PasteError::ExistingParse { .. } => err_json(StatusCode::CONFLICT, "existing_parse_failed", e),
	}
}

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
pub async fn post_import_zip(state: Data<SharedState>, query: web::Query<ImportZipQuery>, body: web::Bytes) -> impl Responder {
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
		ZipImportError::ManifestMissing => err_json(StatusCode::UNPROCESSABLE_ENTITY, "manifest_missing", e),
		ZipImportError::ManifestParse(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "manifest_parse", e),
		ZipImportError::Zip(_) => err_json(StatusCode::UNPROCESSABLE_ENTITY, "zip_parse", e),
		ZipImportError::Io(_) => err_json(StatusCode::INTERNAL_SERVER_ERROR, "io", e),
		ZipImportError::Paste(p) => paste_error_to_response(p),
	}
}
