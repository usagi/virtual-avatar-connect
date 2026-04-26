//! Table Control API の共通ヘルパ（エラー JSON、allow-list、If-Match、TSV I/O、行ロック）。

use std::fs;
use std::io::Write;
use std::path::Path;

use actix_web::{HttpRequest, HttpResponse};
use serde_json::Value as JsonValue;

use crate::conf::ControlTableEntry;
use crate::flowgraph::nodes::table_ops::{parse_tsv_with_mode, write_tsv_string};
use crate::flowgraph::table::{Row, Table};
use crate::web_interface::control::auth::ControlApiRuntime;

pub(crate) fn err_json(status: actix_web::http::StatusCode, code: &str, detail: impl std::fmt::Display) -> HttpResponse {
	HttpResponse::build(status).json(serde_json::json!({
		"error": code,
		"detail": detail.to_string(),
	}))
}

/// allow-list から `key` に一致するエントリを探して返す。見つからなければ 404 HttpResponse。
pub(crate) fn find_entry(runtime: &ControlApiRuntime, key: &str) -> Result<ControlTableEntry, HttpResponse> {
	match runtime.tables.iter().find(|t| t.key == key) {
		Some(e) => Ok(e.clone()),
		None => Err(err_json(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("allow-list に登録されていない table key です: '{key}'"),
		)),
	}
}

/// `If-Match` header を読み取り、`current_hash` と比較する。header 未指定は許容（盲目上書き）。
///
/// 形式は `b3:<64 hex>`。Prefix 無しでも 64 hex なら許容する。
pub(crate) fn check_if_match(req: &HttpRequest, current_hash: &[u8; 32]) -> Result<(), HttpResponse> {
	let Some(hv) = req.headers().get("If-Match") else {
		return Ok(());
	};
	let Ok(raw) = hv.to_str() else {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"bad_if_match",
			"If-Match ヘッダが ASCII ではありません",
		));
	};
	let hex_part = raw.trim().trim_start_matches("b3:").trim_matches('"');
	if hex_part.len() != 64 {
		return Err(err_json(
			actix_web::http::StatusCode::BAD_REQUEST,
			"bad_if_match",
			format!("If-Match は `b3:<64 hex>` 形式です。got='{raw}'"),
		));
	}
	let current_hex = hex_encode(current_hash);
	if !hex_part.eq_ignore_ascii_case(&current_hex) {
		return Err(err_json(
			actix_web::http::StatusCode::CONFLICT,
			"optimistic_lock_failed",
			format!("If-Match 不一致: 現在の content_hash は b3:{current_hex}"),
		));
	}
	Ok(())
}

pub(crate) fn hex_encode(bytes: &[u8; 32]) -> String {
	const HEX: &[u8; 16] = b"0123456789abcdef";
	let mut s = String::with_capacity(64);
	for b in bytes {
		s.push(HEX[(*b >> 4) as usize] as char);
		s.push(HEX[(*b & 0x0f) as usize] as char);
	}
	s
}

/// ディスクから TSV を読んで Table にパース。ファイル未存在は空 Table として扱う。
pub(crate) fn read_table(path: &Path) -> Result<Table, HttpResponse> {
	let contents = match fs::read_to_string(path) {
		Ok(s) => s,
		Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
		Err(e) => {
			return Err(err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"read_failed",
				format!("読み込み失敗: {} ({e})", path.display()),
			));
		}
	};
	if contents.trim().is_empty() {
		return Ok(Table::empty());
	}
	parse_tsv_with_mode(&contents, "auto").map_err(|e| {
		err_json(
			actix_web::http::StatusCode::UNPROCESSABLE_ENTITY,
			"parse_failed",
			format!("TSV パース失敗: {e}"),
		)
	})
}

/// Table を atomic rename で書き戻す。
pub(crate) fn write_table(path: &Path, table: &Table) -> Result<(), HttpResponse> {
	let parent = path.parent().unwrap_or_else(|| Path::new("."));
	if !parent.as_os_str().is_empty() && !parent.exists() {
		if let Err(e) = fs::create_dir_all(parent) {
			return Err(err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"io_failed",
				format!("親ディレクトリ作成失敗: {} ({e})", parent.display()),
			));
		}
	}
	let contents = write_tsv_string(table);
	// ランダム tmp 名（PID + ナノ秒）で衝突を避ける。
	let fname = path.file_name().and_then(|s| s.to_str()).unwrap_or("table.tsv");
	let stamp = std::time::SystemTime::now()
		.duration_since(std::time::UNIX_EPOCH)
		.map(|d| d.as_nanos())
		.unwrap_or(0);
	let tmp_path = parent.join(format!(".{fname}.tmp-{}-{stamp}", std::process::id()));
	match fs::File::create(&tmp_path).and_then(|mut f| f.write_all(contents.as_bytes()).and_then(|_| f.sync_all())) {
		Ok(_) => {}
		Err(e) => {
			let _ = fs::remove_file(&tmp_path);
			return Err(err_json(
				actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
				"write_failed",
				format!("tmp 書き込み失敗: {} ({e})", tmp_path.display()),
			));
		}
	}
	if let Err(e) = fs::rename(&tmp_path, path) {
		let _ = fs::remove_file(&tmp_path);
		return Err(err_json(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"rename_failed",
			format!("atomic rename 失敗: {} → {} ({e})", tmp_path.display(), path.display()),
		));
	}
	Ok(())
}

/// JSON Map を schema 順の `Row` に変換する。未指定カラムは `null` で埋める。
pub(crate) fn map_to_row(table: &Table, map: &serde_json::Map<String, JsonValue>) -> Row {
	let values: Vec<JsonValue> = table
		.schema()
		.column_names()
		.map(|n| map.get(n).cloned().unwrap_or(JsonValue::Null))
		.collect();
	Row::new(values)
}

/// `is_locked` カラムが `true` ならロック済み。列が無い Table では常に `false`。
pub(crate) fn is_row_locked(table: &Table, row: &Row) -> bool {
	let Some(idx) = table.schema().column_index("is_locked") else {
		return false;
	};
	row.get(idx).and_then(|v| v.as_bool()).unwrap_or(false)
}

pub(crate) fn ensure_editable(entry: &ControlTableEntry) -> Result<(), HttpResponse> {
	if !entry.editable {
		return Err(err_json(
			actix_web::http::StatusCode::FORBIDDEN,
			"read_only",
			format!("table '{}' は editable = false で登録されています", entry.key),
		));
	}
	Ok(())
}
