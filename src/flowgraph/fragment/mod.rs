//! Phase δ-7: Fragment codec（spec §9）。
//!
//! Flowgraph ファイル群の部分集合を「コピー / ペースト」「エクスポート / インポート」するための
//! TOML ベースの可搬フォーマットを扱う。GUI の copy/paste、ZIP export/import、コミュニティ共有の
//! 基盤となる。
//!
//! ## モジュール構成
//!
//! - [`Fragment`] / 周辺型: spec §9.2 の TOML スキーマに 1:1 対応する serde 型。
//! - [`copy_targets`]: 既存 flowgraph ディレクトリから fragment を組み立てる（spec §9.4 copy）。
//! - [`parse_fragment`] / [`serialize_fragment`]: TOML 文字列との相互変換。
//! - [`paste_fragment`]: fragment をターゲットディレクトリに書き戻す（spec §9.4 paste、δ-7b）。
//!
//! ## 非スコープ
//!
//! - ZIP フォーマット（spec §9.3）は δ-7d で別モジュール（`zip_codec.rs`）に実装する。
//! - GUI 側の dangling UI（再マッピング）は fragment の `danglings` を pull して独自描画するため、
//!   本モジュールは **検出までを責務とし、再マッピング結果の適用は paste 呼び出し側** が担う。

use crate::flowgraph::loader::{
	dir::{fq_path_of_file, walk_flowgraph_dir},
	file::{parse_flowgraph_file, EdgeEntry, FileMeta, FlowgraphFile, NodeEntry},
	reference::{parse_port_ref, resolve_fq_ref, ResolveContext},
};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

pub mod paste;
pub mod zip_codec;

/// schema identifier。互換破壊時に major bump する。spec §9.2 に記載。
pub const FRAGMENT_SCHEMA_V1: &str = "vac-flowgraph-fragment/v1";

/// scope=nodes の便宜 path。paste 時に「どのファイルにまとめるか」はターゲットが指定する。
pub const SCRATCH_PATH: &str = "__scratch__.flowgraph.toml";

// ---------------------------------------------------------------------------
// TOML schema（spec §9.2）
// ---------------------------------------------------------------------------

/// 選択範囲の種類。spec §9.1 に対応。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum FragmentScope {
	Nodes,
	File,
	Folder,
	Mixed,
}

/// Fragment TOML のトップレベル。`[fragment]` 以下をまるごと保持。
///
/// TOML 表現例は spec §9.2 参照。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fragment {
	#[serde(rename = "fragment")]
	pub header: FragmentHeader,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	#[serde(rename = "fragment.files")]
	pub files: Vec<FragmentFile>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	#[serde(rename = "fragment.danglings")]
	pub danglings: Vec<FragmentDangling>,
}

/// `[fragment]` セクション。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentHeader {
	pub schema: String,
	pub exported_at: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub origin: Option<String>,
	pub scope: FragmentScope,
}

/// `[[fragment.files]]` の 1 要素。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentFile {
	/// root からの相対パス。scope=nodes のときは `SCRATCH_PATH` を使う。
	/// 末尾は常に `.flowgraph.toml`。
	pub path: String,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub meta: Option<FileMeta>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub nodes: Vec<NodeEntry>,
	#[serde(default, skip_serializing_if = "Vec::is_empty")]
	pub edges: Vec<EdgeEntry>,
}

/// `[[fragment.danglings]]` の 1 要素。選択範囲外へ出ていたエッジ端の記録。
///
/// spec §9.2 の `kind` / `original` / `reason` を必須、paste 時の再マッピング補助に使う
/// `edge_from` / `edge_to` / `in_file` / `external_side` を optional 拡張として持つ
/// （schema v1 の範囲で後方互換な追加。読み書きとも 1:1 ラウンドトリップする）。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FragmentDangling {
	/// 現時点では `"edge_endpoint"` のみ。他種別を増やす余地を残すため string。
	pub kind: String,
	/// 元の参照文字列（例: `"../../ingress/twitch::main:content"`）。
	pub original: String,
	/// 理由コード（`"out_of_scope"` / `"unresolved_ref"` / `"unparseable_ref"` 等）。
	pub reason: String,
	/// どの fragment ファイルから切り出されたエッジか（paste で元ファイルに戻すための anchor）。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub in_file: Option<String>,
	/// 元エッジの `from` 文字列（fragment 内側 / 外側どちらも含む生値）。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub edge_from: Option<String>,
	/// 元エッジの `to` 文字列。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub edge_to: Option<String>,
	/// 外側にあった端点の位置。`"from"` or `"to"`。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub external_side: Option<String>,
}

// ---------------------------------------------------------------------------
// Copy API
// ---------------------------------------------------------------------------

/// copy 要求ターゲット。spec §9.4 POST /fragment/copy の `targets[]` に対応。
#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum CopyTarget {
	/// 個別ノード。`fq_file` はファイルの fq（拡張子なし）、`node_id` はそのファイル内のノード ID。
	Node { fq_file: String, node_id: String },
	/// ファイル 1 本をまるごと。
	File { fq: String },
	/// フォルダ配下を再帰的に。`path` は `/` 区切り fq prefix（拡張子なし）。空文字列でルート全体。
	Folder { path: String },
}

/// copy 要求。
#[derive(Debug, Clone, Deserialize)]
pub struct CopyRequest {
	/// targets から自動判定するのが本来望ましいが、GUI 側でわざと "mixed" を強制できるように
	/// リクエストで受け取る（UI の「これは混合選択である」を尊重する）。
	pub scope: FragmentScope,
	pub targets: Vec<CopyTarget>,
	/// エクスポート元の目印。省略可。
	#[serde(default)]
	pub origin: Option<String>,
}

/// copy エラー。spec §9.4 の 400/404 相当。
#[derive(Debug, thiserror::Error)]
pub enum CopyError {
	#[error("flowgraph root が存在しない: {0}")]
	RootMissing(PathBuf),
	#[error("targets が空")]
	NoTargets,
	#[error("参照したファイルが見つからない: {0}")]
	FileNotFound(String),
	#[error("参照したノードが見つからない: fq_file={fq_file} node_id={node_id}")]
	NodeNotFound { fq_file: String, node_id: String },
	#[error("ファイル読み込み失敗: {path}: {err}")]
	Io { path: PathBuf, err: std::io::Error },
	#[error("TOML パース失敗: {0}")]
	Parse(String),
	#[error("targets に不正な fq: {0}")]
	InvalidFq(String),
}

// ---------------------------------------------------------------------------
// Core copy implementation
// ---------------------------------------------------------------------------

/// 選択ターゲットを fragment に切り出す。
///
/// 処理概要:
///   1. targets を展開して「含めるノード集合」を作る:
///      - `File { fq }` → その fq の全ノード。
///      - `Folder { path }` → prefix match する全ファイルの全ノード。
///      - `Node { fq_file, node_id }` → そのペアのみ。
///   2. scope=nodes のときは `SCRATCH_PATH` 1 本に合流、それ以外はファイル別に保持。
///   3. 各ファイルの edges を走査し:
///      - 両端点のノードが selection 集合に入っていれば fragment.files.edges に採録。
///      - 片方が外側なら fragment.danglings に記録（もう片方を参照文字列として保存）。
///      - 両方外側は完全に fragment 外のエッジなので捨てる。
///
/// 同一 `(fq_file, node_id)` が複数 target に出てきても重複排除する。
pub fn copy_targets(root: &Path, req: &CopyRequest) -> Result<Fragment, CopyError> {
	if !root.exists() {
		return Err(CopyError::RootMissing(root.to_path_buf()));
	}
	if req.targets.is_empty() {
		return Err(CopyError::NoTargets);
	}

	// (1) 全 .flowgraph.toml ファイルを読み出してキャッシュ。
	//     copy はスナップショット操作なので、runtime の node_meta ではなく**ディスク上の実ファイル**を
	//     真実とする（GUI 編集中 draft との齟齬を避ける: draft はまだ save されていない可能性がある）。
	let loaded = load_all_files(root)?;

	// (2) targets → 選択集合 + ファイル白リスト。
	let mut selected_nodes: BTreeMap<String, BTreeSet<String>> = BTreeMap::new(); // fq_file → node_ids
	let mut whitelisted_files: BTreeSet<String> = BTreeSet::new(); // scope=file/folder で "全体含める" ファイル
	for t in &req.targets {
		match t {
			CopyTarget::Node { fq_file, node_id } => {
				let fq = normalize_fq_str(fq_file)?;
				let file = loaded
					.get(&fq)
					.ok_or_else(|| CopyError::FileNotFound(fq.clone()))?;
				if !file.doc.nodes.iter().any(|n| &n.id == node_id) {
					return Err(CopyError::NodeNotFound {
						fq_file: fq,
						node_id: node_id.clone(),
					});
				}
				selected_nodes.entry(fq).or_default().insert(node_id.clone());
			}
			CopyTarget::File { fq } => {
				let fq = normalize_fq_str(fq)?;
				let file = loaded
					.get(&fq)
					.ok_or_else(|| CopyError::FileNotFound(fq.clone()))?;
				let ids = selected_nodes.entry(fq.clone()).or_default();
				for n in &file.doc.nodes {
					ids.insert(n.id.clone());
				}
				whitelisted_files.insert(fq);
			}
			CopyTarget::Folder { path } => {
				let prefix = normalize_folder_prefix(path)?;
				let mut any = false;
				// prefix が空なら全ファイル対象、それ以外は `prefix/` で始まる fq を対象。
				for (fq, file) in &loaded {
					let matches = if prefix.is_empty() {
						true
					} else {
						fq == &prefix || fq.starts_with(&format!("{prefix}/"))
					};
					if !matches {
						continue;
					}
					any = true;
					let ids = selected_nodes.entry(fq.clone()).or_default();
					for n in &file.doc.nodes {
						ids.insert(n.id.clone());
					}
					whitelisted_files.insert(fq.clone());
				}
				if !any {
					return Err(CopyError::FileNotFound(format!("(folder) {prefix}")));
				}
			}
		}
	}

	// (3) edges 走査。file/folder 由来か node 由来かで dangling 扱いが変わる:
	//     - whitelisted_files のファイル内 edge は両端が selection 内外かをフルチェック
	//       （scope=file でも出て行くエッジは dangling）。
	//     - scope=nodes のみの場合、edge は所属ファイル内で対象ノードが両端かを見る。
	let known_fqs: HashSet<String> = loaded.keys().cloned().collect();
	let mut out_files: BTreeMap<String, FragmentFile> = BTreeMap::new();
	let mut danglings: Vec<FragmentDangling> = Vec::new();

	// scope に応じた「仮想ファイルへ集約するか」判定。
	let nodes_only = matches!(req.header_scope(), FragmentScope::Nodes);

	for (fq, selected_ids) in &selected_nodes {
		let file = loaded
			.get(fq)
			.ok_or_else(|| CopyError::FileNotFound(fq.clone()))?;

		// 対象ノードをコピー（properties は toml::Table そのまま）。
		let mut nodes_out: Vec<NodeEntry> = Vec::new();
		for n in &file.doc.nodes {
			if selected_ids.contains(&n.id) {
				nodes_out.push(n.clone());
			}
		}

		// エッジ走査。
		let mut edges_out: Vec<EdgeEntry> = Vec::new();
		let ctx = ResolveContext {
			current_file_fq: fq.clone(),
			known_file_fqs: known_fqs.clone(),
		};
		for e in &file.doc.edges {
			let from_res = resolve_endpoint(&e.from, &ctx);
			let to_res = resolve_endpoint(&e.to, &ctx);
			let from_in = from_res
				.as_ref()
				.ok()
				.map(|(ffq, nid)| is_selected(&selected_nodes, ffq, nid))
				.unwrap_or(false);
			let to_in = to_res
				.as_ref()
				.ok()
				.map(|(tfq, nid)| is_selected(&selected_nodes, tfq, nid))
				.unwrap_or(false);

			match (from_in, to_in) {
				(true, true) => {
					// 完全に fragment 内。
					edges_out.push(e.clone());
				}
				(true, false) => {
					danglings.push(FragmentDangling {
						kind: "edge_endpoint".to_string(),
						original: e.to.clone(),
						reason: reason_for(&to_res),
						in_file: Some(fq.clone()),
						edge_from: Some(e.from.clone()),
						edge_to: Some(e.to.clone()),
						external_side: Some("to".to_string()),
					});
				}
				(false, true) => {
					danglings.push(FragmentDangling {
						kind: "edge_endpoint".to_string(),
						original: e.from.clone(),
						reason: reason_for(&from_res),
						in_file: Some(fq.clone()),
						edge_from: Some(e.from.clone()),
						edge_to: Some(e.to.clone()),
						external_side: Some("from".to_string()),
					});
				}
				(false, false) => {
					// どちらも外側 → 捨てる（fragment に関係のないエッジ）。
				}
			}
		}

		let target_path = if nodes_only {
			SCRATCH_PATH.to_string()
		} else {
			format!("{fq}.flowgraph.toml")
		};
		let entry = out_files.entry(target_path.clone()).or_insert_with(|| FragmentFile {
			path: target_path,
			meta: if nodes_only {
				Some(FileMeta {
					title: Some("Copied nodes".to_string()),
					description: None,
					tags: None,
				})
			} else {
				file.doc.meta.clone()
			},
			nodes: Vec::new(),
			edges: Vec::new(),
		});
		entry.nodes.extend(nodes_out);
		entry.edges.extend(edges_out);
	}

	// dangling の重複排除（同じ original / reason が複数 edge から拾われる場合がある）。
	danglings.sort_by(|a, b| a.original.cmp(&b.original).then(a.reason.cmp(&b.reason)));
	danglings.dedup_by(|a, b| a.original == b.original && a.reason == b.reason && a.kind == b.kind);

	let header = FragmentHeader {
		schema: FRAGMENT_SCHEMA_V1.to_string(),
		exported_at: now_rfc3339(),
		origin: req.origin.clone(),
		scope: req.scope,
	};

	Ok(Fragment {
		header,
		files: out_files.into_values().collect(),
		danglings,
	})
}

// ---------------------------------------------------------------------------
// Serialize / Parse
// ---------------------------------------------------------------------------

/// TOML 文字列化。`toml::to_string_pretty` を使い、GUI が scratch ファイルをそのまま見ても
/// 読める見た目にする。
pub fn serialize_fragment(fragment: &Fragment) -> Result<String, String> {
	// `toml::to_string_pretty` は `Fragment` 直下のフラット化（`fragment.files` などの `.` 付き
	// key を正しく `[[fragment.files]]` にレンダリングする）ため、`FragmentRender` 経由で一旦
	// Value に変換する。
	let value = to_toml_value(fragment)?;
	toml::to_string_pretty(&value).map_err(|e| format!("serialize failed: {e}"))
}

/// TOML 文字列からパース。
pub fn parse_fragment(src: &str) -> Result<Fragment, String> {
	let mut value: toml::Value = toml::from_str(src).map_err(|e| format!("parse failed: {e}"))?;

	let table = value
		.as_table_mut()
		.ok_or_else(|| "ルートが table でない".to_string())?;

	// `[fragment]` / `[fragment.files]` / `[fragment.danglings]` を剥がして Fragment に組み直す。
	let fragment_val = table
		.remove("fragment")
		.ok_or_else(|| "[fragment] セクションが無い".to_string())?;
	let mut ftable = match fragment_val {
		toml::Value::Table(t) => t,
		_ => return Err("[fragment] が table でない".to_string()),
	};

	let files_val = ftable.remove("files").unwrap_or(toml::Value::Array(vec![]));
	let danglings_val = ftable.remove("danglings").unwrap_or(toml::Value::Array(vec![]));

	let header: FragmentHeader = toml::Value::Table(ftable)
		.try_into()
		.map_err(|e| format!("[fragment] header パース失敗: {e}"))?;
	let files: Vec<FragmentFile> = files_val
		.try_into()
		.map_err(|e| format!("[[fragment.files]] パース失敗: {e}"))?;
	let danglings: Vec<FragmentDangling> = danglings_val
		.try_into()
		.map_err(|e| format!("[[fragment.danglings]] パース失敗: {e}"))?;

	if header.schema != FRAGMENT_SCHEMA_V1 {
		return Err(format!(
			"schema が未対応: '{}'（期待: '{}'）",
			header.schema, FRAGMENT_SCHEMA_V1
		));
	}

	Ok(Fragment {
		header,
		files,
		danglings,
	})
}

/// 内部用: Fragment を `toml::Value` に変換して、spec §9.2 どおりの table 構造で書き出す。
fn to_toml_value(fragment: &Fragment) -> Result<toml::Value, String> {
	let mut frag_tbl = toml::Table::new();

	// header
	let header_val = toml::Value::try_from(&fragment.header)
		.map_err(|e| format!("header の値化失敗: {e}"))?;
	if let toml::Value::Table(h) = header_val {
		for (k, v) in h {
			frag_tbl.insert(k, v);
		}
	}
	// files
	let files_val = toml::Value::try_from(&fragment.files).map_err(|e| format!("files の値化失敗: {e}"))?;
	frag_tbl.insert("files".to_string(), files_val);
	// danglings
	let dangs_val =
		toml::Value::try_from(&fragment.danglings).map_err(|e| format!("danglings の値化失敗: {e}"))?;
	frag_tbl.insert("danglings".to_string(), dangs_val);

	let mut root = toml::Table::new();
	root.insert("fragment".to_string(), toml::Value::Table(frag_tbl));
	Ok(toml::Value::Table(root))
}

impl CopyRequest {
	fn header_scope(&self) -> FragmentScope {
		self.scope
	}
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

struct LoadedOnDisk {
	#[allow(dead_code)]
	path: PathBuf,
	doc: FlowgraphFile,
}

/// ディスク上の全 `.flowgraph.toml` を fq → doc で読み込む。
fn load_all_files(root: &Path) -> Result<BTreeMap<String, LoadedOnDisk>, CopyError> {
	let paths = walk_flowgraph_dir(root).map_err(|err| CopyError::Io {
		path: root.to_path_buf(),
		err,
	})?;
	let mut out: BTreeMap<String, LoadedOnDisk> = BTreeMap::new();
	for p in paths {
		let src = std::fs::read_to_string(&p).map_err(|err| CopyError::Io {
			path: p.clone(),
			err,
		})?;
		let doc = parse_flowgraph_file(&src, Some(&p))
			.map_err(|e| CopyError::Parse(format!("{}: {}", p.display(), e)))?;
		let fq = fq_path_of_file(root, &p).map_err(CopyError::InvalidFq)?;
		out.insert(fq, LoadedOnDisk { path: p, doc });
	}
	Ok(out)
}

/// `fq_file` 形式（`"tts/main"` など、拡張子なし・`/` 区切り）を検証 + 正規化。
fn normalize_fq_str(s: &str) -> Result<String, CopyError> {
	let s = s.trim();
	if s.is_empty() {
		return Err(CopyError::InvalidFq("(空文字列)".to_string()));
	}
	if s.starts_with('/') || s.starts_with('\\') {
		return Err(CopyError::InvalidFq(format!("絶対パス不可: '{s}'")));
	}
	for seg in s.split('/') {
		if seg == ".." || seg == "." {
			return Err(CopyError::InvalidFq(format!("'..' / '.' 不可: '{s}'")));
		}
		if seg.is_empty() {
			return Err(CopyError::InvalidFq(format!("空セグメント: '{s}'")));
		}
	}
	Ok(s.replace('\\', "/"))
}

/// フォルダ prefix の正規化。空文字列 → ルート全体扱い。
fn normalize_folder_prefix(s: &str) -> Result<String, CopyError> {
	let s = s.trim().trim_end_matches('/');
	if s.is_empty() {
		return Ok(String::new());
	}
	normalize_fq_str(s)
}

/// edge endpoint の `"fq::id:port"` を `(fq, node_id)` に解決する。
fn resolve_endpoint(
	endpoint: &str,
	ctx: &ResolveContext,
) -> Result<(String, String), EndpointError> {
	let pr = parse_port_ref(endpoint).map_err(EndpointError::Parse)?;
	let fq = resolve_fq_ref(&pr, ctx).map_err(EndpointError::Resolve)?;
	Ok((fq, pr.node_id))
}

#[derive(Debug)]
#[allow(dead_code)] // 保持した理由メッセージは将来の GUI 表示／ログで使う。
enum EndpointError {
	Parse(String),
	Resolve(String),
}

fn reason_for(res: &Result<(String, String), EndpointError>) -> String {
	match res {
		Ok(_) => "out_of_scope".to_string(),
		Err(EndpointError::Parse(_)) => "unparseable_ref".to_string(),
		Err(EndpointError::Resolve(_)) => "unresolved_ref".to_string(),
	}
}

fn is_selected(
	selected: &BTreeMap<String, BTreeSet<String>>,
	fq: &str,
	node_id: &str,
) -> bool {
	selected
		.get(fq)
		.map(|ids| ids.contains(node_id))
		.unwrap_or(false)
}

fn now_rfc3339() -> String {
	chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Secs, true)
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use std::fs;
	use std::io::Write;
	use std::sync::atomic::{AtomicU64, Ordering};

	/// `tempfile` crate を入れたくないので manual temp dir（既存 loader テストと同じ方針）。
	struct TempDir {
		path: PathBuf,
	}
	impl TempDir {
		fn path(&self) -> &Path {
			&self.path
		}
	}
	impl Drop for TempDir {
		fn drop(&mut self) {
			let _ = fs::remove_dir_all(&self.path);
		}
	}

	fn write_file(root: &Path, rel: &str, src: &str) {
		let p = root.join(rel);
		if let Some(parent) = p.parent() {
			fs::create_dir_all(parent).unwrap();
		}
		let mut f = fs::File::create(&p).unwrap();
		f.write_all(src.as_bytes()).unwrap();
	}

	fn mk_dir() -> TempDir {
		use std::time::{SystemTime, UNIX_EPOCH};
		static SEQ: AtomicU64 = AtomicU64::new(0);
		let ns = SystemTime::now()
			.duration_since(UNIX_EPOCH)
			.unwrap()
			.as_nanos();
		let seq = SEQ.fetch_add(1, Ordering::Relaxed);
		let dir = std::env::temp_dir().join(format!(
			"vac-fg-fragment-{}-{ns}-{seq}",
			std::process::id()
		));
		fs::create_dir_all(&dir).unwrap();
		TempDir { path: dir }
	}

	#[test]
	fn copy_single_node_into_scratch_file() {
		let td = mk_dir();
		write_file(
			td.path(),
			"main.flowgraph.toml",
			r#"
[meta]
title = "Test"

[[nodes]]
id = "lit"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "hello"

[[nodes]]
id = "log"
feature = "flowgraph.log.info"

[[edges]]
from = "lit:value"
to   = "log:message"
"#,
		);

		let req = CopyRequest {
			scope: FragmentScope::Nodes,
			targets: vec![CopyTarget::Node {
				fq_file: "main".into(),
				node_id: "lit".into(),
			}],
			origin: Some("tests".into()),
		};
		let frag = copy_targets(td.path(), &req).unwrap();
		assert_eq!(frag.header.schema, FRAGMENT_SCHEMA_V1);
		assert_eq!(frag.header.scope, FragmentScope::Nodes);
		assert_eq!(frag.files.len(), 1);
		assert_eq!(frag.files[0].path, SCRATCH_PATH);
		assert_eq!(frag.files[0].nodes.len(), 1);
		assert_eq!(frag.files[0].nodes[0].id, "lit");
		// lit:value → log:message は log が fragment 外 → dangling に出る。
		assert_eq!(frag.danglings.len(), 1, "{:?}", frag.danglings);
		assert_eq!(frag.danglings[0].original, "log:message");
		assert_eq!(frag.danglings[0].reason, "out_of_scope");
	}

	#[test]
	fn copy_whole_file_preserves_meta_and_edges() {
		let td = mk_dir();
		write_file(
			td.path(),
			"chat/main.flowgraph.toml",
			r#"
[meta]
title = "Chat"
description = "d"

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
		let req = CopyRequest {
			scope: FragmentScope::File,
			targets: vec![CopyTarget::File {
				fq: "chat/main".into(),
			}],
			origin: None,
		};
		let frag = copy_targets(td.path(), &req).unwrap();
		assert_eq!(frag.files.len(), 1);
		assert_eq!(frag.files[0].path, "chat/main.flowgraph.toml");
		assert_eq!(frag.files[0].nodes.len(), 2);
		assert_eq!(frag.files[0].edges.len(), 1);
		assert_eq!(frag.files[0].meta.as_ref().unwrap().title.as_deref(), Some("Chat"));
		assert!(frag.danglings.is_empty());
	}

	#[test]
	fn copy_folder_collects_all_descendants() {
		let td = mk_dir();
		write_file(
			td.path(),
			"tts/main.flowgraph.toml",
			r#"
[[nodes]]
id = "main_a"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "x"
"#,
		);
		write_file(
			td.path(),
			"tts/jp.flowgraph.toml",
			r#"
[[nodes]]
id = "jp_a"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "y"
"#,
		);
		write_file(
			td.path(),
			"other/misc.flowgraph.toml",
			r#"
[[nodes]]
id = "misc_a"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "z"
"#,
		);

		let req = CopyRequest {
			scope: FragmentScope::Folder,
			targets: vec![CopyTarget::Folder { path: "tts".into() }],
			origin: None,
		};
		let frag = copy_targets(td.path(), &req).unwrap();
		let paths: BTreeSet<_> = frag.files.iter().map(|f| f.path.clone()).collect();
		assert!(paths.contains("tts/main.flowgraph.toml"));
		assert!(paths.contains("tts/jp.flowgraph.toml"));
		assert!(!paths.contains("other/misc.flowgraph.toml"));
	}

	#[test]
	fn copy_cross_file_edge_produces_dangling() {
		let td = mk_dir();
		write_file(
			td.path(),
			"main.flowgraph.toml",
			r#"
[[nodes]]
id = "src"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "x"

[[edges]]
from = "src:value"
to   = "sink::dst:message"
"#,
		);
		write_file(
			td.path(),
			"sink.flowgraph.toml",
			r#"
[[nodes]]
id = "dst"
feature = "flowgraph.log.info"
"#,
		);

		let req = CopyRequest {
			scope: FragmentScope::File,
			targets: vec![CopyTarget::File { fq: "main".into() }],
			origin: None,
		};
		let frag = copy_targets(td.path(), &req).unwrap();
		assert_eq!(frag.files.len(), 1);
		assert_eq!(frag.files[0].edges.len(), 0, "cross-file edge は fragment 内に残さない");
		assert_eq!(frag.danglings.len(), 1);
		assert_eq!(frag.danglings[0].original, "sink::dst:message");
	}

	#[test]
	fn copy_deduplicates_when_multiple_targets_hit_same_node() {
		let td = mk_dir();
		write_file(
			td.path(),
			"main.flowgraph.toml",
			r#"
[[nodes]]
id = "a"
feature = "flowgraph.literal.string"
[nodes.properties]
value = "x"
"#,
		);
		let req = CopyRequest {
			scope: FragmentScope::Mixed,
			targets: vec![
				CopyTarget::File { fq: "main".into() },
				CopyTarget::Node {
					fq_file: "main".into(),
					node_id: "a".into(),
				},
			],
			origin: None,
		};
		let frag = copy_targets(td.path(), &req).unwrap();
		let file = &frag.files[0];
		assert_eq!(file.nodes.len(), 1, "dedup: 同一ノードは 1 度だけ");
	}

	#[test]
	fn roundtrip_serialize_parse() {
		let frag = Fragment {
			header: FragmentHeader {
				schema: FRAGMENT_SCHEMA_V1.to_string(),
				exported_at: "2026-04-17T00:00:00Z".to_string(),
				origin: Some("test".to_string()),
				scope: FragmentScope::Nodes,
			},
			files: vec![FragmentFile {
				path: SCRATCH_PATH.to_string(),
				meta: Some(FileMeta {
					title: Some("x".into()),
					description: None,
					tags: None,
				}),
				nodes: vec![NodeEntry {
					id: "a".into(),
					feature: "flowgraph.literal.string".into(),
					position: Some([1.0, 2.0]),
					properties: {
						let mut t = toml::Table::new();
						t.insert("value".into(), toml::Value::String("hello".into()));
						t
					},
				}],
				edges: vec![],
			}],
			danglings: vec![FragmentDangling {
				kind: "edge_endpoint".into(),
				original: "other:x".into(),
				reason: "out_of_scope".into(),
				in_file: Some("main".into()),
				edge_from: Some("a:value".into()),
				edge_to: Some("other:x".into()),
				external_side: Some("to".into()),
			}],
		};
		let text = serialize_fragment(&frag).unwrap();
		let parsed = parse_fragment(&text).unwrap();
		assert_eq!(parsed.header.schema, frag.header.schema);
		assert_eq!(parsed.header.scope, FragmentScope::Nodes);
		assert_eq!(parsed.files.len(), 1);
		assert_eq!(parsed.files[0].path, SCRATCH_PATH);
		assert_eq!(parsed.files[0].nodes.len(), 1);
		assert_eq!(parsed.danglings.len(), 1);
	}

	#[test]
	fn parse_rejects_unknown_schema() {
		let src = r#"
[fragment]
schema = "vac-flowgraph-fragment/v999"
exported_at = "2026-04-17T00:00:00Z"
scope = "nodes"
"#;
		assert!(parse_fragment(src).is_err());
	}

	#[test]
	fn normalize_fq_rejects_traversal() {
		assert!(normalize_fq_str("../bad").is_err());
		assert!(normalize_fq_str("/absolute").is_err());
		assert!(normalize_fq_str("").is_err());
		assert!(normalize_fq_str("ok/file").is_ok());
	}

	#[test]
	fn copy_missing_target_is_error() {
		let td = mk_dir();
		let req = CopyRequest {
			scope: FragmentScope::File,
			targets: vec![CopyTarget::File {
				fq: "nonexistent".into(),
			}],
			origin: None,
		};
		assert!(matches!(
			copy_targets(td.path(), &req).unwrap_err(),
			CopyError::FileNotFound(_)
		));
	}

	#[test]
	fn copy_no_targets_errors() {
		let td = mk_dir();
		let req = CopyRequest {
			scope: FragmentScope::Mixed,
			targets: vec![],
			origin: None,
		};
		assert!(matches!(copy_targets(td.path(), &req).unwrap_err(), CopyError::NoTargets));
	}
}
