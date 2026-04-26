//! Phase VI-γ-4a: プロファイル（conf ファイル）操作 API。
//!
//! 既存の `GET /profiles`（[restart.rs]）に対し、「複製」「リネーム」「削除」「内容取得」「内容更新」を追加する。
//!
//! ## 安全設計
//!
//! 全操作は **現 `conf_source_path` の親ディレクトリ内に限定**（chroot 的な意味で）。外部ディレクトリに
//! 出られないよう、入力はファイル名（basename）のみ受け付け、canonicalize 後に親ディレクトリの
//! 部分でなければ 400。拡張子は `.toml` に限定。
//!
//! 破壊的操作（rename/delete/put_content）は **`.bak-YYYYMMDDHHmmSS` バックアップ**を自動生成する。
//! 削除 → バックアップ、上書き → 旧を .bak に移してから書き込み、リネーム → 旧名を .bak にして新名に書き込み。
//!
//! `is_current` なファイルの **削除・リネームは拒否**（現プロセスが参照しているファイルの扱いを単純化するため）。
//! 内容更新は `is_current` でも許すが、クライアントに「再起動しないと反映されない」注意を返す責務がある。
//! 書き込み前に `toml::from_str::<Conf>()` で**妥当性検証**を行い、パース失敗時は 400（ファイルは触らない）。
//!
//! パス検証・バックアップ名は [`util`]。

mod util;

use actix_web::web::{self, Data, Json};
use actix_web::{delete, get, post, put, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

use crate::conf::Conf;
use crate::SharedState;

use util::{backup_path, err_response, is_current, profiles_dir, safe_basename};

// ---- DTO ---------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub struct CloneRequest {
	/// 複製元のファイル名（basename、拡張子込み）。
	pub source: String,
	/// 新ファイル名（basename、拡張子込み）。既存と衝突する場合は 409。
	pub new_filename: String,
}

#[derive(Debug, Deserialize)]
pub struct RenameRequest {
	pub new_filename: String,
}

#[derive(Debug, Deserialize)]
pub struct PutContentRequest {
	/// 新しいファイル全体の内容（UTF-8 TOML）。サーバ側で `Conf` としてパースして通らなければ 400。
	pub content: String,
}

#[derive(Debug, Serialize)]
pub struct ProfileContentResponse {
	pub filename: String,
	pub path: String,
	pub size: u64,
	pub content: String,
	pub is_current: bool,
}

#[derive(Debug, Serialize)]
pub struct ProfileOpResponse {
	pub filename: String,
	pub path: String,
	/// 生成されたバックアップファイル名（あれば）。UI が「バックアップを作成しました」を出せるよう返す。
	pub backup: Option<String>,
	/// 付随する警告（例: 「内容を更新しましたが再起動しないと反映されません」）。
	#[serde(skip_serializing_if = "Option::is_none")]
	pub warning: Option<String>,
}

// ---- エンドポイント実装 ------------------------------------------------------

#[get("/profiles/{filename}/content")]
pub async fn get_profile_content(state: Data<SharedState>, filename: web::Path<String>) -> impl Responder {
	let filename = filename.into_inner();
	let (dir, current) = match profiles_dir(&state).await {
		Ok(x) => x,
		Err(r) => return r,
	};
	let path = match safe_basename(&dir, &filename) {
		Ok(p) => p,
		Err(r) => return r,
	};
	if !path.exists() {
		return err_response(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("{} が存在しません。", path.display()),
		);
	}
	let content = match std::fs::read_to_string(&path) {
		Ok(c) => c,
		Err(e) => return err_response(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "read_failed", e),
	};
	let size = std::fs::metadata(&path).ok().map(|m| m.len()).unwrap_or(0);
	HttpResponse::Ok().json(ProfileContentResponse {
		filename,
		path: path.display().to_string(),
		size,
		content,
		is_current: is_current(&path, &current),
	})
}

#[put("/profiles/{filename}/content")]
pub async fn put_profile_content(state: Data<SharedState>, filename: web::Path<String>, body: Json<PutContentRequest>) -> impl Responder {
	let filename = filename.into_inner();
	let (dir, current) = match profiles_dir(&state).await {
		Ok(x) => x,
		Err(r) => return r,
	};
	let path = match safe_basename(&dir, &filename) {
		Ok(p) => p,
		Err(r) => return r,
	};

	// パース検証（副作用なし）。失敗時は書き込まない。
	if let Err(e) = toml::from_str::<Conf>(&body.content) {
		return err_response(
			actix_web::http::StatusCode::BAD_REQUEST,
			"invalid_toml",
			format!("conf として不正です: {e}"),
		);
	}

	// 既存ファイルのバックアップ（新規作成でなければ）
	let backup = if path.exists() {
		let bak = backup_path(&path);
		if let Err(e) = std::fs::copy(&path, &bak) {
			log::warn!("《ProfileOps》 バックアップに失敗: {} → {}: {e}", path.display(), bak.display());
		}
		Some(bak)
	} else {
		None
	};

	if let Err(e) = std::fs::write(&path, body.content.as_bytes()) {
		return err_response(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "write_failed", e);
	}

	let warning = if is_current(&path, &current) {
		Some("このファイルは現在読み込まれているプロファイルです。再起動しないと反映されません。".to_string())
	} else {
		None
	};

	log::info!("《ProfileOps》 {} を更新しました。", path.display());
	HttpResponse::Ok().json(ProfileOpResponse {
		filename,
		path: path.display().to_string(),
		backup: backup.map(|b| b.file_name().unwrap_or_default().to_string_lossy().to_string()),
		warning,
	})
}

#[post("/profiles/clone")]
pub async fn post_clone(state: Data<SharedState>, body: Json<CloneRequest>) -> impl Responder {
	let (dir, _current) = match profiles_dir(&state).await {
		Ok(x) => x,
		Err(r) => return r,
	};
	let src = match safe_basename(&dir, &body.source) {
		Ok(p) => p,
		Err(r) => return r,
	};
	let dst = match safe_basename(&dir, &body.new_filename) {
		Ok(p) => p,
		Err(r) => return r,
	};
	if !src.exists() {
		return err_response(
			actix_web::http::StatusCode::NOT_FOUND,
			"source_not_found",
			format!("複製元が存在しません: {}", src.display()),
		);
	}
	if dst.exists() {
		return err_response(
			actix_web::http::StatusCode::CONFLICT,
			"destination_exists",
			format!("複製先が既に存在します: {}", dst.display()),
		);
	}
	if let Err(e) = std::fs::copy(&src, &dst) {
		return err_response(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "copy_failed", e);
	}
	log::info!("《ProfileOps》 {} → {} を複製しました。", src.display(), dst.display());
	HttpResponse::Ok().json(ProfileOpResponse {
		filename: body.new_filename.clone(),
		path: dst.display().to_string(),
		backup: None,
		warning: None,
	})
}

#[post("/profiles/{filename}/rename")]
pub async fn post_rename(state: Data<SharedState>, filename: web::Path<String>, body: Json<RenameRequest>) -> impl Responder {
	let filename = filename.into_inner();
	let (dir, current) = match profiles_dir(&state).await {
		Ok(x) => x,
		Err(r) => return r,
	};
	let src = match safe_basename(&dir, &filename) {
		Ok(p) => p,
		Err(r) => return r,
	};
	let dst = match safe_basename(&dir, &body.new_filename) {
		Ok(p) => p,
		Err(r) => return r,
	};
	if !src.exists() {
		return err_response(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("{} が存在しません。", src.display()),
		);
	}
	if is_current(&src, &current) {
		return err_response(
			actix_web::http::StatusCode::CONFLICT,
			"cannot_rename_current",
			"現在読み込み中のプロファイルはリネームできません。先に別プロファイルへ切り替えてください。",
		);
	}
	if dst.exists() {
		return err_response(
			actix_web::http::StatusCode::CONFLICT,
			"destination_exists",
			format!("リネーム先が既に存在します: {}", dst.display()),
		);
	}
	if let Err(e) = std::fs::rename(&src, &dst) {
		return err_response(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "rename_failed", e);
	}
	log::info!("《ProfileOps》 {} → {} にリネームしました。", src.display(), dst.display());
	HttpResponse::Ok().json(ProfileOpResponse {
		filename: body.new_filename.clone(),
		path: dst.display().to_string(),
		backup: None,
		warning: None,
	})
}

#[delete("/profiles/{filename}")]
pub async fn delete_profile(state: Data<SharedState>, filename: web::Path<String>) -> impl Responder {
	let filename = filename.into_inner();
	let (dir, current) = match profiles_dir(&state).await {
		Ok(x) => x,
		Err(r) => return r,
	};
	let path = match safe_basename(&dir, &filename) {
		Ok(p) => p,
		Err(r) => return r,
	};
	if !path.exists() {
		return err_response(
			actix_web::http::StatusCode::NOT_FOUND,
			"not_found",
			format!("{} が存在しません。", path.display()),
		);
	}
	if is_current(&path, &current) {
		return err_response(
			actix_web::http::StatusCode::CONFLICT,
			"cannot_delete_current",
			"現在読み込み中のプロファイルは削除できません。先に別プロファイルへ切り替えてください。",
		);
	}
	// 削除前にバックアップ（削除したいとは言えユーザが間違えるケースはありうる）。
	let bak = backup_path(&path);
	if let Err(e) = std::fs::rename(&path, &bak) {
		return err_response(actix_web::http::StatusCode::INTERNAL_SERVER_ERROR, "delete_failed", e);
	}
	log::info!(
		"《ProfileOps》 {} を {} にバックアップしました（削除扱い）。",
		path.display(),
		bak.display()
	);
	HttpResponse::Ok().json(ProfileOpResponse {
		filename,
		path: path.display().to_string(),
		backup: Some(bak.file_name().unwrap_or_default().to_string_lossy().to_string()),
		warning: None,
	})
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(get_profile_content)
		.service(put_profile_content)
		.service(post_clone)
		.service(post_rename)
		.service(delete_profile);
}
