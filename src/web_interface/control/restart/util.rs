//! 再起動 API のパス解決・プロファイル一覧用ラベル・パス同値判定。

use std::path::{Path, PathBuf};

/// 新 conf path の安全性を検証したうえで絶対パスに解決する。
///
/// 受け入れるパターン:
///   - 絶対パス: そのまま（存在確認する）
///   - 相対: 現 conf の親ディレクトリからの相対（パス区切りを含むなら親ディレクトリ制約のみ）
///     - `..` で親を辿って現ディレクトリ外に脱出する指定は弾く
pub(crate) fn resolve_new_conf_path(current_conf: &Path, requested: &str) -> Result<PathBuf, String> {
	let p = Path::new(requested);
	let resolved = if p.is_absolute() {
		p.to_path_buf()
	} else {
		let parent = current_conf
			.parent()
			.ok_or_else(|| "current conf has no parent directory".to_string())?;
		parent.join(p)
	};

	// 正規化して脱出チェック。canonicalize は存在チェック付き。
	let canon = std::fs::canonicalize(&resolved).map_err(|e| format!("cannot resolve conf path {}: {e}", resolved.display()))?;

	let parent_canon = current_conf.parent().and_then(|p| std::fs::canonicalize(p).ok());
	if let Some(parent_canon) = parent_canon {
		if !canon.starts_with(&parent_canon) {
			return Err(format!(
				"conf path {} is outside the current conf directory ({})",
				canon.display(),
				parent_canon.display()
			));
		}
	}

	if canon.extension().and_then(|e| e.to_str()) != Some("toml") {
		return Err(format!("conf path {} is not a .toml file", canon.display()));
	}
	if !canon.is_file() {
		return Err(format!("conf path {} is not a regular file", canon.display()));
	}

	Ok(canon)
}

/// `conf.example-twitch.toml` → "Conf Example Twitch" 程度。過度に凝らない。
pub(crate) fn label_from_filename(filename: &str) -> String {
	let stem = filename.trim_end_matches(".toml");
	// ドット/ダッシュ/アンダースコアで区切って単語化し、先頭大文字化。
	stem.split(['.', '-', '_'])
		.filter(|s| !s.is_empty())
		.map(|s| {
			let mut chars = s.chars();
			match chars.next() {
				Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
				None => String::new(),
			}
		})
		.collect::<Vec<_>>()
		.join(" ")
}

/// ディレクトリ/シンボリックリンク/大文字小文字を正規化したうえで 2 つのパスを比較する。
/// 失敗時は素の比較にフォールバック。
pub(crate) fn paths_equivalent(a: &Path, b: &Path) -> bool {
	match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
		(Ok(aa), Ok(bb)) => aa == bb,
		_ => a == b,
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn label_from_filename_basic() {
		assert_eq!(label_from_filename("conf.toml"), "Conf");
		assert_eq!(label_from_filename("conf.example-twitch.toml"), "Conf Example Twitch");
		assert_eq!(label_from_filename("streaming_kaltsit.toml"), "Streaming Kaltsit");
		assert_eq!(label_from_filename(".toml"), "");
	}
}
