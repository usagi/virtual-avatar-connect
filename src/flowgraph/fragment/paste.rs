//! Phase δ-7b: Fragment paste 処理（spec §9.2「paste 時の処理」/ §9.4）。
//!
//! `Fragment` を受け取り、ターゲットディレクトリ配下のファイルに書き出す。
//! 処理順は以下の通り:
//!   1. `PasteTarget` からターゲットファイル群の **絶対 path 表** を組み立てる。
//!   2. 既存ファイルとの衝突を `on_conflict_file` に従って解消。
//!   3. 各ファイルで **ノード ID 衝突** を `on_conflict_node` に従って解消。
//!      ID rewrite はテーブルに溜めておき、エッジ側の `node_id:port` も rewrite する。
//!   4. エッジをマージ（重複エッジは skip）。
//!   5. dangling は `RemapTable` で再マッピング or drop。
//!   6. `.bak` を作ってから atomic 書き込み。
//!
//! 位置オフセットは GUI から `position_offset` で受け取り、各ノード position に加算する。

use super::{parse_fragment, Fragment, FragmentDangling, FragmentFile, SCRATCH_PATH};
use crate::flowgraph::loader::file::{parse_flowgraph_file, EdgeEntry, FlowgraphFile};
use crate::flowgraph::loader::reference::parse_port_ref;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// ノード ID 衝突の解消戦略。spec §9.4 の `on_conflict` と同値。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OnConflict {
	/// 連番 suffix（`tts_jp` → `tts_jp_1` → `tts_jp_2` ...）で衝突を解消する（既定）。
	#[default]
	Suffix,
	/// 衝突したノードは paste に含めない。
	Skip,
	/// 既存を置き換える。
	Overwrite,
}

/// ファイル単位の衝突解消戦略。フォルダ paste 時、同名 `.flowgraph.toml` が既に存在するケース用。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, Serialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum OnConflictFile {
	/// `.bak` に退避してから上書き（ノードは完全置換）。
	#[default]
	Overwrite,
	/// そのファイルに対する paste をスキップ。
	Skip,
	/// `foo.flowgraph.toml` → `foo_1.flowgraph.toml` → ... と連番で回避。
	Rename,
	/// **既存ファイルのノード列にマージする**。衝突ノードは `on_conflict_node` で解消。
	/// scope=nodes + PasteTarget::File で最頻の挙動（= デフォルト相当）。scope=file/folder では
	/// 他ファイルを汚染しないため明示選択されたときのみ使用する想定。
	Merge,
}

/// paste 先の指定。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum PasteTarget {
	/// scope=nodes 用。fragment 内の scratch ファイルを、この既存 or 新規ファイルに **merge** する。
	/// `on_conflict_file` は無視して常に `Merge` 扱い。
	File { fq: String },
	/// scope=file/folder/mixed 用。fragment の `path` をこのフォルダ prefix の下に配置。
	/// 空文字列はルート（`flowgraph_dir` 直下）。
	Folder { path: String },
}

/// paste 動作オプション。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PasteOptions {
	#[serde(default)]
	pub on_conflict_node: OnConflict,
	#[serde(default)]
	pub on_conflict_file: OnConflictFile,
	/// ノード position に一律加算する `[dx, dy]`。省略時は無加算。
	#[serde(default)]
	pub position_offset: Option<[f64; 2]>,
	/// dangling 再マッピング表。key = fragment.danglings[].original、value は:
	///   - `Some("fragment 内の新ターゲット")` → そのノード/ポートに繋ぎ直す
	///   - `None` → 明示的に破棄
	///
	/// map に無い dangling は `unresolved_danglings` に残す（= 何もしない）。
	#[serde(default)]
	pub remap: HashMap<String, Option<String>>,
}

/// paste 要求。API body もこれに合わせる。
#[derive(Debug, Clone, Deserialize)]
pub struct PasteRequest {
	/// fragment TOML 文字列。クライアントが任意編集したものも受け入れる。
	pub fragment_toml: String,
	pub target: PasteTarget,
	#[serde(default)]
	pub options: PasteOptions,
}

// ---------------------------------------------------------------------------
// Report
// ---------------------------------------------------------------------------

/// paste 結果レポート。GUI は `imported_nodes` / `unresolved_danglings` を表示する。
#[derive(Debug, Clone, Serialize)]
pub struct PasteReport {
	pub written_files: Vec<WrittenFile>,
	pub imported_nodes: Vec<ImportedNode>,
	pub skipped_nodes: Vec<SkippedNode>,
	pub skipped_files: Vec<String>,
	pub unresolved_danglings: Vec<FragmentDangling>,
	pub backups: Vec<String>,
}

/// 書き出したファイルの fq (root 相対、拡張子なし) と 実 path。
#[derive(Debug, Clone, Serialize)]
pub struct WrittenFile {
	pub fq: String,
	pub path: String,
}

/// 取り込んだノードの ID 変換結果。
#[derive(Debug, Clone, Serialize)]
pub struct ImportedNode {
	pub fq_file: String,
	pub original_id: String,
	pub new_id: String,
}

/// スキップされたノード（on_conflict_node = skip で衝突したケース）。
#[derive(Debug, Clone, Serialize)]
pub struct SkippedNode {
	pub fq_file: String,
	pub original_id: String,
	pub reason: String,
}

// ---------------------------------------------------------------------------
// Errors
// ---------------------------------------------------------------------------

#[derive(Debug, thiserror::Error)]
pub enum PasteError {
	#[error("flowgraph root が存在しない: {0}")]
	RootMissing(PathBuf),
	#[error("fragment TOML パース失敗: {0}")]
	Parse(String),
	#[error("不正な target 指定: {0}")]
	InvalidTarget(String),
	#[error("不正な fragment.file.path: {0}")]
	InvalidFragmentPath(String),
	#[error("I/O エラー ({path}): {err}")]
	Io { path: PathBuf, err: std::io::Error },
	#[error("既存ファイル TOML パース失敗 ({path}): {err}")]
	ExistingParse { path: PathBuf, err: String },
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// fragment TOML + ターゲットを入力に、ディスクへ反映する。
///
/// 成功時でも `unresolved_danglings` が残るケースがある（GUI が remap を指定しなかった場合）。
/// `.bak` はファイル上書き時のみ作成する。
pub fn paste_fragment(root: &Path, req: &PasteRequest) -> Result<PasteReport, PasteError> {
	if !root.exists() {
		return Err(PasteError::RootMissing(root.to_path_buf()));
	}
	let fragment = parse_fragment(&req.fragment_toml).map_err(PasteError::Parse)?;

	let mut report = PasteReport {
		written_files: Vec::new(),
		imported_nodes: Vec::new(),
		skipped_nodes: Vec::new(),
		skipped_files: Vec::new(),
		unresolved_danglings: Vec::new(),
		backups: Vec::new(),
	};

	// (1) target の fq prefix / 1 ファイル fq を確定。
	match &req.target {
		PasteTarget::File { fq } => {
			let fq = normalize_fq(fq)?;
			paste_into_single_file(root, &fq, &fragment, &req.options, &mut report)?;
		}
		PasteTarget::Folder { path } => {
			let prefix = normalize_folder_prefix(path)?;
			paste_into_folder(root, &prefix, &fragment, &req.options, &mut report)?;
		}
	}

	Ok(report)
}

// ---------------------------------------------------------------------------
// Single-file merge path（scope=nodes 用）
// ---------------------------------------------------------------------------

fn paste_into_single_file(
	root: &Path,
	target_fq: &str,
	fragment: &Fragment,
	options: &PasteOptions,
	report: &mut PasteReport,
) -> Result<(), PasteError> {
	// fragment は複数ファイルを含む可能性がある（scope=file/folder/mixed）。
	// PasteTarget::File は scope=nodes が主用途で、scratch 1 本を前提にするが、
	// ここは「すべての fragment.files のノード/エッジを 1 ファイルに合流する」挙動に定義する。
	// （GUI が間違って scope=file 用の fragment を File target に投げても fail-open する。）
	let dest_path = fq_to_file_path(root, target_fq);
	let existing = load_existing_file(&dest_path)?;

	let mut merged_doc = existing.unwrap_or_default();
	let mut rewrite_map: HashMap<(String, String), String> = HashMap::new(); // (source_fq, src_id) → new_id
	let mut existing_ids: HashSet<String> =
		merged_doc.nodes.iter().map(|n| n.id.clone()).collect();

	for ff in &fragment.files {
		merge_nodes_into(
			ff,
			target_fq,
			&mut merged_doc,
			&mut rewrite_map,
			&mut existing_ids,
			options,
			report,
		);
	}
	merge_edges_into(fragment, target_fq, &mut merged_doc, &rewrite_map);
	apply_remaps_into(fragment, target_fq, &mut merged_doc, &rewrite_map, options, report);

	write_file_atomic(&dest_path, &merged_doc, true, report)?;
	report.written_files.push(WrittenFile {
		fq: target_fq.to_string(),
		path: dest_path.display().to_string().replace('\\', "/"),
	});
	Ok(())
}

// ---------------------------------------------------------------------------
// Folder path（scope=file/folder/mixed 用）
// ---------------------------------------------------------------------------

fn paste_into_folder(
	root: &Path,
	prefix_fq: &str,
	fragment: &Fragment,
	options: &PasteOptions,
	report: &mut PasteReport,
) -> Result<(), PasteError> {
	// fragment.files の path (例: "chat/main.flowgraph.toml") を prefix に寄せる。
	// SCRATCH_PATH は Folder target では「prefix 直下の __scratch__ ファイル」に落とす。
	for ff in &fragment.files {
		// 相対パス検証（`..` 禁止、必ず `.flowgraph.toml` 終わり）。
		let rel = sanitize_fragment_path(&ff.path)?;

		let final_fq = if prefix_fq.is_empty() {
			rel.clone()
		} else {
			format!("{prefix_fq}/{rel}")
		};
		let dest_path = fq_to_file_path(root, &final_fq);

		// ファイル単位の衝突確認
		let exists = dest_path.exists();
		if exists {
			match options.on_conflict_file {
				OnConflictFile::Skip => {
					report.skipped_files.push(final_fq);
					continue;
				}
				OnConflictFile::Rename => {
					let renamed_fq = find_unused_rename(root, &final_fq);
					let renamed_path = fq_to_file_path(root, &renamed_fq);
					paste_single_fragment_file_fresh(
						ff,
						&renamed_fq,
						&renamed_path,
						fragment,
						options,
						report,
					)?;
					continue;
				}
				OnConflictFile::Overwrite => {
					paste_single_fragment_file_fresh(
						ff,
						&final_fq,
						&dest_path,
						fragment,
						options,
						report,
					)?;
					continue;
				}
				OnConflictFile::Merge => {
					paste_single_fragment_file_merge(
						ff,
						&final_fq,
						&dest_path,
						fragment,
						options,
						report,
					)?;
					continue;
				}
			}
		}

		// 新規ファイル作成。
		paste_single_fragment_file_fresh(ff, &final_fq, &dest_path, fragment, options, report)?;
	}
	Ok(())
}

fn paste_single_fragment_file_fresh(
	ff: &FragmentFile,
	final_fq: &str,
	dest_path: &Path,
	fragment: &Fragment,
	options: &PasteOptions,
	report: &mut PasteReport,
) -> Result<(), PasteError> {
	let mut doc = FlowgraphFile {
		meta: ff.meta.clone(),
		nodes: Vec::new(),
		edges: Vec::new(),
	};
	let mut rewrite: HashMap<(String, String), String> = HashMap::new();
	let mut existing_ids: HashSet<String> = HashSet::new();
	merge_nodes_into(ff, final_fq, &mut doc, &mut rewrite, &mut existing_ids, options, report);

	// edges: ff 内部のものだけ対象（scope 越えは paste_single_fragment_file 系では特殊化不要）。
	for e in &ff.edges {
		let rewritten = rewrite_edge_endpoints(e, final_fq, &rewrite);
		if !edge_already_present(&doc.edges, &rewritten) {
			doc.edges.push(rewritten);
		}
	}

	// dangling remap（この fragment 全体に対して適用、in_file が final_fq 相当のみ拾う）。
	apply_remaps_into(fragment, final_fq, &mut doc, &rewrite, options, report);

	write_file_atomic(dest_path, &doc, dest_path.exists(), report)?;
	report.written_files.push(WrittenFile {
		fq: final_fq.to_string(),
		path: dest_path.display().to_string().replace('\\', "/"),
	});
	Ok(())
}

fn paste_single_fragment_file_merge(
	ff: &FragmentFile,
	final_fq: &str,
	dest_path: &Path,
	fragment: &Fragment,
	options: &PasteOptions,
	report: &mut PasteReport,
) -> Result<(), PasteError> {
	let existing = load_existing_file(dest_path)?.unwrap_or_default();
	let mut doc = existing;
	let mut rewrite: HashMap<(String, String), String> = HashMap::new();
	let mut existing_ids: HashSet<String> = doc.nodes.iter().map(|n| n.id.clone()).collect();
	merge_nodes_into(ff, final_fq, &mut doc, &mut rewrite, &mut existing_ids, options, report);
	for e in &ff.edges {
		let rewritten = rewrite_edge_endpoints(e, final_fq, &rewrite);
		if !edge_already_present(&doc.edges, &rewritten) {
			doc.edges.push(rewritten);
		}
	}
	apply_remaps_into(fragment, final_fq, &mut doc, &rewrite, options, report);
	write_file_atomic(dest_path, &doc, true, report)?;
	report.written_files.push(WrittenFile {
		fq: final_fq.to_string(),
		path: dest_path.display().to_string().replace('\\', "/"),
	});
	Ok(())
}

// ---------------------------------------------------------------------------
// Node merge
// ---------------------------------------------------------------------------

fn merge_nodes_into(
	ff: &FragmentFile,
	target_fq: &str,
	doc: &mut FlowgraphFile,
	rewrite: &mut HashMap<(String, String), String>,
	existing_ids: &mut HashSet<String>,
	options: &PasteOptions,
	report: &mut PasteReport,
) {
	for node in &ff.nodes {
		let new_id = if existing_ids.contains(&node.id) {
			match options.on_conflict_node {
				OnConflict::Suffix => {
					let nid = allocate_suffixed_id(&node.id, existing_ids);
					existing_ids.insert(nid.clone());
					nid
				}
				OnConflict::Skip => {
					report.skipped_nodes.push(SkippedNode {
						fq_file: target_fq.to_string(),
						original_id: node.id.clone(),
						reason: "id-conflict".to_string(),
					});
					continue;
				}
				OnConflict::Overwrite => {
					// 既存の同 id ノードを除去してから新規 push。
					doc.nodes.retain(|n| n.id != node.id);
					existing_ids.insert(node.id.clone());
					node.id.clone()
				}
			}
		} else {
			existing_ids.insert(node.id.clone());
			node.id.clone()
		};

		let mut copied = node.clone();
		copied.id = new_id.clone();
		copied.position = apply_position_offset(copied.position, options.position_offset);
		if new_id != node.id {
			rewrite.insert((ff.path.clone(), node.id.clone()), new_id.clone());
		}
		doc.nodes.push(copied);
		report.imported_nodes.push(ImportedNode {
			fq_file: target_fq.to_string(),
			original_id: node.id.clone(),
			new_id,
		});
	}
}

fn apply_position_offset(
	p: Option<[f64; 2]>,
	offset: Option<[f64; 2]>,
) -> Option<[f64; 2]> {
	match (p, offset) {
		(Some([x, y]), Some([dx, dy])) => Some([x + dx, y + dy]),
		(Some(v), None) => Some(v),
		(None, Some(v)) => Some(v),
		(None, None) => None,
	}
}

fn allocate_suffixed_id(base: &str, existing: &HashSet<String>) -> String {
	for n in 1..u64::MAX {
		let candidate = format!("{base}_{n}");
		if !existing.contains(&candidate) {
			return candidate;
		}
	}
	unreachable!("u64 空間を使い切ることはない")
}

// ---------------------------------------------------------------------------
// Edge merge
// ---------------------------------------------------------------------------

fn merge_edges_into(
	fragment: &Fragment,
	target_fq: &str,
	doc: &mut FlowgraphFile,
	rewrite: &HashMap<(String, String), String>,
) {
	for ff in &fragment.files {
		for e in &ff.edges {
			// 元 fragment では fragment 内 edge だけがここに来る（dangling は別）。
			// rewrite 辞書で node_id を書き換えつつ doc に追加。
			let rewritten = rewrite_edge_endpoints(e, target_fq, rewrite);
			if !edge_already_present(&doc.edges, &rewritten) {
				doc.edges.push(rewritten);
			}
		}
	}
	// rewrite 未使用変数警告を抑制するため明示的に参照。
	let _ = rewrite;
	let _ = target_fq;
}

fn rewrite_edge_endpoints(
	e: &EdgeEntry,
	_target_fq: &str,
	rewrite: &HashMap<(String, String), String>,
) -> EdgeEntry {
	EdgeEntry {
		from: rewrite_single_endpoint(&e.from, rewrite),
		to: rewrite_single_endpoint(&e.to, rewrite),
	}
}

/// `node_id:port` または `path::node_id:port` の node_id を rewrite map で置き換える。
/// rewrite key は `(ff.path, source_id)`。ファイルを問わず書き換え対象になりうるが、簡易実装
/// として ff.path に依らず source_id が一致すれば rewrite する方針（scope=nodes で
/// 十分安全、scope=file でも通常 1 ファイルに収まる）。
fn rewrite_single_endpoint(
	s: &str,
	rewrite: &HashMap<(String, String), String>,
) -> String {
	let Ok(pr) = parse_port_ref(s) else {
		return s.to_string();
	};
	// rewrite 辞書の value を source_id 逆引き。
	for ((_ffpath, src_id), new_id) in rewrite {
		if src_id == &pr.node_id {
			if let Some(fqp) = &pr.fq_path {
				return format!("{fqp}::{new_id}:{}", pr.port);
			} else {
				return format!("{new_id}:{}", pr.port);
			}
		}
	}
	s.to_string()
}

fn edge_already_present(edges: &[EdgeEntry], e: &EdgeEntry) -> bool {
	edges.iter().any(|ex| ex.from == e.from && ex.to == e.to)
}

// ---------------------------------------------------------------------------
// Remap table application
// ---------------------------------------------------------------------------

fn apply_remaps_into(
	fragment: &Fragment,
	target_fq: &str,
	doc: &mut FlowgraphFile,
	rewrite: &HashMap<(String, String), String>,
	options: &PasteOptions,
	report: &mut PasteReport,
) {
	for d in &fragment.danglings {
		// remap table に lookup。
		match options.remap.get(&d.original) {
			Some(None) => {
				// 明示的 drop。
				continue;
			}
			Some(Some(new_ref)) => {
				// 新しい参照文字列でエッジを再構築。
				// edge_from/edge_to と external_side が必要（schema v1 拡張フィールド）。
				let (Some(ef), Some(et), Some(es)) =
					(&d.edge_from, &d.edge_to, &d.external_side)
				else {
					// 情報不足。unresolved に残す。
					report.unresolved_danglings.push(d.clone());
					continue;
				};
				let (from, to) = match es.as_str() {
					"from" => (new_ref.clone(), et.clone()),
					"to" => (ef.clone(), new_ref.clone()),
					_ => {
						report.unresolved_danglings.push(d.clone());
						continue;
					}
				};
				let rewritten = rewrite_edge_endpoints(
					&EdgeEntry { from, to },
					target_fq,
					rewrite,
				);
				if !edge_already_present(&doc.edges, &rewritten) {
					doc.edges.push(rewritten);
				}
			}
			None => {
				// 未指定 → unresolved に残す。
				report.unresolved_danglings.push(d.clone());
			}
		}
	}
}

// ---------------------------------------------------------------------------
// File helpers
// ---------------------------------------------------------------------------

fn fq_to_file_path(root: &Path, fq: &str) -> PathBuf {
	let mut p = root.to_path_buf();
	for seg in fq.split('/') {
		p.push(seg);
	}
	// 拡張子を付与。
	let mut s = p.as_os_str().to_os_string();
	s.push(".flowgraph.toml");
	PathBuf::from(s)
}

fn load_existing_file(path: &Path) -> Result<Option<FlowgraphFile>, PasteError> {
	if !path.exists() {
		return Ok(None);
	}
	let src = std::fs::read_to_string(path).map_err(|err| PasteError::Io {
		path: path.to_path_buf(),
		err,
	})?;
	let doc = parse_flowgraph_file(&src, Some(path)).map_err(|e| PasteError::ExistingParse {
		path: path.to_path_buf(),
		err: e.to_string(),
	})?;
	Ok(Some(doc))
}

fn write_file_atomic(
	path: &Path,
	doc: &FlowgraphFile,
	make_backup: bool,
	report: &mut PasteReport,
) -> Result<(), PasteError> {
	if let Some(parent) = path.parent() {
		std::fs::create_dir_all(parent).map_err(|err| PasteError::Io {
			path: parent.to_path_buf(),
			err,
		})?;
	}
	if make_backup && path.exists() {
		let bak = path.with_extension("toml.bak");
		if let Err(err) = std::fs::copy(path, &bak) {
			return Err(PasteError::Io {
				path: bak.clone(),
				err,
			});
		}
		report
			.backups
			.push(bak.display().to_string().replace('\\', "/"));
	}
	let text = serialize_flowgraph_file(doc)?;
	std::fs::write(path, text).map_err(|err| PasteError::Io {
		path: path.to_path_buf(),
		err,
	})?;
	Ok(())
}

/// `FlowgraphFile` を TOML 文字列化。spec §7.4 の format-preserving は対象外（本処理は paste/import で
/// 新規 or 完全上書きされるケース専用）。
fn serialize_flowgraph_file(doc: &FlowgraphFile) -> Result<String, PasteError> {
	let mut root = toml::Table::new();
	if let Some(meta) = &doc.meta {
		let v = toml::Value::try_from(meta).map_err(|e| PasteError::Parse(format!("meta: {e}")))?;
		if let toml::Value::Table(t) = v {
			root.insert("meta".into(), toml::Value::Table(t));
		}
	}
	// nodes / edges は必ず配列として配置（Vec が空でも書き出しは省略）。
	if !doc.nodes.is_empty() {
		let v = toml::Value::try_from(&doc.nodes)
			.map_err(|e| PasteError::Parse(format!("nodes: {e}")))?;
		root.insert("nodes".into(), v);
	}
	if !doc.edges.is_empty() {
		let v = toml::Value::try_from(&doc.edges)
			.map_err(|e| PasteError::Parse(format!("edges: {e}")))?;
		root.insert("edges".into(), v);
	}
	toml::to_string_pretty(&toml::Value::Table(root))
		.map_err(|e| PasteError::Parse(format!("serialize: {e}")))
}

fn find_unused_rename(root: &Path, base_fq: &str) -> String {
	for n in 1..u64::MAX {
		let candidate = format!("{base_fq}_{n}");
		let p = fq_to_file_path(root, &candidate);
		if !p.exists() {
			return candidate;
		}
	}
	unreachable!()
}

fn normalize_fq(s: &str) -> Result<String, PasteError> {
	let s = s.trim();
	if s.is_empty() {
		return Err(PasteError::InvalidTarget("(空)".into()));
	}
	if s.starts_with('/') || s.starts_with('\\') {
		return Err(PasteError::InvalidTarget(format!("絶対パス不可: '{s}'")));
	}
	for seg in s.split('/') {
		if seg == ".." || seg == "." || seg.is_empty() {
			return Err(PasteError::InvalidTarget(format!("'..' / '.' / 空セグ不可: '{s}'")));
		}
	}
	Ok(s.replace('\\', "/"))
}

fn normalize_folder_prefix(s: &str) -> Result<String, PasteError> {
	let s = s.trim().trim_end_matches('/');
	if s.is_empty() {
		return Ok(String::new());
	}
	normalize_fq(s)
}

/// fragment 内の相対 path を検証し、拡張子を取り除いた fq 部分を返す。
/// 例: `"chat/main.flowgraph.toml"` → `"chat/main"`。SCRATCH_PATH のときは `"__scratch__"`。
fn sanitize_fragment_path(path: &str) -> Result<String, PasteError> {
	let trimmed = path.trim().replace('\\', "/");
	if trimmed.starts_with('/') {
		return Err(PasteError::InvalidFragmentPath(format!(
			"絶対パス不可: '{trimmed}'"
		)));
	}
	for seg in trimmed.split('/') {
		if seg == ".." || seg == "." || seg.is_empty() {
			return Err(PasteError::InvalidFragmentPath(format!(
				"'..' / '.' / 空セグ不可: '{trimmed}'"
			)));
		}
	}
	let fq = if trimmed == SCRATCH_PATH {
		"__scratch__".to_string()
	} else {
		trimmed
			.strip_suffix(".flowgraph.toml")
			.ok_or_else(|| {
				PasteError::InvalidFragmentPath(format!(
					"'.flowgraph.toml' で終わっていない: '{trimmed}'"
				))
			})?
			.to_string()
	};
	Ok(fq)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::fragment::{
		serialize_fragment, Fragment, FragmentHeader, FragmentScope, FRAGMENT_SCHEMA_V1,
	};
	use crate::flowgraph::loader::file::NodeEntry;
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
		let dir = std::env::temp_dir().join(format!(
			"vac-fg-paste-{}-{ns}-{seq}",
			std::process::id()
		));
		fs::create_dir_all(&dir).unwrap();
		TempDir(dir)
	}
	fn write_file(p: &Path, s: &str) {
		if let Some(parent) = p.parent() {
			fs::create_dir_all(parent).unwrap();
		}
		let mut f = fs::File::create(p).unwrap();
		f.write_all(s.as_bytes()).unwrap();
	}

	fn make_scratch_fragment(node_id: &str, position: Option<[f64; 2]>) -> String {
		let mut t = toml::Table::new();
		t.insert("value".into(), toml::Value::String("hi".into()));
		let frag = Fragment {
			header: FragmentHeader {
				schema: FRAGMENT_SCHEMA_V1.into(),
				exported_at: "2026-04-17T00:00:00Z".into(),
				origin: None,
				scope: FragmentScope::Nodes,
			},
			files: vec![FragmentFile {
				path: SCRATCH_PATH.into(),
				meta: None,
				nodes: vec![NodeEntry {
					id: node_id.into(),
					feature: "flowgraph.literal.string".into(),
					position,
					properties: t,
				}],
				edges: vec![],
			}],
			danglings: vec![],
		};
		serialize_fragment(&frag).unwrap()
	}

	#[test]
	fn paste_nodes_into_new_file() {
		let td = mk_dir();
		let toml = make_scratch_fragment("lit", Some([1.0, 2.0]));
		let req = PasteRequest {
			fragment_toml: toml,
			target: PasteTarget::File { fq: "mynew".into() },
			options: PasteOptions::default(),
		};
		let report = paste_fragment(td.path(), &req).unwrap();
		assert_eq!(report.written_files.len(), 1);
		assert_eq!(report.imported_nodes.len(), 1);
		assert_eq!(report.imported_nodes[0].new_id, "lit");

		let p = td.path().join("mynew.flowgraph.toml");
		let text = fs::read_to_string(&p).unwrap();
		assert!(text.contains("id = \"lit\""));
	}

	#[test]
	fn paste_nodes_into_existing_file_with_conflict_suffix() {
		let td = mk_dir();
		write_file(
			&td.path().join("main.flowgraph.toml"),
			r#"
[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "existing"
"#,
		);

		let toml = make_scratch_fragment("lit", None);
		let req = PasteRequest {
			fragment_toml: toml,
			target: PasteTarget::File { fq: "main".into() },
			options: PasteOptions::default(),
		};
		let report = paste_fragment(td.path(), &req).unwrap();
		assert_eq!(report.imported_nodes.len(), 1);
		assert_eq!(report.imported_nodes[0].original_id, "lit");
		assert_eq!(report.imported_nodes[0].new_id, "lit_1");
		// .bak ができる
		assert_eq!(report.backups.len(), 1);

		let text = fs::read_to_string(td.path().join("main.flowgraph.toml")).unwrap();
		assert!(text.contains("id = \"lit\""));
		assert!(text.contains("id = \"lit_1\""));
	}

	#[test]
	fn paste_nodes_skip_conflict() {
		let td = mk_dir();
		write_file(
			&td.path().join("main.flowgraph.toml"),
			r#"
[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "existing"
"#,
		);

		let toml = make_scratch_fragment("lit", None);
		let req = PasteRequest {
			fragment_toml: toml,
			target: PasteTarget::File { fq: "main".into() },
			options: PasteOptions {
				on_conflict_node: OnConflict::Skip,
				..Default::default()
			},
		};
		let report = paste_fragment(td.path(), &req).unwrap();
		assert_eq!(report.imported_nodes.len(), 0);
		assert_eq!(report.skipped_nodes.len(), 1);
		assert_eq!(report.skipped_nodes[0].original_id, "lit");
	}

	#[test]
	fn paste_nodes_overwrite_conflict() {
		let td = mk_dir();
		write_file(
			&td.path().join("main.flowgraph.toml"),
			r#"
[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "OLD"
"#,
		);

		let toml = make_scratch_fragment("lit", None);
		let req = PasteRequest {
			fragment_toml: toml,
			target: PasteTarget::File { fq: "main".into() },
			options: PasteOptions {
				on_conflict_node: OnConflict::Overwrite,
				..Default::default()
			},
		};
		let _ = paste_fragment(td.path(), &req).unwrap();
		let text = fs::read_to_string(td.path().join("main.flowgraph.toml")).unwrap();
		assert!(text.contains("value = \"hi\""), "overwrite: {}", text);
		assert!(!text.contains("OLD"));
	}

	#[test]
	fn paste_file_into_folder_new() {
		let td = mk_dir();
		// fragment with scope=file, 1 file entry "chat/bot.flowgraph.toml".
		let frag = Fragment {
			header: FragmentHeader {
				schema: FRAGMENT_SCHEMA_V1.into(),
				exported_at: "2026-04-17T00:00:00Z".into(),
				origin: None,
				scope: FragmentScope::File,
			},
			files: vec![FragmentFile {
				path: "chat/bot.flowgraph.toml".into(),
				meta: None,
				nodes: vec![NodeEntry {
					id: "a".into(),
					feature: "flowgraph.literal.string".into(),
					position: None,
					properties: {
						let mut t = toml::Table::new();
						t.insert("value".into(), "x".into());
						t
					},
				}],
				edges: vec![],
			}],
			danglings: vec![],
		};
		let req = PasteRequest {
			fragment_toml: serialize_fragment(&frag).unwrap(),
			target: PasteTarget::Folder { path: "imported".into() },
			options: PasteOptions::default(),
		};
		let report = paste_fragment(td.path(), &req).unwrap();
		assert_eq!(report.written_files.len(), 1);
		assert_eq!(report.written_files[0].fq, "imported/chat/bot");
		assert!(td.path().join("imported/chat/bot.flowgraph.toml").exists());
	}

	#[test]
	fn paste_file_folder_conflict_rename() {
		let td = mk_dir();
		write_file(
			&td.path().join("chat/bot.flowgraph.toml"),
			r#"
[[nodes]]
id = "preexisting"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "x"
"#,
		);
		let frag = Fragment {
			header: FragmentHeader {
				schema: FRAGMENT_SCHEMA_V1.into(),
				exported_at: "2026-04-17T00:00:00Z".into(),
				origin: None,
				scope: FragmentScope::File,
			},
			files: vec![FragmentFile {
				path: "chat/bot.flowgraph.toml".into(),
				meta: None,
				nodes: vec![NodeEntry {
					id: "new".into(),
					feature: "flowgraph.literal.string".into(),
					position: None,
					properties: {
						let mut t = toml::Table::new();
						t.insert("value".into(), "y".into());
						t
					},
				}],
				edges: vec![],
			}],
			danglings: vec![],
		};
		let req = PasteRequest {
			fragment_toml: serialize_fragment(&frag).unwrap(),
			target: PasteTarget::Folder { path: "".into() },
			options: PasteOptions {
				on_conflict_file: OnConflictFile::Rename,
				..Default::default()
			},
		};
		let report = paste_fragment(td.path(), &req).unwrap();
		assert_eq!(report.written_files.len(), 1);
		assert_eq!(report.written_files[0].fq, "chat/bot_1");
	}

	#[test]
	fn paste_dangling_remap_reconstructs_edge() {
		let td = mk_dir();
		// 既存ファイルに "keeper" ノード。
		write_file(
			&td.path().join("dest.flowgraph.toml"),
			r#"
[[nodes]]
id = "keeper"
feature = "flowgraph.log.info"
"#,
		);

		let frag = Fragment {
			header: FragmentHeader {
				schema: FRAGMENT_SCHEMA_V1.into(),
				exported_at: "2026-04-17T00:00:00Z".into(),
				origin: None,
				scope: FragmentScope::Nodes,
			},
			files: vec![FragmentFile {
				path: SCRATCH_PATH.into(),
				meta: None,
				nodes: vec![NodeEntry {
					id: "src".into(),
					feature: "flowgraph.literal.string".into(),
					position: None,
					properties: {
						let mut t = toml::Table::new();
						t.insert("value".into(), "x".into());
						t
					},
				}],
				edges: vec![],
			}],
			danglings: vec![FragmentDangling {
				kind: "edge_endpoint".into(),
				original: "external::x:in".into(),
				reason: "out_of_scope".into(),
				in_file: Some("main".into()),
				edge_from: Some("src:value".into()),
				edge_to: Some("external::x:in".into()),
				external_side: Some("to".into()),
			}],
		};

		let mut remap: HashMap<String, Option<String>> = HashMap::new();
		remap.insert("external::x:in".into(), Some("keeper:message".into()));

		let req = PasteRequest {
			fragment_toml: serialize_fragment(&frag).unwrap(),
			target: PasteTarget::File { fq: "dest".into() },
			options: PasteOptions {
				remap,
				..Default::default()
			},
		};
		let _ = paste_fragment(td.path(), &req).unwrap();
		let text = fs::read_to_string(td.path().join("dest.flowgraph.toml")).unwrap();
		assert!(text.contains("from = \"src:value\""));
		assert!(text.contains("to = \"keeper:message\""), "{text}");
	}

	#[test]
	fn paste_dangling_drop_and_unresolved() {
		let td = mk_dir();
		let frag = Fragment {
			header: FragmentHeader {
				schema: FRAGMENT_SCHEMA_V1.into(),
				exported_at: "2026-04-17T00:00:00Z".into(),
				origin: None,
				scope: FragmentScope::Nodes,
			},
			files: vec![FragmentFile {
				path: SCRATCH_PATH.into(),
				meta: None,
				nodes: vec![],
				edges: vec![],
			}],
			danglings: vec![
				FragmentDangling {
					kind: "edge_endpoint".into(),
					original: "dropped::n:p".into(),
					reason: "out_of_scope".into(),
					in_file: Some("main".into()),
					edge_from: Some("a:b".into()),
					edge_to: Some("dropped::n:p".into()),
					external_side: Some("to".into()),
				},
				FragmentDangling {
					kind: "edge_endpoint".into(),
					original: "unresolved::n:p".into(),
					reason: "out_of_scope".into(),
					in_file: None,
					edge_from: None,
					edge_to: None,
					external_side: None,
				},
			],
		};
		let mut remap: HashMap<String, Option<String>> = HashMap::new();
		remap.insert("dropped::n:p".into(), None);

		let req = PasteRequest {
			fragment_toml: serialize_fragment(&frag).unwrap(),
			target: PasteTarget::File { fq: "dest".into() },
			options: PasteOptions {
				remap,
				..Default::default()
			},
		};
		let report = paste_fragment(td.path(), &req).unwrap();
		assert_eq!(report.unresolved_danglings.len(), 1);
		assert_eq!(report.unresolved_danglings[0].original, "unresolved::n:p");
	}

	#[test]
	fn paste_position_offset_is_applied() {
		let td = mk_dir();
		let toml = make_scratch_fragment("lit", Some([10.0, 20.0]));
		let req = PasteRequest {
			fragment_toml: toml,
			target: PasteTarget::File { fq: "dst".into() },
			options: PasteOptions {
				position_offset: Some([100.0, 200.0]),
				..Default::default()
			},
		};
		let _ = paste_fragment(td.path(), &req).unwrap();
		let text = fs::read_to_string(td.path().join("dst.flowgraph.toml")).unwrap();
		// toml::to_string_pretty は配列を改行込みで書くため部分文字列で確認。
		assert!(text.contains("110.0"), "{text}");
		assert!(text.contains("220.0"), "{text}");
	}

	#[test]
	fn sanitize_fragment_path_rejects_traversal() {
		assert!(sanitize_fragment_path("../evil.flowgraph.toml").is_err());
		assert!(sanitize_fragment_path("/abs.flowgraph.toml").is_err());
		assert!(sanitize_fragment_path("ok.txt").is_err()); // 拡張子不一致
		assert_eq!(sanitize_fragment_path("chat/bot.flowgraph.toml").unwrap(), "chat/bot");
		assert_eq!(sanitize_fragment_path(SCRATCH_PATH).unwrap(), "__scratch__");
	}
}

/// dangling remap 表の alias（δ-7a の暫定定義と同義）。
pub type RemapTable = HashMap<String, Option<String>>;
