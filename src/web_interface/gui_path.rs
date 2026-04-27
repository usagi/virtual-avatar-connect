//! `/gui` URL を `gui/dist` 内の相対キーへ正規化（`..` 排除）。内蔵配信・将来の検証で共有。

/// `/gui` / `/gui/` / `/gui/assets/foo.js` → `index.html` または相対パス（`..` 禁止）。
#[cfg_attr(not(feature = "embed-gui"), allow(dead_code))]
pub fn path_under_gui(full_path: &str) -> Option<String> {
	let rest = full_path.strip_prefix("/gui").unwrap_or("");
	let trimmed = rest.trim_start_matches('/');
	if trimmed.is_empty() {
		return Some("index.html".into());
	}
	if !is_safe_rel_path(trimmed) {
		return None;
	}
	Some(trimmed.into())
}

#[cfg_attr(not(feature = "embed-gui"), allow(dead_code))]
fn is_safe_rel_path(p: &str) -> bool {
	for part in p.split('/') {
		if part == ".." {
			return false;
		}
	}
	true
}

#[cfg(test)]
mod tests {
	use super::path_under_gui;

	#[test]
	fn path_under_gui_root() {
		assert_eq!(path_under_gui("/gui").as_deref(), Some("index.html"));
		assert_eq!(path_under_gui("/gui/").as_deref(), Some("index.html"));
	}

	#[test]
	fn path_under_gui_asset() {
		assert_eq!(path_under_gui("/gui/assets/index-abc.js").as_deref(), Some("assets/index-abc.js"));
	}

	#[test]
	fn path_under_gui_rejects_traversal() {
		assert!(path_under_gui("/gui/../etc/passwd").is_none());
	}
}
