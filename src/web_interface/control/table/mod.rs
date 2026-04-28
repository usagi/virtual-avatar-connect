//! Phase φ-1: Control API の Table CRUD エンドポイント群。
//!
//! spec [`docs/roadmap/phase-phi-control-api-dictionary-editor.md §3 / §5`] に対応。
//!
//! ## 概要
//!
//! 「ディスク上にある 11 カラム辞書 TSV ／汎用 Table TSV ファイル」を GUI から直接編集できるようにする
//! 最小 CRUD API。Flowgraph ノード側は `flowgraph.table.load_tsv` が毎 tick / 都度 reload する実装になっており、
//! ここで書き換えられたファイルは **次の load_tsv 実行タイミングで自然に runtime に反映される**。
//!
//! ## ルート
//!
//! 全て `/api/v1/control/table` scope 配下。
//!
//! - `GET  /tables`                             → allow-list カタログ
//! - `GET  /table/{key}`                        → ファイル読み込み（+ content_hash）
//! - `PUT  /table/{key}`                        → 全行置換（If-Match で楽観ロック）
//! - `POST /table/{key}/entry`                  → 1 行 append
//! - `PATCH /table/{key}/entry/{row_index}`     → 1 行更新
//! - `DELETE /table/{key}/entry/{row_index}`    → 1 行削除
//!
//! ## 安全設計
//!
//! 1. allow-list: `Conf::control_api.tables` に無い key は全て 404。
//! 2. 楽観ロック: mutation 系は `If-Match: b3:<hex>` を任意で受け付け、現在ディスク上ファイルのハッシュと
//!    一致しない場合 409 optimistic_lock_failed。省略時は「盲目上書き」となるが、GUI は常に送る想定。
//! 3. `is_locked = true` の行は PATCH / DELETE を 403 locked で拒否（ファイル由来の組み込み辞書を保護）。
//!    行新規作成（POST）は可能。PUT で全行置換する場合は GUI 側で保護を解除する前提。
//! 4. atomic write: `write_tsv_string` 結果を `<path>.tmp-<random>` に書いてから `fs::rename` で置換する。
//!
//! ## 依存
//!
//! `parse_tsv_with_mode` / `write_tsv_string` を `flowgraph::nodes::table_ops` から再利用し、TSV 解釈の
//! 重複実装を避ける。Table の `content_hash()` / `push_row()` などの API も同じ公開面を使う。
//! ディスク・If-Match 周りの補助は [`util`] モジュール。

mod util;

use actix_web::web::{self, Data, Json};
use actix_web::{delete, get, patch, post, put, HttpRequest, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use serde_json::Value as JsonValue;

use crate::conf::ControlTableQuickAdd;
use crate::flowgraph::table::Table;
use crate::web_interface::control::auth::ControlApiRuntime;

use util::{
	check_if_match, ensure_editable, err_json, find_entry, hex_encode, is_row_locked, map_to_row, read_table, write_table,
};

// ============================================================================
// DTO
// ============================================================================

/// allow-list 内の 1 テーブルを GUI に返すカタログ要素。
#[derive(Debug, Serialize)]
pub struct TableCatalogItem {
	pub key: String,
	pub path: String,
	pub label: Option<String>,
	pub role: Option<String>,
	pub editable: bool,
	pub quick_add: Option<ControlTableQuickAdd>,
	/// ファイルが現在ディスクに存在するか（存在しなくても API は動く／空 Table 扱い）。
	pub exists: bool,
}

#[derive(Debug, Serialize)]
pub struct TableCatalogResponse {
	pub tables: Vec<TableCatalogItem>,
	pub count: usize,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TableFileDto {
	pub key: String,
	pub path: String,
	/// 列名リスト（挿入順）。
	pub columns: Vec<String>,
	pub rows: Vec<TableEntryDto>,
	/// 現在内容の blake3 ハッシュ（`b3:` prefix 無し 64 hex）。`If-Match` に渡す値。
	pub content_hash: String,
	pub editable: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct TableEntryDto {
	pub row_index: usize,
	pub values: serde_json::Map<String, JsonValue>,
}

/// PUT 全行置換の入力。
#[derive(Debug, Deserialize)]
pub struct PutTableRequest {
	/// 列順序を保持するための明示オプション。未指定なら現 Table の schema 列順に従う。
	#[serde(default)]
	pub columns: Option<Vec<String>>,
	pub rows: Vec<serde_json::Map<String, JsonValue>>,
}

/// POST append / PATCH 共通の単行入力。
#[derive(Debug, Deserialize)]
pub struct EntryRequest {
	pub values: serde_json::Map<String, JsonValue>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MutationResponse {
	pub ok: bool,
	pub content_hash: String,
	pub row_count: usize,
	pub affected_row_index: Option<usize>,
}

// ============================================================================
// GET /tables
// ============================================================================

#[get("/tables")]
pub async fn list_tables(runtime: Data<ControlApiRuntime>) -> impl Responder {
	let items: Vec<TableCatalogItem> = runtime
		.tables
		.iter()
		.map(|t| TableCatalogItem {
			key: t.key.clone(),
			path: t.path.display().to_string(),
			label: t.label.clone(),
			role: t.role.clone(),
			editable: t.editable,
			quick_add: t.quick_add.clone(),
			exists: t.path.is_file(),
		})
		.collect();
	HttpResponse::Ok().json(TableCatalogResponse {
		count: items.len(),
		tables: items,
	})
}

// ============================================================================
// GET /table/{key}
// ============================================================================

#[get("/table/{key}")]
pub async fn get_table(runtime: Data<ControlApiRuntime>, key: web::Path<String>) -> impl Responder {
	let key = key.into_inner();
	let entry = match find_entry(&runtime, &key) {
		Ok(e) => e,
		Err(r) => return r,
	};
	let table = match read_table(&entry.path) {
		Ok(t) => t,
		Err(r) => return r,
	};
	let columns: Vec<String> = table.schema().column_names().map(|s| s.to_string()).collect();
	let rows: Vec<TableEntryDto> = table
		.rows()
		.iter()
		.enumerate()
		.map(|(i, r)| TableEntryDto {
			row_index: i,
			values: r.as_map(table.schema()),
		})
		.collect();
	HttpResponse::Ok().json(TableFileDto {
		key: entry.key,
		path: entry.path.display().to_string(),
		columns,
		rows,
		content_hash: hex_encode(table.content_hash()),
		editable: entry.editable,
	})
}

// ============================================================================
// PUT /table/{key}
// ============================================================================

#[put("/table/{key}")]
pub async fn put_table(
	runtime: Data<ControlApiRuntime>,
	req: HttpRequest,
	key: web::Path<String>,
	body: Json<PutTableRequest>,
) -> impl Responder {
	let key = key.into_inner();
	let entry = match find_entry(&runtime, &key) {
		Ok(e) => e,
		Err(r) => return r,
	};
	if let Err(r) = ensure_editable(&entry) {
		return r;
	}
	let current = match read_table(&entry.path) {
		Ok(t) => t,
		Err(r) => return r,
	};
	if let Err(r) = check_if_match(&req, current.content_hash()) {
		return r;
	}
	// 列順は「入力 columns > 現 schema」の順で優先。空 Table のときは columns 必須。
	let schema_basis: Table = match &body.columns {
		Some(cols) if !cols.is_empty() => {
			// 指定列だけの schema を持つ空 Table を作ってベースにする。
			use crate::flowgraph::socket::SocketType;
			use crate::flowgraph::table::{ColumnSpec, TableSchema};
			let specs: Vec<ColumnSpec> = cols
				.iter()
				.map(|n| ColumnSpec::new(n.clone(), SocketType::String).nullable())
				.collect();
			Table::new(TableSchema::new(specs), vec![])
		}
		_ => {
			if current.schema().is_empty() {
				return err_json(
					actix_web::http::StatusCode::BAD_REQUEST,
					"schema_missing",
					"現 Table に schema が無いため、PUT body に columns を明示してください",
				);
			}
			current.clone()
		}
	};
	let mut new_table = schema_basis.clone();
	{
		let inner = new_table.make_mut();
		inner.rows.clear();
		for m in &body.rows {
			let row = map_to_row(&schema_basis, m);
			inner.rows.push(row);
		}
	}
	if let Err(r) = write_table(&entry.path, &new_table) {
		return r;
	}
	HttpResponse::Ok().json(MutationResponse {
		ok: true,
		content_hash: hex_encode(new_table.content_hash()),
		row_count: new_table.len(),
		affected_row_index: None,
	})
}

// ============================================================================
// POST /table/{key}/entry
// ============================================================================

#[post("/table/{key}/entry")]
pub async fn post_entry(
	runtime: Data<ControlApiRuntime>,
	req: HttpRequest,
	key: web::Path<String>,
	body: Json<EntryRequest>,
) -> impl Responder {
	let key = key.into_inner();
	let entry = match find_entry(&runtime, &key) {
		Ok(e) => e,
		Err(r) => return r,
	};
	if let Err(r) = ensure_editable(&entry) {
		return r;
	}
	let mut table = match read_table(&entry.path) {
		Ok(t) => t,
		Err(r) => return r,
	};
	if let Err(r) = check_if_match(&req, table.content_hash()) {
		return r;
	}
	if table.schema().is_empty() {
		return err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"schema_missing",
			"空ファイルへの append は未サポート。先に PUT で schema を確定してください",
		);
	}
	let row = map_to_row(&table, &body.values);
	table.push_row(row);
	let idx = table.len().saturating_sub(1);
	if let Err(r) = write_table(&entry.path, &table) {
		return r;
	}
	HttpResponse::Ok().json(MutationResponse {
		ok: true,
		content_hash: hex_encode(table.content_hash()),
		row_count: table.len(),
		affected_row_index: Some(idx),
	})
}

// ============================================================================
// PATCH /table/{key}/entry/{row_index}
// ============================================================================

#[patch("/table/{key}/entry/{row_index}")]
pub async fn patch_entry(
	runtime: Data<ControlApiRuntime>,
	req: HttpRequest,
	params: web::Path<(String, usize)>,
	body: Json<EntryRequest>,
) -> impl Responder {
	let (key, row_index) = params.into_inner();
	let entry = match find_entry(&runtime, &key) {
		Ok(e) => e,
		Err(r) => return r,
	};
	if let Err(r) = ensure_editable(&entry) {
		return r;
	}
	let mut table = match read_table(&entry.path) {
		Ok(t) => t,
		Err(r) => return r,
	};
	if let Err(r) = check_if_match(&req, table.content_hash()) {
		return r;
	}
	if row_index >= table.len() {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"row_not_found",
			format!("row_index={row_index} は範囲外（現在 {} 行）", table.len()),
		);
	}
	let current_row = table.rows()[row_index].clone();
	if is_row_locked(&table, &current_row) {
		return err_json(
			actix_web::http::StatusCode::FORBIDDEN,
			"locked",
			format!("row_index={row_index} は is_locked = true のため編集できません"),
		);
	}
	// merge: current row → map → 上書き → row
	let mut merged = current_row.as_map(table.schema());
	for (k, v) in &body.values {
		merged.insert(k.clone(), v.clone());
	}
	let new_row = map_to_row(&table, &merged);
	{
		let inner = table.make_mut();
		inner.rows[row_index] = new_row;
	}
	if let Err(r) = write_table(&entry.path, &table) {
		return r;
	}
	HttpResponse::Ok().json(MutationResponse {
		ok: true,
		content_hash: hex_encode(table.content_hash()),
		row_count: table.len(),
		affected_row_index: Some(row_index),
	})
}

// ============================================================================
// DELETE /table/{key}/entry/{row_index}
// ============================================================================

#[delete("/table/{key}/entry/{row_index}")]
pub async fn delete_entry(runtime: Data<ControlApiRuntime>, req: HttpRequest, params: web::Path<(String, usize)>) -> impl Responder {
	let (key, row_index) = params.into_inner();
	let entry = match find_entry(&runtime, &key) {
		Ok(e) => e,
		Err(r) => return r,
	};
	if let Err(r) = ensure_editable(&entry) {
		return r;
	}
	let mut table = match read_table(&entry.path) {
		Ok(t) => t,
		Err(r) => return r,
	};
	if let Err(r) = check_if_match(&req, table.content_hash()) {
		return r;
	}
	if row_index >= table.len() {
		return err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"row_not_found",
			format!("row_index={row_index} は範囲外（現在 {} 行）", table.len()),
		);
	}
	let current_row = table.rows()[row_index].clone();
	if is_row_locked(&table, &current_row) {
		return err_json(
			actix_web::http::StatusCode::FORBIDDEN,
			"locked",
			format!("row_index={row_index} は is_locked = true のため削除できません"),
		);
	}
	{
		let inner = table.make_mut();
		inner.rows.remove(row_index);
	}
	if let Err(r) = write_table(&entry.path, &table) {
		return r;
	}
	HttpResponse::Ok().json(MutationResponse {
		ok: true,
		content_hash: hex_encode(table.content_hash()),
		row_count: table.len(),
		affected_row_index: Some(row_index),
	})
}

// ============================================================================
// configure
// ============================================================================

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(list_tables)
		.service(get_table)
		.service(put_table)
		.service(post_entry)
		.service(patch_entry)
		.service(delete_entry);
}

// ============================================================================
// tests
// ============================================================================

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;

	use crate::conf::ControlTableEntry;
	use crate::web_interface::control::auth::TokenSource;
	use actix_web::{http::StatusCode, test, App};
	use std::path::{Path, PathBuf};
	use std::sync::atomic::{AtomicU64, Ordering};
	use std::time::{SystemTime, UNIX_EPOCH};

	/// Drop 時に再帰削除する軽量 tmp dir。tempfile crate を足さないための自前実装。
	pub struct TempDir(PathBuf);
	impl TempDir {
		pub fn path(&self) -> &Path {
			&self.0
		}
	}
	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}
	fn mk_tmp_dir() -> TempDir {
		static SEQ: AtomicU64 = AtomicU64::new(0);
		let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
		let seq = SEQ.fetch_add(1, Ordering::Relaxed);
		let p = std::env::temp_dir().join(format!("vac-ctrl-table-{}-{ns}-{seq}", std::process::id()));
		fs::create_dir_all(&p).unwrap();
		TempDir(p)
	}

	/// テスト用の `ControlApiRuntime` を組み立て、tmp dir に TSV ファイルを置く。
	fn setup_runtime(initial_tsv: &str) -> (ControlApiRuntime, TempDir, PathBuf) {
		let tmp = mk_tmp_dir();
		let tsv_path = tmp.path().join("sample.tsv");
		if !initial_tsv.is_empty() {
			fs::write(&tsv_path, initial_tsv).expect("write tsv");
		}
		let runtime = ControlApiRuntime {
			token: "test-token".into(),
			token_source: TokenSource::Generated,
			require_token_for_loopback: false,
			require_token_for_non_loopback: false,
			written_token_file: None,
			tables: vec![
				ControlTableEntry {
					key: "sample".into(),
					path: tsv_path.clone(),
					label: Some("Sample".into()),
					role: Some("glossary".into()),
					editable: true,
					quick_add: None,
				},
				ControlTableEntry {
					key: "readonly".into(),
					path: tsv_path.clone(),
					label: None,
					role: None,
					editable: false,
					quick_add: None,
				},
			],
		};
		(runtime, tmp, tsv_path)
	}

	fn sample_headerful() -> &'static str {
		concat!(
			"source\treplacement\tkind\tpriority\tis_locked\tenabled\tby\tcreated_at\texpires_at\ttags\tnote\n",
			"hello\thi\tliteral\t0\tfalse\ttrue\ttest\t2026-01-01T00:00:00Z\t\t\t\n",
			"locked\tspecial\tliteral\t0\ttrue\ttrue\tsystem\t2026-01-01T00:00:00Z\t\t\t\n"
		)
	}

	#[actix_web::test]
	async fn list_tables_returns_catalog() {
		let (runtime, _tmp, _path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(list_tables)).await;
		let req = test::TestRequest::get().uri("/tables").to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::OK);
		let body: serde_json::Value = test::read_body_json(resp).await;
		assert_eq!(body["count"], 2);
		assert_eq!(body["tables"][0]["key"], "sample");
		assert_eq!(body["tables"][0]["exists"], true);
	}

	#[actix_web::test]
	async fn get_table_returns_rows_and_hash() {
		let (runtime, _tmp, _path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(get_table)).await;
		let req = test::TestRequest::get().uri("/table/sample").to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::OK);
		let body: TableFileDto = test::read_body_json(resp).await;
		assert_eq!(body.key, "sample");
		assert_eq!(body.columns.len(), 11);
		assert_eq!(body.rows.len(), 2);
		assert_eq!(body.rows[0].values.get("source").and_then(|v| v.as_str()), Some("hello"));
		assert_eq!(body.content_hash.len(), 64);
	}

	#[actix_web::test]
	async fn get_table_unknown_key_is_404() {
		let (runtime, _tmp, _path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(get_table)).await;
		let req = test::TestRequest::get().uri("/table/unknown").to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::NOT_FOUND);
	}

	#[actix_web::test]
	async fn put_with_if_match_success_and_conflict() {
		let (runtime, _tmp, path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(get_table).service(put_table)).await;
		// 現 hash を取得
		let resp = test::call_service(&app, test::TestRequest::get().uri("/table/sample").to_request()).await;
		let dto: TableFileDto = test::read_body_json(resp).await;
		let hash = dto.content_hash.clone();

		// 1 行だけの PUT
		let body = serde_json::json!({
			"rows": [
				{"source": "foo", "replacement": "bar", "kind": "literal", "priority": 0,
				 "is_locked": false, "enabled": true, "by": "user", "created_at": "2026-04-23T00:00:00Z",
				 "expires_at": null, "tags": "", "note": ""}
			]
		});
		let req = test::TestRequest::put()
			.uri("/table/sample")
			.insert_header(("If-Match", format!("b3:{hash}")))
			.set_json(&body)
			.to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::OK);
		let res: MutationResponse = test::read_body_json(resp).await;
		assert!(res.ok);
		assert_eq!(res.row_count, 1);
		assert_ne!(res.content_hash, hash); // 変わったはず

		// 旧 hash で再度 PUT → 409
		let req = test::TestRequest::put()
			.uri("/table/sample")
			.insert_header(("If-Match", format!("b3:{hash}")))
			.set_json(&body)
			.to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::CONFLICT);

		// ディスクに書いた内容を目視確認
		let written = std::fs::read_to_string(&path).expect("read back");
		assert!(written.contains("foo\tbar\t"));
	}

	#[actix_web::test]
	async fn post_entry_appends_row() {
		let (runtime, _tmp, _path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(post_entry)).await;
		let body = serde_json::json!({
			"values": {
				"source": "new", "replacement": "shiny", "kind": "literal", "priority": 5,
				"is_locked": false, "enabled": true, "by": "gui", "created_at": "2026-04-23T01:00:00Z",
				"expires_at": null, "tags": "test", "note": "phi-1"
			}
		});
		let req = test::TestRequest::post().uri("/table/sample/entry").set_json(&body).to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::OK);
		let res: MutationResponse = test::read_body_json(resp).await;
		assert_eq!(res.row_count, 3);
		assert_eq!(res.affected_row_index, Some(2));
	}

	#[actix_web::test]
	async fn delete_locked_row_is_403() {
		let (runtime, _tmp, _path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(delete_entry)).await;
		// row_index = 1 の行は is_locked = true
		let req = test::TestRequest::delete().uri("/table/sample/entry/1").to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::FORBIDDEN);
	}

	#[actix_web::test]
	async fn readonly_table_rejects_mutation() {
		let (runtime, _tmp, _path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(post_entry)).await;
		let body = serde_json::json!({"values": {"source": "x", "replacement": "y"}});
		let req = test::TestRequest::post().uri("/table/readonly/entry").set_json(&body).to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::FORBIDDEN);
	}

	#[actix_web::test]
	async fn patch_unlocked_row_updates_value() {
		let (runtime, _tmp, _path) = setup_runtime(sample_headerful());
		let app = test::init_service(App::new().app_data(Data::new(runtime)).service(patch_entry).service(get_table)).await;
		let body = serde_json::json!({"values": {"replacement": "HOWDY"}});
		let req = test::TestRequest::patch().uri("/table/sample/entry/0").set_json(&body).to_request();
		let resp = test::call_service(&app, req).await;
		assert_eq!(resp.status(), StatusCode::OK);

		let resp = test::call_service(&app, test::TestRequest::get().uri("/table/sample").to_request()).await;
		let dto: TableFileDto = test::read_body_json(resp).await;
		assert_eq!(dto.rows[0].values.get("replacement").and_then(|v| v.as_str()), Some("HOWDY"));
	}
}
