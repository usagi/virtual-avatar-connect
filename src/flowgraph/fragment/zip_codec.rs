//! Phase δ-7d: Fragment ZIP codec（spec §9.3）。
//!
//! 複数ファイル（＋補助ドキュメント等のコンパニオン）をまとめて 1 つの ZIP に export / import する。
//!
//! ## 構成
//! ```text
//! fragment.flowgraph.zip
//! ├── manifest.flowgraph.toml  // [fragment] header + [[fragment.danglings]] のみ
//! └── flowgraph/               // 元のフォルダ構造をそのまま内包
//!     ├── main.flowgraph.toml
//!     └── .../**/*.flowgraph.toml
//! ```
//! `[[fragment.files]]` は ZIP 内の実ファイルに委譲する（spec §9.3）。scope=nodes のときは
//! `flowgraph/__scratch__.flowgraph.toml` 1 本。
//!
//! ## 安全ルール（import 時、spec §9.3）
//! - `..` / 絶対パス / Windows ドライブ / シンボリックリンク → 拒否
//! - 実行ファイル系拡張子（`.exe`, `.dll`, `.bat`, `.ps1`, `.sh`, `.cmd`, `.com`, `.scr`, `.msi`）→ 拒否
//! - 上記以外は許可（`.md`, 画像, 音声等のコンパニオンを許容）
//!
//! ## dry_run プレビュー
//! import は `dry_run=true` でプレビューを返し、衝突一覧・dangling 一覧・進入予定の file 一覧を提示。
//! GUI はこれを見てから `dry_run=false` で本番 import を実行する。

use crate::flowgraph::fragment::paste::{
	paste_fragment, PasteError, PasteOptions, PasteRequest, PasteTarget,
};
use crate::flowgraph::fragment::{
	copy_targets, parse_fragment, serialize_fragment, CopyError, CopyRequest, Fragment,
	FragmentDangling, FragmentFile, FragmentHeader, FragmentScope, SCRATCH_PATH,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::{Cursor, Read, Write};
use std::path::{Path, PathBuf};

/// 拒否する危険拡張子（spec §9.3）。
/// 大文字小文字は区別せず小文字で比較する。
const DENY_EXTENSIONS: &[&str] = &[
	"exe", "dll", "bat", "ps1", "sh", "cmd", "com", "scr", "msi", "app", "apk", "jar",
];

/// ZIP 内で flowgraph ファイルを置くフォルダ（spec §9.3）。
const ZIP_FLOWGRAPH_PREFIX: &str = "flowgraph/";

/// ZIP 内の manifest file 名。
const MANIFEST_NAME: &str = "manifest.flowgraph.toml";

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum ZipExportError {
	#[error("copy に失敗: {0}")]
	Copy(#[from] CopyError),
	#[error("serialize に失敗: {0}")]
	Serialize(String),
	#[error("zip 書き込みに失敗: {0}")]
	Zip(#[from] zip::result::ZipError),
	#[error("I/O: {0}")]
	Io(#[from] std::io::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum ZipImportError {
	#[error("zip 読み出しに失敗: {0}")]
	Zip(#[from] zip::result::ZipError),
	#[error("I/O: {0}")]
	Io(#[from] std::io::Error),
	#[error("manifest が見つからない: '{MANIFEST_NAME}' が zip 内にありません")]
	ManifestMissing,
	#[error("manifest パース失敗: {0}")]
	ManifestParse(String),
	#[error("危険なエントリを検出: {0}")]
	UnsafeEntry(String),
	#[error("paste 段階で失敗: {0}")]
	Paste(#[from] PasteError),
}

// ---------------------------------------------------------------------------
// Export
// ---------------------------------------------------------------------------

/// copy と同じ targets 指定で、ZIP バイナリを生成する。
///
/// 内部では `copy_targets` で fragment を作り、`[[fragment.files]]` を ZIP 内の
/// 実ファイルに展開する。`manifest.flowgraph.toml` は fragment から files セクションを
/// 抜いた薄いメタだけ残す（spec §9.3）。
pub fn export_zip(root: &Path, req: &CopyRequest) -> Result<Vec<u8>, ZipExportError> {
	let fragment = copy_targets(root, req)?;

	let mut buf: Vec<u8> = Vec::new();
	{
		let cursor = Cursor::new(&mut buf);
		let mut zw = zip::ZipWriter::new(cursor);
		let opts: zip::write::SimpleFileOptions = zip::write::SimpleFileOptions::default()
			.compression_method(zip::CompressionMethod::Deflated)
			.unix_permissions(0o644);

		// 1) manifest（fragment の header + danglings のみ）
		let manifest_str = build_manifest_toml(&fragment)?;
		zw.start_file(MANIFEST_NAME, opts)?;
		zw.write_all(manifest_str.as_bytes())?;

		// 2) flowgraph/ 配下の各ファイル
		for ff in &fragment.files {
			let zip_path = format!("{ZIP_FLOWGRAPH_PREFIX}{}", ff.path);
			zw.start_file(&zip_path, opts)?;
			let file_toml = serialize_fragment_file_as_flowgraph_toml(ff)
				.map_err(ZipExportError::Serialize)?;
			zw.write_all(file_toml.as_bytes())?;
		}

		zw.finish()?;
	}
	Ok(buf)
}

/// manifest = fragment から `[[fragment.files]]` を外した TOML。
fn build_manifest_toml(fragment: &Fragment) -> Result<String, ZipExportError> {
	let manifest = Fragment {
		header: fragment.header.clone(),
		files: Vec::new(),
		danglings: fragment.danglings.clone(),
	};
	serialize_fragment(&manifest).map_err(ZipExportError::Serialize)
}

/// FragmentFile を `*.flowgraph.toml` の生形式に書き出す（meta + nodes + edges）。
fn serialize_fragment_file_as_flowgraph_toml(ff: &FragmentFile) -> Result<String, String> {
	let mut root = toml::Table::new();
	if let Some(m) = &ff.meta {
		let v = toml::Value::try_from(m).map_err(|e| format!("meta: {e}"))?;
		root.insert("meta".into(), v);
	}
	if !ff.nodes.is_empty() {
		let v = toml::Value::try_from(&ff.nodes).map_err(|e| format!("nodes: {e}"))?;
		root.insert("nodes".into(), v);
	}
	if !ff.edges.is_empty() {
		let v = toml::Value::try_from(&ff.edges).map_err(|e| format!("edges: {e}"))?;
		root.insert("edges".into(), v);
	}
	toml::to_string_pretty(&toml::Value::Table(root)).map_err(|e| format!("serialize: {e}"))
}

// ---------------------------------------------------------------------------
// Import
// ---------------------------------------------------------------------------

/// import オプション。HTTP 層 (`POST /flowgraph/import/zip`) から渡す。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct ZipImportOptions {
	/// true なら実ファイル書き込みは行わず [`ZipImportPreview`] を返す。
	#[serde(default)]
	pub dry_run: bool,
	/// 既定 `"imported"`。prefix 以下に fragment の flowgraph/ を展開する。
	/// 空文字列で root 直下展開（上書きリスクが上がるので非推奨）。
	#[serde(default = "default_target_prefix")]
	pub target_prefix: String,
	/// paste オプション（衝突戦略・remap 等）。
	#[serde(default)]
	pub paste: PasteOptions,
}

fn default_target_prefix() -> String {
	"imported".to_string()
}

/// dry_run 応答。衝突と dangling の件数・リストを一覧化する。
#[derive(Debug, Clone, Serialize)]
pub struct ZipImportPreview {
	pub manifest: FragmentHeader,
	pub file_count: usize,
	/// 展開予定のエントリ（flowgraph/ 以下のみ / 安全チェック通過済）。
	pub entries: Vec<ZipImportEntry>,
	pub danglings: Vec<FragmentDangling>,
	/// target_prefix（実際に適用される）。
	pub target_prefix: String,
	/// 既存と衝突する fq（拡張子なしの root 相対）。
	pub conflicts: Vec<String>,
	/// companion files（`.flowgraph.toml` 以外、展開先のみ提示）。
	pub companions: Vec<ZipImportEntry>,
}

/// 本番 import の結果。内部で `paste_fragment` を呼んでいるため [`PasteReport`] 互換のサマリ。
#[derive(Debug, Clone, Serialize)]
pub struct ZipImportReport {
	pub manifest: FragmentHeader,
	pub written_files: Vec<String>,
	pub written_companions: Vec<String>,
	pub danglings_unresolved: Vec<FragmentDangling>,
	pub target_prefix: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct ZipImportEntry {
	pub zip_path: String,
	pub dest_path: String,
	pub size: u64,
}

/// ZIP バイナリを import（dry_run or 本番）。
pub fn import_zip(
	root: &Path,
	bytes: &[u8],
	opts: &ZipImportOptions,
) -> Result<ZipImportOutcome, ZipImportError> {
	let reader = Cursor::new(bytes);
	let mut archive = zip::ZipArchive::new(reader)?;

	// (1) manifest + safe paths を舐めてメモリに載せる。
	let mut manifest_text: Option<String> = None;
	let mut fg_files: BTreeMap<String, String> = BTreeMap::new(); // zip_rel_path → content
	let mut companions: Vec<ZipImportEntry> = Vec::new();
	let mut companion_bytes: BTreeMap<String, Vec<u8>> = BTreeMap::new();

	for i in 0..archive.len() {
		let mut entry = archive.by_index(i)?;
		let raw_name = entry.name().to_string();
		if entry.is_dir() {
			continue;
		}
		if is_unsafe_entry_path(&raw_name) {
			return Err(ZipImportError::UnsafeEntry(raw_name));
		}
		// unix mode 確認: symlink 検出。
		if let Some(mode) = entry.unix_mode() {
			// S_IFLNK = 0o120000
			if mode & 0o170000 == 0o120000 {
				return Err(ZipImportError::UnsafeEntry(format!(
					"symlink を拒否: {raw_name}"
				)));
			}
		}
		if has_deny_extension(&raw_name) {
			return Err(ZipImportError::UnsafeEntry(format!(
				"危険な拡張子を拒否: {raw_name}"
			)));
		}

		if raw_name == MANIFEST_NAME {
			let mut s = String::new();
			entry.read_to_string(&mut s)?;
			manifest_text = Some(s);
		} else if let Some(rel) = raw_name.strip_prefix(ZIP_FLOWGRAPH_PREFIX) {
			// flowgraph/ 配下
			if rel.ends_with(".flowgraph.toml") {
				let mut s = String::new();
				entry.read_to_string(&mut s)?;
				fg_files.insert(rel.to_string(), s);
			} else {
				// companion file（GUI 側で活用される前提で保存）
				let size = entry.size();
				let mut b = Vec::with_capacity(size as usize);
				entry.read_to_end(&mut b)?;
				companions.push(ZipImportEntry {
					zip_path: raw_name.clone(),
					dest_path: companion_dest(rel, &opts.target_prefix),
					size,
				});
				companion_bytes.insert(rel.to_string(), b);
			}
		}
		// それ以外（zip 直下の `.md` 等）は無視する。
	}

	let manifest_text = manifest_text.ok_or(ZipImportError::ManifestMissing)?;
	// manifest は Fragment の軽量版（files 空 + danglings のみ + header）。
	let manifest_frag = parse_fragment(&manifest_text).map_err(ZipImportError::ManifestParse)?;

	// (2) fragment を再構築: manifest.header + danglings + fg_files からの nodes/edges。
	let mut rebuilt = Fragment {
		header: manifest_frag.header.clone(),
		files: Vec::new(),
		danglings: manifest_frag.danglings.clone(),
	};
	for (rel, content) in &fg_files {
		let parsed = crate::flowgraph::loader::file::parse_flowgraph_file(content, None)
			.map_err(|e| ZipImportError::ManifestParse(format!("{rel}: {e}")))?;
		rebuilt.files.push(FragmentFile {
			path: rel.clone(),
			meta: parsed.meta,
			nodes: parsed.nodes,
			edges: parsed.edges,
		});
	}

	// (3) dry_run なら preview を作って return。
	let target_prefix = sanitize_target_prefix(&opts.target_prefix)?;
	if opts.dry_run {
		let entries = fg_files
			.iter()
			.map(|(rel, content)| ZipImportEntry {
				zip_path: format!("{ZIP_FLOWGRAPH_PREFIX}{rel}"),
				dest_path: flowgraph_dest(rel, &target_prefix),
				size: content.len() as u64,
			})
			.collect::<Vec<_>>();
		let conflicts = entries
			.iter()
			.filter(|e| Path::new(&e.dest_path).exists() || root.join(&e.dest_path).exists())
			.map(|e| e.dest_path.clone())
			.collect::<Vec<_>>();
		return Ok(ZipImportOutcome::Preview(ZipImportPreview {
			manifest: rebuilt.header,
			file_count: entries.len(),
			entries,
			danglings: rebuilt.danglings,
			target_prefix,
			conflicts,
			companions,
		}));
	}

	// (4) 本番書き込み: fragment の TOML を作って paste_fragment に委譲。
	let fragment_toml = serialize_fragment(&rebuilt).map_err(ZipImportError::ManifestParse)?;
	let paste_req = PasteRequest {
		fragment_toml,
		target: PasteTarget::Folder {
			path: target_prefix.clone(),
		},
		options: opts.paste.clone(),
	};
	let report = paste_fragment(root, &paste_req)?;

	// (5) companion files を flowgraph_dir/<target_prefix>/ 以下にそのまま展開。
	let mut written_companions: Vec<String> = Vec::new();
	for (rel, bytes) in &companion_bytes {
		let rel_path = Path::new(rel);
		// rel_path は flowgraph/ 配下の相対パス。再度 traversal チェックは is_unsafe_entry_path で済。
		let dest = compose_companion_path(root, &target_prefix, rel_path);
		if let Some(parent) = dest.parent() {
			std::fs::create_dir_all(parent)?;
		}
		std::fs::write(&dest, bytes)?;
		written_companions.push(dest.display().to_string().replace('\\', "/"));
	}

	Ok(ZipImportOutcome::Report(ZipImportReport {
		manifest: rebuilt.header,
		written_files: report
			.written_files
			.iter()
			.map(|w| w.path.clone())
			.collect(),
		written_companions,
		danglings_unresolved: report.unresolved_danglings,
		target_prefix,
	}))
}

/// `import_zip` の戻り値を dry_run / 本番で分岐。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ZipImportOutcome {
	Preview(ZipImportPreview),
	Report(ZipImportReport),
}

// ---------------------------------------------------------------------------
// Safety helpers
// ---------------------------------------------------------------------------

fn is_unsafe_entry_path(name: &str) -> bool {
	if name.is_empty() {
		return true;
	}
	if name.contains('\0') {
		return true;
	}
	// Windows ドライブ / UNC
	if name.starts_with('/') || name.starts_with('\\') {
		return true;
	}
	// `C:\...` 系
	let bytes = name.as_bytes();
	if bytes.len() >= 2 && bytes[1] == b':' {
		return true;
	}
	for seg in name.split('/') {
		if seg == ".." {
			return true;
		}
	}
	false
}

fn has_deny_extension(name: &str) -> bool {
	let lower = name.to_ascii_lowercase();
	let Some(dot) = lower.rfind('.') else {
		return false;
	};
	let ext = &lower[dot + 1..];
	DENY_EXTENSIONS.contains(&ext)
}

fn sanitize_target_prefix(s: &str) -> Result<String, ZipImportError> {
	let t = s.trim().trim_matches('/').replace('\\', "/");
	if t.is_empty() {
		return Ok(String::new());
	}
	for seg in t.split('/') {
		if seg == ".." || seg == "." || seg.is_empty() {
			return Err(ZipImportError::UnsafeEntry(format!(
				"target_prefix 不正: '{s}'"
			)));
		}
	}
	Ok(t)
}

fn flowgraph_dest(rel: &str, target_prefix: &str) -> String {
	if target_prefix.is_empty() {
		rel.to_string()
	} else {
		format!("{target_prefix}/{rel}")
	}
}

fn companion_dest(rel: &str, target_prefix: &str) -> String {
	if target_prefix.is_empty() {
		rel.to_string()
	} else {
		format!("{target_prefix}/{rel}")
	}
}

fn compose_companion_path(root: &Path, target_prefix: &str, rel: &Path) -> PathBuf {
	let mut p = root.to_path_buf();
	if !target_prefix.is_empty() {
		for seg in target_prefix.split('/') {
			p.push(seg);
		}
	}
	for c in rel.components() {
		if let std::path::Component::Normal(os) = c {
			p.push(os);
		}
	}
	p
}

// 未使用警告避け
#[allow(dead_code)]
fn _unused_scope_touch(_: FragmentScope) {}
#[allow(dead_code)]
const _SCRATCH_TOUCH: &str = SCRATCH_PATH;

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::fragment::{CopyRequest, CopyTarget, FragmentScope};
	use std::fs;
	use std::io::Write;
	use std::sync::atomic::{AtomicU64, Ordering};
	use std::time::{SystemTime, UNIX_EPOCH};

	struct TempDir(PathBuf);
	impl TempDir {
		fn path(&self) -> &Path {
			&self.0
		}
	}
	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.0);
		}
	}
	fn mk_dir() -> TempDir {
		static SEQ: AtomicU64 = AtomicU64::new(0);
		let ns = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
		let seq = SEQ.fetch_add(1, Ordering::Relaxed);
		let p = std::env::temp_dir().join(format!(
			"vac-fg-zip-{}-{ns}-{seq}",
			std::process::id()
		));
		fs::create_dir_all(&p).unwrap();
		TempDir(p)
	}
	fn write_file(p: &Path, s: &str) {
		if let Some(parent) = p.parent() {
			fs::create_dir_all(parent).unwrap();
		}
		let mut f = fs::File::create(p).unwrap();
		f.write_all(s.as_bytes()).unwrap();
	}

	#[test]
	fn export_import_roundtrip() {
		let src = mk_dir();
		write_file(
			&src.path().join("tts/main.flowgraph.toml"),
			r#"
[meta]
title = "Main"

[[nodes]]
id = "a"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "x"

[[nodes]]
id = "b"
feature = "flowgraph.log.info"

[[edges]]
from = "a:value"
to   = "b:message"
"#,
		);
		write_file(
			&src.path().join("tts/jp.flowgraph.toml"),
			r#"
[[nodes]]
id = "c"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "y"
"#,
		);

		let req = CopyRequest {
			scope: FragmentScope::Folder,
			targets: vec![CopyTarget::Folder { path: "tts".into() }],
			origin: Some("tests".into()),
		};
		let bytes = export_zip(src.path(), &req).unwrap();
		assert!(!bytes.is_empty());

		// zip 内に manifest と 2 ファイルがある。
		let mut ar = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
		let mut names: Vec<String> = (0..ar.len())
			.map(|i| ar.by_index(i).unwrap().name().to_string())
			.collect();
		names.sort();
		assert!(names.iter().any(|n| n == MANIFEST_NAME));
		assert!(names.iter().any(|n| n == "flowgraph/tts/main.flowgraph.toml"));
		assert!(names.iter().any(|n| n == "flowgraph/tts/jp.flowgraph.toml"));

		// dry_run preview
		let dest = mk_dir();
		let opts = ZipImportOptions {
			dry_run: true,
			target_prefix: "imported".into(),
			..Default::default()
		};
		let outcome = import_zip(dest.path(), &bytes, &opts).unwrap();
		let preview = match outcome {
			ZipImportOutcome::Preview(p) => p,
			_ => panic!("dry_run should return preview"),
		};
		assert_eq!(preview.file_count, 2);
		assert_eq!(preview.target_prefix, "imported");
		assert!(preview.conflicts.is_empty());
		assert_eq!(preview.manifest.scope, FragmentScope::Folder);

		// 本番 import
		let opts2 = ZipImportOptions {
			dry_run: false,
			target_prefix: "imported".into(),
			..Default::default()
		};
		let outcome = import_zip(dest.path(), &bytes, &opts2).unwrap();
		let rep = match outcome {
			ZipImportOutcome::Report(r) => r,
			_ => panic!("non-dry_run should return report"),
		};
		assert_eq!(rep.written_files.len(), 2);
		assert!(dest.path().join("imported/tts/main.flowgraph.toml").exists());
		assert!(dest.path().join("imported/tts/jp.flowgraph.toml").exists());
	}

	#[test]
	fn import_rejects_deny_extension() {
		// 人工的に `.exe` を含む ZIP を組み立てる。
		let mut buf: Vec<u8> = Vec::new();
		{
			let cursor = Cursor::new(&mut buf);
			let mut zw = zip::ZipWriter::new(cursor);
			let opts = zip::write::SimpleFileOptions::default();
			// manifest は最低限
			let frag = Fragment {
				header: FragmentHeader {
					schema: crate::flowgraph::fragment::FRAGMENT_SCHEMA_V1.into(),
					exported_at: "2026-04-17T00:00:00Z".into(),
					origin: None,
					scope: FragmentScope::File,
				},
				files: vec![],
				danglings: vec![],
			};
			zw.start_file(MANIFEST_NAME, opts).unwrap();
			zw.write_all(serialize_fragment(&frag).unwrap().as_bytes())
				.unwrap();
			zw.start_file("flowgraph/evil.exe", opts).unwrap();
			zw.write_all(b"MZ...").unwrap();
			zw.finish().unwrap();
		}

		let dest = mk_dir();
		let opts = ZipImportOptions {
			dry_run: true,
			..Default::default()
		};
		let err = import_zip(dest.path(), &buf, &opts).unwrap_err();
		assert!(matches!(err, ZipImportError::UnsafeEntry(_)));
	}

	#[test]
	fn import_rejects_parent_traversal() {
		let mut buf: Vec<u8> = Vec::new();
		{
			let cursor = Cursor::new(&mut buf);
			let mut zw = zip::ZipWriter::new(cursor);
			let opts = zip::write::SimpleFileOptions::default();
			let frag = Fragment {
				header: FragmentHeader {
					schema: crate::flowgraph::fragment::FRAGMENT_SCHEMA_V1.into(),
					exported_at: "2026-04-17T00:00:00Z".into(),
					origin: None,
					scope: FragmentScope::File,
				},
				files: vec![],
				danglings: vec![],
			};
			zw.start_file(MANIFEST_NAME, opts).unwrap();
			zw.write_all(serialize_fragment(&frag).unwrap().as_bytes())
				.unwrap();
			zw.start_file("flowgraph/../evil.flowgraph.toml", opts).unwrap();
			zw.write_all(b"").unwrap();
			zw.finish().unwrap();
		}
		let dest = mk_dir();
		let opts = ZipImportOptions::default();
		let err = import_zip(dest.path(), &buf, &opts).unwrap_err();
		assert!(matches!(err, ZipImportError::UnsafeEntry(_)));
	}

	#[test]
	fn import_missing_manifest_errors() {
		let mut buf: Vec<u8> = Vec::new();
		{
			let cursor = Cursor::new(&mut buf);
			let mut zw = zip::ZipWriter::new(cursor);
			zw.start_file("flowgraph/nope.flowgraph.toml", zip::write::SimpleFileOptions::default())
				.unwrap();
			zw.write_all(b"").unwrap();
			zw.finish().unwrap();
		}
		let dest = mk_dir();
		let err = import_zip(dest.path(), &buf, &ZipImportOptions::default()).unwrap_err();
		assert!(matches!(err, ZipImportError::ManifestMissing));
	}

	#[test]
	fn companion_file_roundtrips() {
		let src = mk_dir();
		write_file(
			&src.path().join("main.flowgraph.toml"),
			r#"
[[nodes]]
id = "a"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "x"
"#,
		);
		let req = CopyRequest {
			scope: FragmentScope::File,
			targets: vec![CopyTarget::File { fq: "main".into() }],
			origin: None,
		};
		let mut bytes = export_zip(src.path(), &req).unwrap();
		// 手動で companion として README.md を追加する（VAC export は current 実装では付けない）。
		{
			// 既存 zip を読み直して書き足すのが面倒なので新規作成。
			let mut buf: Vec<u8> = Vec::new();
			{
				let cursor = Cursor::new(&mut buf);
				let mut zw = zip::ZipWriter::new(cursor);
				// 既存エントリを写経
				let mut ar = zip::ZipArchive::new(Cursor::new(&bytes)).unwrap();
				for i in 0..ar.len() {
					let mut e = ar.by_index(i).unwrap();
					let name = e.name().to_string();
					zw.start_file(&name, zip::write::SimpleFileOptions::default())
						.unwrap();
					let mut s = Vec::new();
					e.read_to_end(&mut s).unwrap();
					zw.write_all(&s).unwrap();
				}
				zw.start_file(
					"flowgraph/README.md",
					zip::write::SimpleFileOptions::default(),
				)
				.unwrap();
				zw.write_all(b"# hi").unwrap();
				zw.finish().unwrap();
			}
			bytes = buf;
		}

		let dest = mk_dir();
		let outcome = import_zip(
			dest.path(),
			&bytes,
			&ZipImportOptions {
				dry_run: false,
				target_prefix: "imp".into(),
				..Default::default()
			},
		)
		.unwrap();
		let rep = match outcome {
			ZipImportOutcome::Report(r) => r,
			_ => panic!(),
		};
		assert!(rep.written_companions.iter().any(|p| p.ends_with("README.md")));
		assert!(dest.path().join("imp/README.md").exists());
	}
}
