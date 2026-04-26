//! η-5: V1 の辞書 / 正規表現ファイルを η の 11 カラム統一 TSV に変換する CLI。
//!
//! # 入力フォーマット（V1）
//!
//! - **Dictionary (loose)**: `dictionary.*.txt` — 各行が `source replacement`（空白または TAB 1 区切り）。
//!   コメント行は `#` 始まり。
//! - **Regex (space)**: `regex.*.txt` — 各行が `replacement pattern`（先頭トークンが replacement）。
//!   空白 1 区切り。
//! - **Regex (CSV)**: `regex.*.csv` — `replacement,pattern`。CSV 標準のクォートサポート。
//!   空 replacement（= 削除ルール）も許可、このとき変換後の `replacement` は空文字列。
//!
//! # 出力フォーマット（η）
//!
//! `docs/roadmap/phase-eta-dictionary-unification.md` §3.1 の 11 カラム辞書スキーマ TSV。
//!
//! # 使い方
//!
//! ```sh
//! # 単一ファイル変換
//! virtual-avatar-connect-migrate-dict \
//!     --input dictionary.arknights.txt \
//!     --kind literal \
//!     --output dictionary.arknights.dict.tsv \
//!     --by "arknights:seed" --locked
//!
//! # 複数 input を 1 TSV に merge
//! virtual-avatar-connect-migrate-dict \
//!     --input dictionary.chat.txt:literal:chat \
//!     --input regex.chat.csv:regex:chat \
//!     --output dictionary.chat.dict.tsv
//! ```

use std::fs;
use std::path::PathBuf;

use anyhow::{anyhow, bail, Context, Result};
use clap::Parser;

#[derive(Debug, Parser)]
#[command(
	name = "migrate-dict",
	about = "V1 dictionary/regex files -> η unified 11-column TSV",
	long_about = "V1 の dictionary.*.txt / regex.*.txt / regex.*.csv を η の 11 カラム統一 TSV に変換する CLI。\n\
	             --input は \"path\" か \"path:kind\" か \"path:kind:tag\" で複数指定可能。\n\
	             kind: auto / literal / regex (auto は拡張子と接頭辞から推測)"
)]
struct Cli {
	/// 入力ファイル。`path` / `path:kind` / `path:kind:tag` 形式で複数回指定可能。
	#[arg(short, long, required = true, num_args = 1..)]
	input: Vec<String>,

	/// 出力 TSV path。`-` で stdout。
	#[arg(short, long)]
	output: String,

	/// 全エントリ共通の `by` フィールド（個別指定は `--input` の tag が優先）。
	#[arg(long, default_value = "legacy")]
	by: String,

	/// `is_locked=true` で全行を出力する。ファイル由来の "固定辞書" 扱い。
	#[arg(long)]
	locked: bool,

	/// `--locked` 指定時以外のデフォルト kind（path 指定側で上書き可能）。
	#[arg(long, default_value = "auto")]
	default_kind: String,

	/// 上書き確認。指定しないと既存 output を上書きしない。
	#[arg(long)]
	force: bool,
}

#[derive(Debug, Clone)]
struct InputSpec {
	path: PathBuf,
	kind_hint: KindHint,
	tag: Option<String>,
}

#[derive(Debug, Clone, Copy)]
enum KindHint {
	Auto,
	Literal,
	Regex,
}

impl KindHint {
	fn parse(s: &str) -> Result<Self> {
		match s {
			"auto" => Ok(Self::Auto),
			"literal" => Ok(Self::Literal),
			"regex" => Ok(Self::Regex),
			other => Err(anyhow!("unknown kind '{other}' (expected auto/literal/regex)")),
		}
	}
}

fn parse_input_arg(s: &str, default_kind: &str) -> Result<InputSpec> {
	let parts: Vec<&str> = s.splitn(3, ':').collect();
	let path = PathBuf::from(parts[0]);
	let kind_hint = match parts.get(1) {
		Some(k) if !k.is_empty() => KindHint::parse(k)?,
		_ => KindHint::parse(default_kind)?,
	};
	let tag = parts.get(2).map(|s| s.to_string()).filter(|s| !s.is_empty());
	Ok(InputSpec { path, kind_hint, tag })
}

fn detect_kind(path: &PathBuf, hint: KindHint) -> &'static str {
	if matches!(hint, KindHint::Literal) {
		return "literal";
	}
	if matches!(hint, KindHint::Regex) {
		return "regex";
	}
	// auto: 拡張子と接頭辞から判定
	let name = path.file_name().and_then(|s| s.to_str()).unwrap_or("");
	let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
	if name.starts_with("regex.") || ext == "csv" {
		"regex"
	} else if name.starts_with("dictionary.") {
		"literal"
	} else {
		"literal"
	}
}

#[derive(Debug, Clone)]
struct Row {
	source: String,
	replacement: String,
	kind: String,
	priority: i64,
	is_locked: bool,
	enabled: bool,
	by: String,
	created_at: String,
	expires_at: Option<String>,
	tags: String,
	note: String,
}

impl Row {
	fn to_tsv_line(&self) -> String {
		let cells = [
			escape(&self.source),
			escape(&self.replacement),
			escape(&self.kind),
			self.priority.to_string(),
			self.is_locked.to_string(),
			self.enabled.to_string(),
			escape(&self.by),
			escape(&self.created_at),
			self.expires_at.as_deref().map(escape).unwrap_or_default(),
			escape(&self.tags),
			escape(&self.note),
		];
		cells.join("\t")
	}
}

fn escape(s: &str) -> String {
	let mut out = String::with_capacity(s.len());
	for c in s.chars() {
		match c {
			'\\' => out.push_str(r"\\"),
			'\t' => out.push_str(r"\t"),
			'\n' => out.push_str(r"\n"),
			'\r' => out.push_str(r"\r"),
			c => out.push(c),
		}
	}
	out
}

fn now_rfc3339() -> String {
	jiff::Timestamp::now().strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
}

fn parse_dict_loose(contents: &str) -> Vec<(String, String)> {
	let mut rows = Vec::new();
	for line in contents.lines() {
		let trimmed = line.trim();
		if trimmed.is_empty() || trimmed.starts_with('#') {
			continue;
		}
		let mut it = trimmed.split_whitespace();
		let Some(src) = it.next() else { continue };
		let Some(rep) = it.next() else { continue };
		rows.push((src.to_string(), rep.to_string()));
	}
	rows
}

fn parse_regex_space(contents: &str) -> Vec<(String, String)> {
	// V1 形式: `replacement<SPACES>pattern` 1 行 1 エントリ。
	// replacement が複数単語になりうるため、「`^` から始まるトークン」を pattern 開始点として探す。
	// 見つからない場合は先頭単語 = replacement, 残り = pattern にフォールバック。
	let mut rows = Vec::new();
	for line in contents.lines() {
		let trimmed = line.trim_end_matches(['\r', '\n']);
		if trimmed.trim().is_empty() || trimmed.trim_start().starts_with('#') {
			continue;
		}
		let t = trimmed.trim_start();

		// "^" 始まりの regex パターンを探す: (空白) + "^"
		let anchor_split = find_regex_anchor_split(t);
		let (replacement, pattern) = match anchor_split {
			Some(idx) => {
				let r = t[..idx].trim_end().to_string();
				let p = t[idx..].to_string();
				(r, p)
			}
			None => {
				// 先頭単語が replacement
				let end = t.char_indices().find(|(_, c)| c.is_whitespace()).map(|(i, _)| i).unwrap_or(t.len());
				let r = t[..end].to_string();
				let p = t[end..].trim_start().to_string();
				(r, p)
			}
		};
		if pattern.is_empty() {
			continue;
		}
		rows.push((pattern, replacement));
	}
	rows
}

/// `t` の中で "空白 + '^'" というパターンの '^' 位置を返す。
fn find_regex_anchor_split(t: &str) -> Option<usize> {
	let bytes = t.as_bytes();
	for i in 1..bytes.len() {
		if bytes[i] == b'^' {
			let prev = bytes[i - 1];
			if prev == b' ' || prev == b'\t' {
				return Some(i);
			}
		}
	}
	None
}

fn parse_regex_csv(contents: &str) -> Result<Vec<(String, String)>> {
	let mut reader = csv::ReaderBuilder::new()
		.has_headers(false)
		.flexible(true)
		.from_reader(contents.as_bytes());
	let mut rows = Vec::new();
	for rec in reader.records() {
		let rec = rec.context("CSV parse")?;
		if rec.is_empty() {
			continue;
		}
		let replacement = rec.get(0).unwrap_or("").to_string();
		let pattern = rec.get(1).unwrap_or("").to_string();
		if pattern.is_empty() {
			continue;
		}
		rows.push((pattern, replacement));
	}
	Ok(rows)
}

fn convert_input(spec: &InputSpec, by_default: &str, locked: bool) -> Result<Vec<Row>> {
	let contents = fs::read_to_string(&spec.path).with_context(|| format!("read {}", spec.path.display()))?;
	let kind = detect_kind(&spec.path, spec.kind_hint);
	let now = now_rfc3339();

	let pairs: Vec<(String, String)> = match kind {
		"literal" => parse_dict_loose(&contents),
		"regex" => {
			let ext = spec.path.extension().and_then(|s| s.to_str()).unwrap_or("");
			if ext.eq_ignore_ascii_case("csv") {
				parse_regex_csv(&contents)?
			} else {
				parse_regex_space(&contents)
			}
		}
		_ => unreachable!(),
	};

	let by = spec.tag.as_deref().unwrap_or(by_default).to_string();
	// ソース由来の tag も tags 列に詰める（by とは別軸のメタ）
	let tag_string = spec.path.file_stem().and_then(|s| s.to_str()).unwrap_or("").to_string();

	let mut rows = Vec::with_capacity(pairs.len());
	for (source, replacement) in pairs {
		rows.push(Row {
			source,
			replacement,
			kind: kind.to_string(),
			priority: 0,
			is_locked: locked,
			enabled: true,
			by: by.clone(),
			created_at: now.clone(),
			expires_at: None,
			tags: tag_string.clone(),
			note: format!("migrated from {}", spec.path.display()),
		});
	}
	Ok(rows)
}

fn header_line() -> &'static str {
	"source\treplacement\tkind\tpriority\tis_locked\tenabled\tby\tcreated_at\texpires_at\ttags\tnote"
}

fn main() -> Result<()> {
	let cli = Cli::parse();
	let specs: Vec<InputSpec> = cli
		.input
		.iter()
		.map(|s| parse_input_arg(s, &cli.default_kind))
		.collect::<Result<_>>()?;

	let mut all_rows: Vec<Row> = Vec::new();
	for spec in &specs {
		let rows = convert_input(spec, &cli.by, cli.locked)?;
		eprintln!(
			"[migrate-dict] {} -> {} rows ({}, locked={})",
			spec.path.display(),
			rows.len(),
			detect_kind(&spec.path, spec.kind_hint),
			cli.locked
		);
		all_rows.extend(rows);
	}

	let mut out = String::new();
	out.push_str(header_line());
	out.push('\n');
	for r in &all_rows {
		out.push_str(&r.to_tsv_line());
		out.push('\n');
	}

	if cli.output == "-" {
		print!("{out}");
	} else {
		let dst = PathBuf::from(&cli.output);
		if dst.exists() && !cli.force {
			bail!("{} が既に存在します。上書きするには --force を指定してください", dst.display());
		}
		let tmp = dst.with_extension("tmp");
		fs::write(&tmp, &out).with_context(|| format!("write tmp {}", tmp.display()))?;
		fs::rename(&tmp, &dst).with_context(|| format!("rename {} -> {}", tmp.display(), dst.display()))?;
		eprintln!(
			"[migrate-dict] wrote {} ({} rows, {} bytes)",
			dst.display(),
			all_rows.len(),
			out.len()
		);
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn parse_dict_loose_handles_space_and_tab() {
		let input = "にんげん 人間\nいかく\t異格\n# comment\n\n";
		let pairs = parse_dict_loose(input);
		assert_eq!(pairs.len(), 2);
		assert_eq!(pairs[0], ("にんげん".into(), "人間".into()));
		assert_eq!(pairs[1], ("いかく".into(), "異格".into()));
	}

	#[test]
	fn parse_regex_space_replacement_and_rest_as_pattern() {
		let input = "/quit ^\\s*システムコマンド\\s*シャットダウン.*\nUSAGI.NETWORK うさぎ\\s*ネットワーク\n";
		let pairs = parse_regex_space(input);
		assert_eq!(pairs.len(), 2);
		assert_eq!(pairs[0].0, "^\\s*システムコマンド\\s*シャットダウン.*");
		assert_eq!(pairs[0].1, "/quit");
		assert_eq!(pairs[1].1, "USAGI.NETWORK");
	}

	#[test]
	fn parse_regex_space_multi_word_replacement_split_at_anchor() {
		// V1 `regex.pre-command.txt` の "/set init ^..." パターン（replacement に空白あり）
		let input = "/set init ^\\s*システムコマンド\\s*セット\\s*(イニット|イニシャライズ).*\n";
		let pairs = parse_regex_space(input);
		assert_eq!(pairs.len(), 1);
		assert_eq!(pairs[0].1, "/set init");
		assert!(pairs[0].0.starts_with('^'));
	}

	#[test]
	fn parse_regex_space_no_anchor_falls_back_to_first_token() {
		let input = "prefix .*\n";
		let pairs = parse_regex_space(input);
		assert_eq!(pairs.len(), 1);
		assert_eq!(pairs[0].1, "prefix");
		assert_eq!(pairs[0].0, ".*");
	}

	#[test]
	fn parse_regex_csv_replacement_pattern_order() {
		let input = "パチパチパチ,\"[8８]{3,}\"\n,\"([!?,.]+)\"\n$1 (以下略),\"^(.{32}).*\"\n";
		let pairs = parse_regex_csv(input).unwrap();
		assert_eq!(pairs.len(), 3);
		assert_eq!(pairs[0].0, "[8８]{3,}");
		assert_eq!(pairs[0].1, "パチパチパチ");
		assert_eq!(pairs[1].1, ""); // empty replacement OK
		assert_eq!(pairs[2].1, "$1 (以下略)");
	}

	#[test]
	fn detect_kind_from_filename() {
		assert_eq!(detect_kind(&PathBuf::from("dictionary.foo.txt"), KindHint::Auto), "literal");
		assert_eq!(detect_kind(&PathBuf::from("regex.foo.txt"), KindHint::Auto), "regex");
		assert_eq!(detect_kind(&PathBuf::from("regex.foo.csv"), KindHint::Auto), "regex");
		assert_eq!(detect_kind(&PathBuf::from("foo.csv"), KindHint::Auto), "regex");
		assert_eq!(detect_kind(&PathBuf::from("random.txt"), KindHint::Literal), "literal");
	}

	#[test]
	fn tsv_row_round_trips_escapes() {
		let r = Row {
			source: "a\tb".into(),
			replacement: "c\nd".into(),
			kind: "literal".into(),
			priority: 5,
			is_locked: true,
			enabled: true,
			by: "legacy".into(),
			created_at: "2026-04-23T00:00:00Z".into(),
			expires_at: None,
			tags: String::new(),
			note: String::new(),
		};
		let line = r.to_tsv_line();
		assert!(!line.contains('\n'));
		assert!(line.contains(r"\t"));
		assert!(line.contains(r"\n"));
	}

	#[test]
	fn parse_input_arg_forms() {
		let a = parse_input_arg("foo.txt", "auto").unwrap();
		assert!(a.tag.is_none());
		assert!(matches!(a.kind_hint, KindHint::Auto));
		let b = parse_input_arg("foo.txt:regex", "auto").unwrap();
		assert!(matches!(b.kind_hint, KindHint::Regex));
		let c = parse_input_arg("foo.txt:literal:chat", "auto").unwrap();
		assert!(matches!(c.kind_hint, KindHint::Literal));
		assert_eq!(c.tag.as_deref(), Some("chat"));
	}
}
