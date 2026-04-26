//! Windows + `voice-vosk` 時: Vosk の `vosk-win64-*.zip` を `.vendors` にキャッシュし、
//! `libvosk.lib` をリンク、zip 同梱の **すべての `.dll`**（`libvosk.dll` と MinGW ランタイム
//! `libgcc_s_seh-1.dll` 等）を `target/{profile}/` と `deps/` にコピーする。

use std::env;
use std::fs;
use std::io::Cursor;
use std::path::{Path, PathBuf};

/// GitHub releases の zip 名・タグに合わせる（`VOSK_WIN64_VERSION` で上書き可）
const VOSK_WIN64_DEFAULT_VERSION: &str = "0.3.45";

fn main() {
	let target = env::var("TARGET").unwrap_or_default();
	let windows_target = target.contains("windows");
	let voice_vosk = env::var("CARGO_FEATURE_VOICE_VOSK").is_ok();

	if cfg!(target_os = "windows") {
		let mut res = winres::WindowsResource::new();
		res.set_icon("icon.ico");
		res.compile().unwrap();
	}

	println!("cargo:rerun-if-changed=build.rs");
	println!("cargo:rerun-if-env-changed=VOSK_WIN64_VERSION");
	println!("cargo:rerun-if-env-changed=VOSK_LIB_PATH");

	if env::var("CARGO_FEATURE_EMBED_GUI").is_ok() {
		let manifest = env::var("CARGO_MANIFEST_DIR").unwrap_or_default();
		let index = Path::new(&manifest).join("gui/dist/index.html");
		if !index.is_file() {
			panic!(
				"feature `embed-gui` にはビルド済み GUI が必要です。次を実行してから再度 `cargo build --features embed-gui` してください:\n  cd gui && npm ci && npm run build\n期待パス: {}",
				index.display()
			);
		}
		rerun_if_dir_changed(&Path::new(&manifest).join("gui/dist"));
	}

	if !voice_vosk || !windows_target {
		return;
	}

	if let Ok(manual) = env::var("VOSK_LIB_PATH") {
		let dir = PathBuf::from(manual);
		println!("cargo:rustc-link-search=native={}", dir.display());
		copy_vosk_dlls_from_dir(&dir);
		return;
	}

	match ensure_vosk_win64_in_vendors() {
  Ok(dir) => {
   println!("cargo:rustc-link-search=native={}", dir.display());
   copy_vosk_dlls_from_dir(&dir);
  },
  Err(e) => panic!(
   "Vosk Windows ライブラリの取得に失敗しました。`VOSK_LIB_PATH` で .lib があるディレクトリを指定するか、ネットワークと VOSK_WIN64_VERSION（既定 {}）を確認してください: {}",
   VOSK_WIN64_DEFAULT_VERSION,
   e
  ),
 }
}

fn ensure_vosk_win64_in_vendors() -> Result<PathBuf, String> {
	let manifest = env::var("CARGO_MANIFEST_DIR").map_err(|e| e.to_string())?;
	let version = env::var("VOSK_WIN64_VERSION").unwrap_or_else(|_| VOSK_WIN64_DEFAULT_VERSION.to_string());
	let cache_dir = PathBuf::from(&manifest).join(".vendors").join("vosk-win64").join(&version);

	fs::create_dir_all(&cache_dir).map_err(|e| e.to_string())?;

	let lib = cache_dir.join("libvosk.lib");
	let dll = cache_dir.join("libvosk.dll");
	// 旧 build は libvosk のみ展開していた。MinGW 同梱 DLL が無いキャッシュは不完全とみなして zip から再取得する。
	let gcc = cache_dir.join("libgcc_s_seh-1.dll");
	if lib.is_file() && dll.is_file() && gcc.is_file() {
		return Ok(cache_dir);
	}

	let tag = format!("v{}", version);
	let zip_name = format!("vosk-win64-{}.zip", version);
	let url = format!("https://github.com/alphacep/vosk-api/releases/download/{}/{}", tag, zip_name);

	let zip_path = cache_dir.join(&zip_name);
	eprintln!("[build] Vosk: ダウンロードします {}", url);
	download_file(&url, &zip_path)?;

	let data = fs::read(&zip_path).map_err(|e| e.to_string())?;
	extract_vosk_libs_from_zip(&data, &cache_dir)?;

	if !lib.is_file() || !dll.is_file() {
		return Err(format!("展開後も {} または {} が見つかりません", lib.display(), dll.display()));
	}
	if !gcc.is_file() {
		return Err(format!(
			"展開後も MinGW ランタイム {} が見つかりません（公式 zip の内容を確認してください）",
			gcc.display()
		));
	}

	let _ = fs::remove_file(&zip_path);

	Ok(cache_dir)
}

fn download_file(url: &str, dest: &Path) -> Result<(), String> {
	let resp = ureq::get(url)
		.set(
			"User-Agent",
			"virtual-avatar-connect/build.rs (Vosk vendor; https://github.com/alphacep/vosk-api)",
		)
		.call()
		.map_err(|e| format!("GET {}: {}", url, e))?;

	if resp.status() != 200 {
		return Err(format!("GET {} が HTTP {} を返しました", url, resp.status()));
	}

	let mut reader = resp.into_reader();
	let mut f = fs::File::create(dest).map_err(|e| e.to_string())?;
	std::io::copy(&mut reader, &mut f).map_err(|e| format!("保存: {}", e))?;
	Ok(())
}

fn extract_vosk_libs_from_zip(data: &[u8], dest_flat: &Path) -> Result<(), String> {
	let reader = Cursor::new(data);
	let mut archive = zip::ZipArchive::new(reader).map_err(|e| format!("zip: {}", e))?;

	for i in 0..archive.len() {
		let mut file = archive.by_index(i).map_err(|e| format!("zip index {}: {}", i, e))?;
		let name = file.name();
		let base = Path::new(name).file_name().and_then(|s| s.to_str()).unwrap_or("");
		let lower = base.to_ascii_lowercase();
		// 公式 zip には libvosk に加え MinGW 依存 DLL（libgcc_s_seh-1.dll 等）が入る。すべて展開する。
		if !lower.ends_with(".dll") && base != "libvosk.lib" {
			continue;
		}
		let out = dest_flat.join(base);
		let mut out_f = fs::File::create(&out).map_err(|e| format!("create {}: {}", out.display(), e))?;
		std::io::copy(&mut file, &mut out_f).map_err(|e| format!("write {}: {}", out.display(), e))?;
	}

	Ok(())
}

/// `target/debug` または `target/release`（exe と同じ階層）。
/// Cargo が `CARGO_TARGET_DIR` を渡さない環境があるため `OUT_DIR` から導出する。
fn profile_output_dir() -> Option<PathBuf> {
	if let Ok(td) = env::var("CARGO_TARGET_DIR") {
		let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".into());
		return Some(PathBuf::from(td).join(profile));
	}
	let out = env::var("OUT_DIR").ok()?;
	let p = PathBuf::from(out);
	// .../target/debug/build/<crate-hash>/out → 3 つ上が profile 出力ルート
	p.parent()?.parent()?.parent().map(|x| x.to_path_buf())
}

/// `vendor_dir` 内のすべての `.dll` を exe と同じ `target/{profile}/` および `deps/` へコピーする。
fn copy_vosk_dlls_from_dir(vendor_dir: &Path) {
	let Some(out_dir) = profile_output_dir() else {
		eprintln!("[build] 警告: 出力ディレクトリを決められません（OUT_DIR / CARGO_TARGET_DIR）");
		return;
	};

	let entries = match fs::read_dir(vendor_dir) {
		Ok(e) => e,
		Err(e) => {
			eprintln!("[build] 警告: {} を読めません（DLL コピーをスキップ）: {}", vendor_dir.display(), e);
			return;
		}
	};

	if let Err(e) = fs::create_dir_all(&out_dir) {
		eprintln!("[build] 警告: {} を作成できません: {}", out_dir.display(), e);
		return;
	}
	let deps = out_dir.join("deps");
	if let Err(e) = fs::create_dir_all(&deps) {
		eprintln!("[build] 警告: {} を作成できません: {}", deps.display(), e);
		return;
	}

	let mut any_dll = false;
	for entry in entries.flatten() {
		let p = entry.path();
		if !p.is_file() {
			continue;
		}
		if !p.extension().map(|ex| ex.eq_ignore_ascii_case("dll")).unwrap_or(false) {
			continue;
		}
		any_dll = true;
		let Some(name) = p.file_name() else {
			continue;
		};

		let dest_exe = out_dir.join(name);
		if let Err(e) = fs::copy(&p, &dest_exe) {
			eprintln!("[build] 警告: {:?} -> {} へコピーできません: {}", p, dest_exe.display(), e);
		}
		let dest_deps = deps.join(name);
		if let Err(e) = fs::copy(&p, &dest_deps) {
			eprintln!("[build] 警告: {:?} -> {} へコピーできません: {}", p, dest_deps.display(), e);
		}
	}

	if !any_dll {
		eprintln!(
			"[build] 警告: {} に .dll がありません。公式 vosk-win64 zip を展開したディレクトリか VOSK_LIB_PATH を確認してください。",
			vendor_dir.display()
		);
	}
}

/// `embed-gui` 時に `gui/dist` 配下の変更で再コンパイルする。
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
