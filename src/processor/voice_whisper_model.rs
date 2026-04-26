//! Whisper GGML のパス解決（ローカルファイルまたは Hugging Face からの初回ダウンロード）。

use std::io::Write;
use std::path::{Path, PathBuf};

/// ggerganov/whisper.cpp（Hugging Face）の `resolve/main` 直リンク。
const HF_RESOLVE_BASE: &str = "https://huggingface.co/ggerganov/whisper.cpp/resolve/main";

fn cache_dir() -> PathBuf {
	dirs::cache_dir()
		.unwrap_or_else(std::env::temp_dir)
		.join("virtual-avatar-connect")
		.join("whisper")
}

fn preset_to_filename(preset: &str) -> Option<&'static str> {
	let p = preset.trim();
	if p.is_empty() {
		return None;
	}
	if p.eq_ignore_ascii_case("tiny") {
		return Some("ggml-tiny.bin");
	}
	if p.eq_ignore_ascii_case("base") {
		return Some("ggml-base.bin");
	}
	if p.eq_ignore_ascii_case("small") {
		return Some("ggml-small.bin");
	}
	if p.eq_ignore_ascii_case("medium") {
		return Some("ggml-medium.bin");
	}
	if p.eq_ignore_ascii_case("large-v2") || p.eq_ignore_ascii_case("large_v2") {
		return Some("ggml-large-v2.bin");
	}
	if p.eq_ignore_ascii_case("large-v3") || p.eq_ignore_ascii_case("large_v3") || p.eq_ignore_ascii_case("large") {
		return Some("ggml-large-v3.bin");
	}
	None
}

/// `voice_whisper_model_path` の文字列を解決する。
///
/// - 既存のファイルパス（相対・絶対）ならそのまま使う。
/// - それ以外はプリセット名（`tiny`, `base`, …）として解釈し、キャッシュに無ければダウンロードする。
pub(crate) fn resolve_whisper_model(spec: &str) -> Result<PathBuf, String> {
	let s = spec.trim();
	if s.is_empty() {
		return Err("voice_whisper_model_path が空です".into());
	}

	let path = Path::new(s);
	if path.is_file() {
		return path.canonicalize().map_err(|e| format!("モデルパスの canonicalize に失敗: {e}"));
	}

	let looks_like_path = s.contains('/') || s.contains('\\') || is_windows_drive_path(s);
	if looks_like_path {
		return Err(format!("Whisper モデルファイルが見つかりません: {}", s));
	}

	let filename = preset_to_filename(s).ok_or_else(|| {
  format!(
   "不明なプリセットまたはパスです: {s}。tiny / base / small / medium / large-v2 / large-v3、または ggml-*.bin のファイルパスを指定してください。"
  )
 })?;

	let dir = cache_dir();
	std::fs::create_dir_all(&dir).map_err(|e| format!("キャッシュディレクトリ作成: {e}"))?;
	let dest = dir.join(filename);

	if dest.is_file() {
		let len = dest.metadata().map(|m| m.len()).unwrap_or(0);
		if len > 512 * 1024 {
			log::info!("《Voice》: Whisper キャッシュを使用します {:?} ({} MiB)", dest, len / (1024 * 1024));
			return Ok(dest);
		}
		let _ = std::fs::remove_file(&dest);
	}

	let url = format!("{}/{}", HF_RESOLVE_BASE, filename);
	log::info!(
		"《Voice》: Whisper モデルを初回ダウンロードします（数分かかることがあります）\n  {}\n  -> {:?}",
		url,
		dest
	);

	let mut resp = reqwest::blocking::get(&url).map_err(|e| format!("HTTP GET: {e}"))?;
	if !resp.status().is_success() {
		return Err(format!("モデルのダウンロードに失敗しました: HTTP {} ({})", resp.status(), url));
	}

	let part = dest.with_extension("bin.part");
	if let Some(parent) = part.parent() {
		let _ = std::fs::create_dir_all(parent);
	}
	{
		let mut file = std::fs::File::create(&part).map_err(|e| format!("一時ファイル: {e}"))?;
		std::io::copy(&mut resp, &mut file).map_err(|e| format!("ダウンロード書き込み: {e}"))?;
		file.flush().ok();
	}

	std::fs::rename(&part, &dest).map_err(|e| format!("モデルファイルの確定に失敗: {e}"))?;

	log::info!("《Voice》: Whisper モデルのダウンロードが完了しました {:?}", dest);
	Ok(dest)
}

fn is_windows_drive_path(s: &str) -> bool {
	let b = s.as_bytes();
	b.len() >= 2 && b[1] == b':' && b[0].is_ascii_alphabetic()
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn preset_maps() {
		assert_eq!(preset_to_filename("BASE"), Some("ggml-base.bin"));
		assert_eq!(preset_to_filename("large"), Some("ggml-large-v3.bin"));
		assert!(preset_to_filename("nosuch").is_none());
	}
}
