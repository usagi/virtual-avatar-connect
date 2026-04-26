//! プロファイル API のパス解決・basename 検証・バックアップ名生成。

use std::path::{Path, PathBuf};

use actix_web::HttpResponse;

use crate::SharedState;

pub(crate) fn err_response(status: actix_web::http::StatusCode, code: &str, detail: impl std::fmt::Display) -> HttpResponse {
	HttpResponse::build(status).json(serde_json::json!({
	 "error": code,
	 "detail": detail.to_string(),
	}))
}

/// `state.conf_source_path` の親ディレクトリを取得。未設定なら 500 相当の Err を返す。
pub(crate) async fn profiles_dir(state: &SharedState) -> Result<(PathBuf, PathBuf), HttpResponse> {
	let Some(current) = state.read().await.conf_source_path.clone() else {
		return Err(err_response(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"conf_source_path_unset",
			"conf ファイルの出自が記録されていないので、プロファイル操作はできません（特殊モード起動？）。",
		));
	};
	let Some(parent) = current.parent() else {
		return Err(err_response(
			actix_web::http::StatusCode::INTERNAL_SERVER_ERROR,
			"no_parent_dir",
			"conf の親ディレクトリが取得できません。",
		));
	};
	Ok((parent.to_path_buf(), current))
}

/// `filename` を厳密な basename として検証したうえで、profiles_dir 内のパスを組み立てる。
/// - path separator（`/` や `\`）を含まない
/// - `..` / `.` でない
/// - 拡張子 `.toml`（case-insensitive）
///
/// canonicalize は「ファイルが既に存在する」ケースのみ実施。存在しない場合は「親ディレクトリが
/// canonicalize 可能 + basename を付けたパス」を返す。
pub(crate) fn safe_basename(dir: &Path, filename: &str) -> Result<PathBuf, HttpResponse> {
	// 前方防御
	let trimmed = filename.trim();
	if trimmed.is_empty() {
		return Err(err_response(
			actix_web::http::StatusCode::BAD_REQUEST,
			"empty_filename",
			"ファイル名が空です。",
		));
	}
	if trimmed.contains('/') || trimmed.contains('\\') {
		return Err(err_response(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_filename",
			format!("ファイル名にパス区切りを含められません: {trimmed:?}"),
		));
	}
	if trimmed == "." || trimmed == ".." || trimmed.starts_with("..") {
		return Err(err_response(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_filename",
			format!("ファイル名に `.` や `..` は使えません: {trimmed:?}"),
		));
	}
	let ext_ok = Path::new(trimmed)
		.extension()
		.and_then(|e| e.to_str())
		.map(|e| e.eq_ignore_ascii_case("toml"))
		.unwrap_or(false);
	if !ext_ok {
		return Err(err_response(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_extension",
			format!("拡張子は `.toml` のみです: {trimmed:?}"),
		));
	}
	Ok(dir.join(trimmed))
}

/// バックアップファイル名を組み立てる。`conf.toml` → `conf.toml.bak-20260417-123456` 形式。
pub(crate) fn backup_path(original: &Path) -> PathBuf {
	let ts = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
	let orig_name = original.file_name().and_then(|s| s.to_str()).unwrap_or("conf.toml");
	let bak_name = format!("{}.bak-{}", orig_name, ts);
	original.with_file_name(bak_name)
}

pub(crate) fn is_current(path: &Path, current: &Path) -> bool {
	match (std::fs::canonicalize(path), std::fs::canonicalize(current)) {
		(Ok(a), Ok(b)) => a == b,
		_ => path == current,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn safe_basename_accepts_plain_toml() {
		let dir = std::env::temp_dir();
		let p = safe_basename(&dir, "conf.toml").expect("should accept");
		assert_eq!(p.parent().unwrap(), dir.as_path());
		assert_eq!(p.file_name().unwrap().to_string_lossy(), "conf.toml");
	}

	#[test]
	fn safe_basename_rejects_separators() {
		let dir = std::env::temp_dir();
		assert!(safe_basename(&dir, "sub/conf.toml").is_err());
		assert!(safe_basename(&dir, "sub\\conf.toml").is_err());
		assert!(safe_basename(&dir, "../conf.toml").is_err());
	}

	#[test]
	fn safe_basename_rejects_wrong_extension() {
		let dir = std::env::temp_dir();
		assert!(safe_basename(&dir, "conf.tom").is_err());
		assert!(safe_basename(&dir, "conf").is_err());
		assert!(safe_basename(&dir, "").is_err());
	}

	#[test]
	fn safe_basename_accepts_uppercase_extension() {
		let dir = std::env::temp_dir();
		assert!(safe_basename(&dir, "conf.TOML").is_ok());
	}

	#[test]
	fn backup_path_appends_suffix() {
		let p = Path::new("/tmp/conf.toml");
		let bak = backup_path(p);
		let name = bak.file_name().unwrap().to_string_lossy().to_string();
		assert!(name.starts_with("conf.toml.bak-"), "unexpected: {name}");
	}
}
