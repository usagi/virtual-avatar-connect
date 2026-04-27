//! Flowgraph Loader（spec §7 / §8）。
//!
//! `*.flowgraph.toml` ファイル群をパースして、Node/Edge 単位で `NodeRegistry` と
//! 突合しながら `FlowgraphProgram` を構築する。
//!
//! ## エントリポイント
//!
//! - [`load_file`][]: 単一ファイルをロードしてプログラムを構築。小規模テスト向け。
//! - [`load_flowgraph_dir`][]: `flowgraph/` ルートを再帰ウォークして全ファイル統合。
//!   プロダクション用。
//!
//! ## 診断
//!
//! どちらのエントリポイントも失敗時は [`LoadError`][] を返し、
//! 可能な限り複数のエラーをまとめて `Diagnostic` リストで返す（§8.3）。
//!
//! 部分成功（warning 伴う）は [`LoadReport`][] として返る。
//!
//! ## δ-5 非対応事項
//!
//! - ホットリロード（§8.5）: δ-6 以降で file watcher 連携。
//! - line / column の diagnostic 位置情報: `toml` crate は span を出さないため
//!   現状 file レベルのみ。GUI 表示用の精緻化は δ-6 で `toml_edit` span 化予定。
//! - include ディレクティブ: **非採用**（フォルダ走査で十分、明示的 include は混乱のもと）。

pub mod diagnostic;
pub mod dir;
pub mod file;
pub mod reference;

pub use diagnostic::{
	Diagnostic, DiagnosticCode, FlowgraphFileActivationMeta, LoadError, LoadReport, LoadedNodeMeta, Severity,
};
pub use dir::{fq_path_of_file, is_flowgraph_file, load_flowgraph_dir, walk_flowgraph_dir};
pub use file::{
	file_activation_meta, load_file, normalized_library_id, parse_flowgraph_file, EdgeEntry, FileMeta, FlowgraphEnumDef,
	FlowgraphFile, NodeEntry,
};
pub use reference::{parse_port_ref, resolve_fq_ref, PortRefStr, ResolveContext};
