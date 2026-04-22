//! Loader 診断情報（spec §8.3）。
//!
//! GUI / Control API 向けに JSON serializable。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use thiserror::Error;

/// 診断の深刻度。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
	Error,
	Warning,
	Info,
}

/// 機械可読な診断コード。GUI の "Fix it" ボタン実装で条件分岐に使う。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum DiagnosticCode {
	/// TOML パースエラー。
	TomlParse,
	/// `feature` が `NodeRegistry` に無い。
	UnknownFeature,
	/// `id` 重複（ファイル内 or 全体）。
	DuplicateNodeId,
	/// edge の `from` / `to` が `node:port` 文法に合致しない。
	InvalidPortRef,
	/// edge が参照するノードが見つからない。
	UnresolvedNodeRef,
	/// edge が参照するポートが NodeSpec に存在しない。
	UnknownPort,
	/// プロパティ型がスキーマと一致しない。
	PropertyTypeMismatch,
	/// 必須プロパティが未指定。
	MissingRequiredProperty,
	/// 未知プロパティ（typo 警告）。
	UnknownProperty,
	/// engine `build()` が返した各種検証エラー。
	EngineBuild,
	/// I/O エラー（ファイル読み込み失敗など）。
	Io,
	/// 曖昧な main 省略参照（`X::id` に対して `X/main.flowgraph.toml` が無い）。
	AmbiguousMainRef,
}

/// 単一診断メッセージ。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
	pub severity: Severity,
	pub code: DiagnosticCode,
	pub message: String,
	/// 発生源ファイル（複数ファイル統合時に特定用）。相対パス推奨。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub file: Option<PathBuf>,
	/// ノード ID（判明している場合）。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub node: Option<String>,
	/// edge インデックスまたは port 名など、文脈ヒント。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub hint: Option<String>,
}

impl Diagnostic {
	pub fn error(code: DiagnosticCode, message: impl Into<String>) -> Self {
		Self {
			severity: Severity::Error,
			code,
			message: message.into(),
			file: None,
			node: None,
			hint: None,
		}
	}

	pub fn warning(code: DiagnosticCode, message: impl Into<String>) -> Self {
		Self {
			severity: Severity::Warning,
			code,
			message: message.into(),
			file: None,
			node: None,
			hint: None,
		}
	}

	pub fn with_file(mut self, file: impl Into<PathBuf>) -> Self {
		self.file = Some(file.into());
		self
	}

	pub fn with_node(mut self, node: impl Into<String>) -> Self {
		self.node = Some(node.into());
		self
	}

	pub fn with_hint(mut self, hint: impl Into<String>) -> Self {
		self.hint = Some(hint.into());
		self
	}
}

/// Loader 失敗時のトップレベルエラー。
///
/// 原則として `diagnostics` にまとめて詰めて返す。最初の 1 件でアボートはしない。
#[derive(Debug, Error)]
#[error("flowgraph のロードに失敗 ({} error / {} warning)",
  diagnostics.iter().filter(|d| d.severity == Severity::Error).count(),
  diagnostics.iter().filter(|d| d.severity == Severity::Warning).count())]
pub struct LoadError {
	pub diagnostics: Vec<Diagnostic>,
}

impl LoadError {
	pub fn new(diagnostics: Vec<Diagnostic>) -> Self {
		Self { diagnostics }
	}

	pub fn from_single(d: Diagnostic) -> Self {
		Self { diagnostics: vec![d] }
	}

	pub fn has_errors(&self) -> bool {
		self.diagnostics.iter().any(|d| d.severity == Severity::Error)
	}

	pub fn errors(&self) -> impl Iterator<Item = &Diagnostic> {
		self.diagnostics.iter().filter(|d| d.severity == Severity::Error)
	}

	pub fn warnings(&self) -> impl Iterator<Item = &Diagnostic> {
		self.diagnostics.iter().filter(|d| d.severity == Severity::Warning)
	}
}

/// 成功ロード結果。warning が付帯しうる。
pub struct LoadReport {
	pub program: crate::flowgraph::FlowgraphProgram,
	pub diagnostics: Vec<Diagnostic>,
	/// 解決済みノード（`fq_name` → 定義元ファイル / feature）。GUI / debug 用メタ情報。
	pub node_meta: std::collections::HashMap<String, LoadedNodeMeta>,
}

impl std::fmt::Debug for LoadReport {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		f.debug_struct("LoadReport")
			.field("nodes", &self.node_meta.keys().collect::<Vec<_>>())
			.field("diagnostics", &self.diagnostics)
			.finish()
	}
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoadedNodeMeta {
	pub feature: String,
	pub file: PathBuf,
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub position: Option<[f64; 2]>,
	/// δ-9: 解決済み properties（`PropertySpec` ベースの型変換済）。ブリッジ層が
	/// `path` / `engine` などを読むのに使う。`Serialize` には不要なので skip。
	#[serde(default, skip_serializing, skip_deserializing)]
	pub properties: crate::flowgraph::node::InputMap,
}
