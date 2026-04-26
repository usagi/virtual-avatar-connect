//! Vosk モデルディレクトリの解決（ローカルパスまたは alphacephei.com からの初回取得）。

use std::fs::File;
use std::io::{self, BufReader, Write};
use std::path::{Path, PathBuf};

/// 公式モデル一覧: https://alphacephei.com/vosk/models
const ALPHACEPHEI_MODEL_ZIP_BASE: &str = "https://alphacephei.com/vosk/models";

fn vosk_models_cache_root() -> PathBuf {
	dirs::cache_dir()
		.unwrap_or_else(std::env::temp_dir)
		.join("virtual-avatar-connect")
		.join("vosk-models")
}

/// 展開済み Vosk モデルか（`am` などが想定どおりあるか簡易判定）。
fn looks_like_vosk_model_dir(p: &Path) -> bool {
	p.join("am").is_dir()
}

/// `voice_vosk_model_path` の文字列を解決する。
///
/// - 既存ディレクトリならそのまま。
/// - それ以外は **モデル ID**（例: `vosk-model-small-ja-0.22`）として
///   `https://alphacephei.com/vosk/models/{id}.zip` をキャッシュへ取得・展開する。
pub(crate) fn resolve_vosk_model_dir(spec: &str) -> Result<PathBuf, String> {
	let s = spec.trim();
	if s.is_empty() {
		return Err("voice_vosk_model_path が空です".into());
	}

	let path = Path::new(s);
	if path.is_dir() {
		return path.canonicalize().map_err(|e| format!("モデルパスの canonicalize: {e}"));
	}

	let looks_like_path = s.contains('/') || s.contains('\\') || is_windows_drive_path(s);
	if looks_like_path {
		return Err(format!("Vosk モデルディレクトリが見つかりません: {}", s));
	}

	let base = s.trim_end_matches(".zip").trim();
	if base.is_empty() {
		return Err("モデル ID が空です".into());
	}

	let cache_root = vosk_models_cache_root();
	let dest = cache_root.join(base);

	if looks_like_vosk_model_dir(&dest) {
		log::info!("《Voice》: Vosk モデルキャッシュを使用します {:?}", dest);
		return Ok(dest);
	}

	std::fs::create_dir_all(&cache_root).map_err(|e| format!("キャッシュ作成: {e}"))?;

	let url = format!("{}/{}.zip", ALPHACEPHEI_MODEL_ZIP_BASE, base);
	log::info!(
		"《Voice》: Vosk モデルを初回ダウンロードします（数分かかることがあります）\n  {}",
		url
	);

	let zip_path = cache_root.join(format!("{}.zip.part", base));
	if zip_path.exists() {
		let _ = std::fs::remove_file(&zip_path);
	}

	{
		let mut resp = reqwest::blocking::get(&url).map_err(|e| format!("HTTP GET: {e}"))?;
		if !resp.status().is_success() {
			return Err(format!(
				"モデルのダウンロードに失敗: HTTP {} ({})（モデル名を https://alphacephei.com/vosk/models と照合してください）",
				resp.status(),
				url
			));
		}
		let mut file = File::create(&zip_path).map_err(|e| format!("一時 zip: {e}"))?;
		io::copy(&mut resp, &mut file).map_err(|e| format!("ダウンロード書き込み: {e}"))?;
		file.flush().ok();
	}

	let zip_final = cache_root.join(format!("{}.zip", base));
	std::fs::rename(&zip_path, &zip_final).map_err(|e| format!("zip 確定: {e}"))?;

	{
		let file = File::open(&zip_final).map_err(|e| format!("zip オープン: {e}"))?;
		let reader = BufReader::new(file);
		let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("zip 読み取り: {e}"))?;
		archive.extract(&cache_root).map_err(|e| format!("zip 展開: {e}"))?;
	}

	let _ = std::fs::remove_file(&zip_final);

	if !looks_like_vosk_model_dir(&dest) {
		if let Some(found) = find_vosk_model_under(&cache_root, base) {
			log::info!("《Voice》: 展開先を検出しました {:?}", found);
			return Ok(found);
		}
		return Err(format!(
			"展開後もモデルディレクトリを認識できませんでした（想定 {:?}）。zip の内容を確認してください。",
			dest
		));
	}

	log::info!("《Voice》: Vosk モデルのダウンロード・展開が完了しました {:?}", dest);
	Ok(dest)
}

fn find_vosk_model_under(cache_root: &Path, base: &str) -> Option<PathBuf> {
	let direct = cache_root.join(base);
	if looks_like_vosk_model_dir(&direct) {
		return Some(direct);
	}
	let entries = std::fs::read_dir(cache_root).ok()?;
	for e in entries.flatten() {
		let p = e.path();
		if p.is_dir() && looks_like_vosk_model_dir(&p) {
			if p.file_name().map(|n| n.to_string_lossy().contains(base)).unwrap_or(false) {
				return Some(p);
			}
		}
	}
	None
}

fn is_windows_drive_path(s: &str) -> bool {
	let b = s.as_bytes();
	b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn cache_root_join() {
		let p = vosk_models_cache_root();
		assert!(p.to_string_lossy().contains("vosk-models"));
	}
}
