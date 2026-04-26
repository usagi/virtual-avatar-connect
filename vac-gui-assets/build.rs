//! `gui/dist` が存在することを検証し、配下の変更で再ビルドする。

use std::fs;
use std::path::Path;

fn main() {
	let manifest = std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR");
	let dist = Path::new(&manifest).join("../gui/dist");
	let index = dist.join("index.html");
	if !index.is_file() {
		panic!(
			"vac-gui-assets: {} が見つかりません。次を実行してから再度ビルドしてください:\n  cd gui && npm ci && npm run build",
			index.display()
		);
	}
	rerun_if_dir_changed(&dist);
}

fn rerun_if_dir_changed(dir: &Path) {
	let Ok(entries) = fs::read_dir(dir) else {
		println!("cargo:rerun-if-changed={}", dir.display());
		return;
	};
	for e in entries.flatten() {
		let p = e.path();
		println!("cargo:rerun-if-changed={}", p.display());
		if p.is_dir() {
			rerun_if_dir_changed(&p);
		}
	}
}
