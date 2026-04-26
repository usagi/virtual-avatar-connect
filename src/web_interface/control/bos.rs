//! Phase VI-γ-2a: Broadcast Output Sources (BOS) レジストリ。
//!
//! ## BOS とは
//!
//! VAC が OBS 向けに提供する **ブラウザソース** の総称。既存の `resources/browser-output/` 配下や
//! ユーザーが `conf.browser_source.document_root` に指定したディレクトリにあるサブディレクトリ群
//! （`subtitles-1/`, `bgm-player/`, etc.）のこと。各サブディレクトリ直下に `index.html` がある前提。
//!
//! 本モジュールは、そのレジストリを **一覧 API** として提供する:
//!   `GET /api/v1/control/bos`
//!
//! GUI の Live タブが、この一覧から 1 つ選んで iframe 埋め込みし、OBS 用の URL コピーボタンを置く用途。
//!
//! ## 設計方針
//!
//! - **走査はシンプル**: `document_root` 直下のサブディレクトリで `index.html` を持つものを列挙する。
//!   深いネストは Phase 初期では扱わない（必要になったら拡張）。
//! - **カテゴリはファイル名接頭辞で推定**（例: `subtitles-*` → `subtitles`）。将来の GUI 側グルーピング用。
//! - **channel パラメータ対応判定**: 雑に index.html / 同階層の .js を読んで `channel` 文字列を含むか。
//!   ミスる可能性はあるが、誤判定しても致命的ではないのでこれで十分。
//! - **URL は `/browser-output/<subdir>/` の相対 path** で返す。GUI 側が `window.location.origin` と
//!   合成して絶対 URL にする（OBS に貼る用のコピーは GUI 側の仕事）。
//!
//! ## 将来拡張（γ-2 以降）
//! - テーマ/パラメータのスキーマを BOS ディレクトリ直下の `bos.json` で宣言 → GUI が型付き UI で編集
//! - 複数 channel の比較表示
//! - 認証トークン付き URL 生成（LAN 越し向け）

use std::path::Path;

use actix_web::web::{self, Data};
use actix_web::{get, HttpResponse, Responder};
use serde::Serialize;

use crate::SharedState;

/// 1 件の BOS エントリ。GUI の select/option のもと。
#[derive(Debug, Serialize, Clone)]
pub struct BosEntry {
	/// 一意 ID = サブディレクトリ名そのまま（ASCII 想定）。URL にも使う。
	pub id: String,
	/// 人間向け表示名。`id` を Title Case 化した程度。
	pub title: String,
	/// 推定カテゴリ。未知接頭辞は "other"。
	pub category: BosCategory,
	/// GUI が `window.location.origin` と合成して使う相対 URL。例: `/browser-output/subtitles-3/`
	pub url: String,
	/// ディスク上のパス（デバッグ/運用情報表示用）。
	pub path: String,
	/// `index.html` / 同階層の .js に `channel` パラメータへの言及があるか。
	pub supports_channel_param: bool,
	/// VAC 同梱 (`resources/browser-output/` 直下と同定できたもの) か、ユーザー定義か。
	pub user_defined: bool,
}

#[derive(Debug, Serialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BosCategory {
	Subtitles,
	Bgm,
	Effects,
	Other,
}

#[derive(Debug, Serialize)]
pub struct BosResponse {
	/// 走査した document_root の絶対パス。
	pub document_root: Option<String>,
	/// 公開 URL プレフィックス（常に `/browser-output`）。
	pub url_prefix: String,
	/// エントリ一覧（category → 名前順）。
	pub entries: Vec<BosEntry>,
}

#[get("/bos")]
pub async fn get_bos(state: Data<SharedState>) -> impl Responder {
	let document_root = {
		let s = state.read().await;
		s.browser_source_document_root.clone()
	};

	let Some(root) = document_root else {
		return HttpResponse::Ok().json(BosResponse {
			document_root: None,
			url_prefix: "/browser-output".to_string(),
			entries: Vec::new(),
		});
	};

	// canonicalize は存在しない相対パスで失敗するが、その場合は素の root でフォールバック。
	let root_canon = std::fs::canonicalize(&root).unwrap_or_else(|_| root.clone());
	let bundled_canon = std::fs::canonicalize(Path::new("resources").join("browser-output")).ok();

	let mut entries = Vec::new();
	match std::fs::read_dir(&root) {
		Ok(iter) => {
			for ent in iter.flatten() {
				let p = ent.path();
				if !p.is_dir() {
					continue;
				}
				let index = p.join("index.html");
				if !index.is_file() {
					continue;
				}
				let name_os = p.file_name().unwrap_or_default().to_owned();
				let Some(name) = name_os.to_str() else {
					continue;
				};
				// 隠しディレクトリ (.* / _*) はスキップ。
				if name.starts_with('.') || name.starts_with('_') {
					continue;
				}
				let category = guess_category(name);
				let supports_channel = detect_channel_param(&p).unwrap_or(false);
				// user_defined 判定: bundled_canon 配下のサブディレクトリとして位置していなければユーザー定義扱い。
				// root 自体が bundled を指しているなら全て user_defined=false。
				let p_canon = std::fs::canonicalize(&p).unwrap_or_else(|_| p.clone());
				let user_defined = match &bundled_canon {
					Some(bundled) => !p_canon.starts_with(bundled),
					None => true,
				};
				entries.push(BosEntry {
					id: name.to_string(),
					title: title_from_id(name),
					category,
					url: format!("/browser-output/{name}/"),
					path: p.display().to_string(),
					supports_channel_param: supports_channel,
					user_defined,
				});
			}
		}
		Err(e) => {
			log::warn!("《BOS》 document_root 走査に失敗: {} ({})", root.display(), e);
		}
	}

	entries.sort_by(|a, b| {
		let order = |c: BosCategory| match c {
			BosCategory::Subtitles => 0,
			BosCategory::Bgm => 1,
			BosCategory::Effects => 2,
			BosCategory::Other => 3,
		};
		order(a.category)
			.cmp(&order(b.category))
			.then_with(|| a.id.to_ascii_lowercase().cmp(&b.id.to_ascii_lowercase()))
	});

	HttpResponse::Ok().json(BosResponse {
		document_root: Some(root_canon.display().to_string()),
		url_prefix: "/browser-output".to_string(),
		entries,
	})
}

/// サブディレクトリ名からカテゴリを推定する。接頭辞マッチ。
fn guess_category(name: &str) -> BosCategory {
	let lower = name.to_ascii_lowercase();
	if lower.starts_with("subtitles") || lower.starts_with("subtitle") || lower.starts_with("caption") {
		BosCategory::Subtitles
	} else if lower.starts_with("bgm") || lower.contains("music") || lower.contains("player") {
		BosCategory::Bgm
	} else if lower.starts_with("effect") || lower.starts_with("fx") {
		BosCategory::Effects
	} else {
		BosCategory::Other
	}
}

/// `index.html` または同ディレクトリの `.js` に `channel` 言及があるかを雑に検出。
/// 大きすぎるファイルは先頭 128 KiB だけ読む（無駄 I/O 防止）。
fn detect_channel_param(dir: &Path) -> Option<bool> {
	const MAX_READ: usize = 128 * 1024;
	let check = |p: &Path| -> bool {
		let Ok(mut f) = std::fs::File::open(p) else { return false };
		let mut buf = Vec::with_capacity(4096);
		use std::io::Read as _;
		// ベストエフォートで一気に読む（小さいはず）
		let _ = f.by_ref().take(MAX_READ as u64).read_to_end(&mut buf);
		let text = String::from_utf8_lossy(&buf);
		text.contains("channel")
	};
	if check(&dir.join("index.html")) {
		return Some(true);
	}
	// 同階層の .js も 1 段だけ覗く。poll.js など。
	if let Ok(iter) = std::fs::read_dir(dir) {
		for ent in iter.flatten() {
			let p = ent.path();
			if p.extension().and_then(|e| e.to_str()) == Some("js") && check(&p) {
				return Some(true);
			}
		}
	}
	Some(false)
}

/// `subtitles-3` → "Subtitles 3"、`bgm-player` → "Bgm Player"。
fn title_from_id(id: &str) -> String {
	id.split(['-', '_'])
		.filter(|s| !s.is_empty())
		.map(|s| {
			let mut chars = s.chars();
			match chars.next() {
				Some(c) => c.to_ascii_uppercase().to_string() + chars.as_str(),
				None => String::new(),
			}
		})
		.collect::<Vec<_>>()
		.join(" ")
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(get_bos);
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn category_guess() {
		assert_eq!(guess_category("subtitles-1"), BosCategory::Subtitles);
		assert_eq!(guess_category("subtitle-big"), BosCategory::Subtitles);
		assert_eq!(guess_category("caption-ja"), BosCategory::Subtitles);
		assert_eq!(guess_category("bgm-player"), BosCategory::Bgm);
		assert_eq!(guess_category("effects"), BosCategory::Effects);
		assert_eq!(guess_category("something-custom"), BosCategory::Other);
	}

	#[test]
	fn title_conversion() {
		assert_eq!(title_from_id("subtitles-3"), "Subtitles 3");
		assert_eq!(title_from_id("bgm-player"), "Bgm Player");
		assert_eq!(title_from_id("effects"), "Effects");
	}
}
