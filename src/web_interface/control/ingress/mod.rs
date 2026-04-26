//! Phase VI-α-6: Control API から ChannelDatum を直接投入するエンドポイント。
//!
//! 既存の `/input`（《WebInput》 processor） は `[[processors]] feature = "webinput"` の宣言に依存し、
//! 登録されていないパスは 500 を返す。GUI から「常に叩ける」入力口が欲しいので、Control API 配下に
//! 設定非依存の ingress エンドポイントを新設する。
//!
//! - 認証: `/api/v1/control/*` 全体と同じ `control_api_auth` ミドルウェアに従う。
//! - 対象: 任意チャネル + 任意メタデータ（flags / source / meta）。
//! - attachments は Phase VI-α-6 時点では未対応（バイナリの扱いは GUI ごと別途設計する）。
//!
//! レスポンスには `id`（ChannelDatum の連番）と解決された `channel` を返す。GUI は WS の `channel_datum`
//! イベントとこの id を突き合わせれば自分の送信がパイプラインを流れたか追跡できる（将来拡張）。
//!
//! ハンドラ本体は [`post`]。

mod post;

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::state::DataSource;

use actix_web::web;

/// `/ingress` リクエストボディ。`channel` と `content` 以外はすべて省略可能。
#[derive(Debug, Deserialize)]
pub struct IngressRequest {
	/// 宛先の VAC 内部チャンネル名。必須・空文字不可。
	pub channel: String,
	/// 表示／LLM／TTS に流れる人間可読テキスト。必須・空文字可。
	pub content: String,
	/// `FLAG_IS_FINAL` 相当。省略時は `true`（GUI からの手動確定送信を想定）。
	#[serde(default)]
	pub is_final: Option<bool>,
	/// 追加で立てたい flags（`is_final` はここに列挙せず上の専用フィールドで制御することを推奨）。
	#[serde(default)]
	pub flags: Option<Vec<String>>,
	/// `ChannelDatum.source`。省略時は `DataSource { kind: "control.ingress", actor: "gui" }` を付与。
	#[serde(default)]
	pub source: Option<DataSource>,
	/// 自由形式の構造化メタ。JSON object を想定。
	#[serde(default)]
	pub meta: Option<BTreeMap<String, serde_json::Value>>,
}

#[derive(Debug, Serialize)]
pub struct IngressResponse {
	/// 投入された ChannelDatum の連番 ID。
	pub id: u64,
	/// 実際に使われたチャネル名（`channel` フィールドのトリム結果）。
	pub channel: String,
	/// 常に `true`（失敗時は 4xx/5xx で返すので body には来ない）。
	pub accepted: bool,
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(post::post_ingress);
}
