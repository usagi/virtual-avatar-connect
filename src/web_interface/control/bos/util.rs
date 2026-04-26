//! BOS 走査のカテゴリ推定・タイトル生成・ `channel` パラメータ検出。

use std::path::Path;

use super::BosCategory;

/// サブディレクトリ名からカテゴリを推定する。接頭辞マッチ。
pub(super) fn guess_category(name: &str) -> BosCategory {
	let lower = name.to_ascii_lowercase();
	if lower.starts_with("subtitles") || lower.starts_with("subtitle") || lower.starts_with("caption") {
		BosCategory::Subtitles
	} else if lower.starts_with("bgm") || lower.contains("music") || lower.contains("player") {
		BosCategory::Bgm
	} else if lower.starts_with("effect") || lower.starts_with("fx") {
		BosCategory::Effects
	} else {
		BosCategory::Other
	}
}

/// `index.html` または同ディレクトリの `.js` に `channel` 言及があるかを雑に検出。
/// 大きすぎるファイルは先頭 128 KiB だけ読む（無駄 I/O 防止）。
pub(super) fn detect_channel_param(dir: &Path) -> Option<bool> {
	const MAX_READ: usize = 128 * 1024;
	let check = |p: &Path| -> bool {
		let Ok(mut f) = std::fs::File::open(p) else { return false };
		let mut buf = Vec::with_capacity(4096);
		use std::io::Read as _;
		// ベストエフォートで一気に読む（小さいはず）
		let _ = f.by_ref().take(MAX_READ as u64).read_to_end(&mut buf);
		let text = String::from_utf8_lossy(&buf);
		text.contains("channel")
	};
	if check(&dir.join("index.html")) {
		return Some(true);
	}
	// 同階層の .js も 1 段だけ覗く。poll.js など。
	if let Ok(iter) = std::fs::read_dir(dir) {
		for ent in iter.flatten() {
			let p = ent.path();
			if p.extension().and_then(|e| e.to_str()) == Some("js") && check(&p) {
				return Some(true);
			}
		}
	}
	Some(false)
}

/// `subtitles-3` → "Subtitles 3"、`bgm-player` → "Bgm Player"。
pub(super) fn title_from_id(id: &str) -> String {
	id.split(['-', '_'])
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
