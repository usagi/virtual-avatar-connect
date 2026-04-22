//! ポート参照文字列のパースと fq 解決（spec §6.4 / §7.3）。
//!
//! edge の `from` / `to` 値は `<node_ref>:<port_name>` 形式。
//!
//! - `node_ref` は fq name 構文:
//!   - `local_node`                     — 同一ファイル内ノード ID
//!   - `dir/file::node_id`              — 絶対 fq（root からのパス）
//!   - `../sibling/file::node_id`       — 相対パス（現在ファイルの親から起算）
//!   - `dir::node_id`                   — `dir/main.flowgraph.toml` の main 規約
//! - `port_name` は `[a-z0-9_]+` + `__xxx__` パターン（internal port）も許容。
//! - `::` は fq path と node_id の区切り、`:` はノードとポートの区切り。

use std::path::{Component, Path, PathBuf};

/// パース済み port 参照。`node_ref` は未解決の fq 断片のまま保持。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortRefStr {
	/// `::` 前の fq path。`None` の場合は「現在ファイル内ローカル」の意味。
	pub fq_path: Option<String>,
	/// ノード ID。
	pub node_id: String,
	/// ポート名。
	pub port: String,
}

/// port 参照文字列をパース。
///
/// 文法:
/// - `node:port` → `PortRefStr { fq_path: None, node_id: "node", port: "port" }`
/// - `path::node:port` → `PortRefStr { fq_path: Some("path"), node_id: "node", port: "port" }`
///
/// エラー:
/// - port 区切り `:` が見つからない
/// - port 名が空
/// - node_id が空
pub fn parse_port_ref(s: &str) -> Result<PortRefStr, String> {
	let s = s.trim();
	if s.is_empty() {
		return Err("port 参照文字列が空".into());
	}

	// 末尾の ":port_name" を切り出す。ただし "::" は fq 区切りなのでスキップ。
	// 右から探し、`::` 直後ではない単独 `:` を見つける。
	let bytes = s.as_bytes();
	let mut colon_idx: Option<usize> = None;
	let mut i = bytes.len();
	while i > 0 {
		i -= 1;
		if bytes[i] == b':' {
			// 直前も `:` なら fq 区切り、スキップ
			if i > 0 && bytes[i - 1] == b':' {
				// `::` の 2 文字分スキップ
				i -= 1;
				continue;
			}
			// 直後も `:` なら fq 区切りの前半、スキップ
			if i + 1 < bytes.len() && bytes[i + 1] == b':' {
				continue;
			}
			colon_idx = Some(i);
			break;
		}
	}

	let colon_idx = colon_idx.ok_or_else(|| {
		format!("port 参照に ':' が無い: '{s}'（期待形式: 'node:port' または 'path::node:port'）")
	})?;

	let node_part = &s[..colon_idx];
	let port = s[colon_idx + 1..].trim();

	if port.is_empty() {
		return Err(format!("ポート名が空: '{s}'"));
	}
	if node_part.is_empty() {
		return Err(format!("ノード参照が空: '{s}'"));
	}

	// node_part 側に `::` があるか
	if let Some(sep) = node_part.rfind("::") {
		let fq_path = &node_part[..sep];
		let node_id = node_part[sep + 2..].trim();
		if node_id.is_empty() {
			return Err(format!("ノード ID が空: '{s}'"));
		}
		if fq_path.trim().is_empty() {
			return Err(format!("fq path が空: '{s}'"));
		}
		Ok(PortRefStr {
			fq_path: Some(fq_path.trim().to_string()),
			node_id: node_id.to_string(),
			port: port.to_string(),
		})
	} else {
		Ok(PortRefStr {
			fq_path: None,
			node_id: node_part.trim().to_string(),
			port: port.to_string(),
		})
	}
}

/// fq path 解決に必要な文脈。
#[derive(Debug, Clone)]
pub struct ResolveContext {
	/// 現在ファイルの fq path（拡張子を除いたルート相対パス。例: `tts/jp_routing`）。
	pub current_file_fq: String,
	/// 既知のファイル fq path 集合（`tts/main`, `tts/jp_routing`, ...）。
	/// `dir::id` の main 規約解決時、`dir/main` が存在するか確認するのに使う。
	pub known_file_fqs: std::collections::HashSet<String>,
}

/// `PortRefStr` の `fq_path` を現在ファイルの文脈で正規化し、絶対 fq ファイル path を返す。
///
/// 返り値は拡張子なし root 相対 fq path（例: `tts/jp_routing`）。
///
/// 解決ルール（spec §6.4）:
/// - `None` → 現在ファイルの fq
/// - `/absolute` → ルート絶対（先頭 `/` を剥がす）
/// - `../sibling` → 現在ファイルのディレクトリから相対
/// - `dir/file` → ルート絶対扱い（spec では `/dir/file` と同義、先頭 `/` は省略可）
/// - `dir::main` の main 規約は **`dir/main` が既知 fq に含まれる場合** に `dir/main` へ書き換え。
///   つまり `dir` 単独を fq path として受け取ったときに、`dir.flowgraph.toml` が無く
///   `dir/main.flowgraph.toml` が存在するならそちらへ解決する。
pub fn resolve_fq_ref(raw: &PortRefStr, ctx: &ResolveContext) -> Result<String, String> {
	let fq_path = match &raw.fq_path {
		None => return Ok(ctx.current_file_fq.clone()),
		Some(p) => p.as_str(),
	};

	let candidate: String = if let Some(stripped) = fq_path.strip_prefix('/') {
		normalize_fq(stripped)?
	} else if fq_path.starts_with("..") || fq_path.starts_with("./") {
		// 現在ファイルのディレクトリ（fq path の最後のセグメントを親にする）
		let current_dir: PathBuf = match Path::new(&ctx.current_file_fq).parent() {
			Some(p) => p.to_path_buf(),
			None => PathBuf::new(),
		};
		let joined = current_dir.join(fq_path);
		normalize_fq_path(&joined)?
	} else {
		normalize_fq(fq_path)?
	};

	// main 規約: `dir` 単独で指されている → `dir/main` にフォールバック
	if !ctx.known_file_fqs.contains(&candidate) {
		let with_main = if candidate.is_empty() {
			"main".to_string()
		} else {
			format!("{candidate}/main")
		};
		if ctx.known_file_fqs.contains(&with_main) {
			return Ok(with_main);
		}
	}

	Ok(candidate)
}

/// `foo//bar/./baz/../qux` → `foo/bar/qux` 風の軽量正規化。forward slash 固定。
fn normalize_fq(s: &str) -> Result<String, String> {
	let mut segs: Vec<String> = Vec::new();
	for seg in s.split('/') {
		match seg {
			"" | "." => continue,
			".." => {
				if segs.pop().is_none() {
					return Err(format!("fq path がルート外を参照: '{s}'"));
				}
			}
			other => segs.push(other.to_string()),
		}
	}
	Ok(segs.join("/"))
}

/// `PathBuf` 経由の正規化。`../` を `Component::ParentDir` 経由で扱う。
fn normalize_fq_path(p: &Path) -> Result<String, String> {
	let mut segs: Vec<String> = Vec::new();
	for c in p.components() {
		match c {
			Component::CurDir => continue,
			Component::ParentDir => {
				if segs.pop().is_none() {
					return Err(format!(
						"fq path がルート外を参照: '{}'",
						p.display().to_string().replace('\\', "/")
					));
				}
			}
			Component::Normal(os) => segs.push(os.to_string_lossy().into_owned()),
			Component::RootDir | Component::Prefix(_) => {
				return Err(format!(
					"OS ルートパスは fq に使えない: '{}'",
					p.display().to_string().replace('\\', "/")
				));
			}
		}
	}
	Ok(segs.join("/"))
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::collections::HashSet;

	fn ctx(current: &str, known: &[&str]) -> ResolveContext {
		ResolveContext {
			current_file_fq: current.to_string(),
			known_file_fqs: known.iter().map(|s| s.to_string()).collect::<HashSet<_>>(),
		}
	}

	#[test]
	fn parse_local_ref() {
		let r = parse_port_ref("node_a:value").unwrap();
		assert_eq!(r.fq_path, None);
		assert_eq!(r.node_id, "node_a");
		assert_eq!(r.port, "value");
	}

	#[test]
	fn parse_fq_ref() {
		let r = parse_port_ref("tts/jp::node_b:exec_out").unwrap();
		assert_eq!(r.fq_path.as_deref(), Some("tts/jp"));
		assert_eq!(r.node_id, "node_b");
		assert_eq!(r.port, "exec_out");
	}

	#[test]
	fn parse_relative_ref() {
		let r = parse_port_ref("../ingress/twitch::main:content").unwrap();
		assert_eq!(r.fq_path.as_deref(), Some("../ingress/twitch"));
		assert_eq!(r.node_id, "main");
		assert_eq!(r.port, "content");
	}

	#[test]
	fn parse_absolute_ref() {
		let r = parse_port_ref("/sink/bos::final:text").unwrap();
		assert_eq!(r.fq_path.as_deref(), Some("/sink/bos"));
		assert_eq!(r.node_id, "final");
		assert_eq!(r.port, "text");
	}

	#[test]
	fn parse_internal_port() {
		let r = parse_port_ref("node:__trigger__").unwrap();
		assert_eq!(r.port, "__trigger__");
	}

	#[test]
	fn parse_missing_colon_errors() {
		assert!(parse_port_ref("node_no_port").is_err());
	}

	#[test]
	fn parse_empty_errors() {
		assert!(parse_port_ref("").is_err());
		assert!(parse_port_ref(":port").is_err());
		assert!(parse_port_ref("node:").is_err());
	}

	#[test]
	fn resolve_local_stays_in_current_file() {
		let r = PortRefStr {
			fq_path: None,
			node_id: "a".into(),
			port: "x".into(),
		};
		let c = ctx("tts/jp", &["tts/jp"]);
		assert_eq!(resolve_fq_ref(&r, &c).unwrap(), "tts/jp");
	}

	#[test]
	fn resolve_absolute_path() {
		let r = PortRefStr {
			fq_path: Some("/sink/bos".into()),
			node_id: "a".into(),
			port: "x".into(),
		};
		let c = ctx("tts/jp", &["sink/bos", "tts/jp"]);
		assert_eq!(resolve_fq_ref(&r, &c).unwrap(), "sink/bos");
	}

	#[test]
	fn resolve_relative_path() {
		let r = PortRefStr {
			fq_path: Some("../ingress/twitch".into()),
			node_id: "a".into(),
			port: "x".into(),
		};
		let c = ctx("tts/jp", &["ingress/twitch"]);
		assert_eq!(resolve_fq_ref(&r, &c).unwrap(), "ingress/twitch");
	}

	#[test]
	fn resolve_main_rule() {
		let r = PortRefStr {
			fq_path: Some("tts".into()),
			node_id: "a".into(),
			port: "x".into(),
		};
		let c = ctx("ingress/twitch", &["tts/main", "ingress/twitch"]);
		assert_eq!(resolve_fq_ref(&r, &c).unwrap(), "tts/main");
	}

	#[test]
	fn resolve_main_not_applied_when_direct_hit() {
		// `tts` が known_file_fqs に直接ある場合は main フォールバックしない
		let r = PortRefStr {
			fq_path: Some("tts".into()),
			node_id: "a".into(),
			port: "x".into(),
		};
		let c = ctx("ingress/twitch", &["tts", "ingress/twitch"]);
		assert_eq!(resolve_fq_ref(&r, &c).unwrap(), "tts");
	}

	#[test]
	fn resolve_parent_below_root_errors() {
		let r = PortRefStr {
			fq_path: Some("../../too/high".into()),
			node_id: "a".into(),
			port: "x".into(),
		};
		let c = ctx("tts/jp", &[]);
		assert!(resolve_fq_ref(&r, &c).is_err());
	}

	#[test]
	fn parse_node_id_with_underscores_and_digits() {
		let r = parse_port_ref("node_123:my_port").unwrap();
		assert_eq!(r.node_id, "node_123");
		assert_eq!(r.port, "my_port");
	}
}
