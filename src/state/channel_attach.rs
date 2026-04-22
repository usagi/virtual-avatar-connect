//! `ChannelDatum` に添付する「由来（source）」「構造化メタ情報（meta）」「添付データ（attachments）」の型。
//!
//! 設計方針:
//! - 既存の `content: String` はそのまま「表示／LLM 入力／TTS に流せる人間可読テキスト」として維持する。
//! - 本モジュールの型はすべて **加算**（追加）の拡張点で、既存 processor は無改修で動き続ける。
//! - バイナリ実体は `Inline`（小さい・短命）と `File`（ランタイム一時ディレクトリ下のファイル参照）に分ける。
//!   WS / Web UI へは **実体ではなく URL / href を返す**方針とする（Phase 1+ 実装予定）。

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// 《`ChannelDatum` の由来》 どの処理系 / どの外部イベントから生まれた Datum かを表す。
///
/// ルーティング・ログ・LLM コンテキスト前置き等に利用する。いずれも表示・ログ用の緩い文字列で、enum 化はしない
/// （新しい種別をコード変更なしで運用できる柔軟さを優先）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataSource {
 /// 大分類。例: `"twitch.eventsub"`, `"twitch.irc"`, `"voice.vosk"`, `"voice.whisper"`,
 /// `"openai.chat"`, `"ocr"`, `"screenshot"`, `"user.web"`, `"command"`, `"translation.libre"`。
 pub kind: String,
 /// 小分類。例: `"channel.cheer"`, `"gpt-5.4-medium"`, `"vosk-model-ja-0.22"`。
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub subtype: Option<String>,
 /// 表示用の当事者名（ユーザー表示名、発言者名、ウィンドウタイトルなど）。
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub actor: Option<String>,
}

impl DataSource {
 pub fn new(kind: impl Into<String>) -> Self {
  Self {
   kind: kind.into(),
   subtype: None,
   actor: None,
  }
 }

 pub fn with_subtype(mut self, subtype: impl Into<String>) -> Self {
  self.subtype = Some(subtype.into());
  self
 }

 pub fn with_actor(mut self, actor: impl Into<String>) -> Self {
  self.actor = Some(actor.into());
  self
 }
}

/// 《`ChannelDatum` 添付データ》 音声・画像・URL・生 JSON 等を構造的に持ち運ぶ。
///
/// `#[serde(tag = "kind")]` により JSON では `{"kind": "url", "url": "..."}` のように平坦化される。
/// Phase 1 以降で以下を追加予定:
/// - `File` に `Arc<tempfile::TempPath>` を `#[serde(skip)]` で持たせ、最後のクローンが drop されたら物理削除する。
/// - `Inline` は `conf.attachment_inline_max_bytes`（既定 32 KiB）を超えたら自動で `File` 化する。
/// - WS / Web UI への出力は `href` / `url` だけを露出し、`Inline::data_base64` は削る投影型を噛ませる。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Attachment {
 /// 外部 URL 参照。`http(s)://` / `data:` などを想定。
 Url {
  url: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  mime: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  title: Option<String>,
 },
 /// ランタイム一時ディレクトリ配下のファイル（Phase 1 以降で Web ルートから `href` で配信）。
 File {
  /// VAC の Web ルートからの相対 href（例: `/runtime/<session>/audio/<uuid>.wav`）。Phase 0 時点では未配信。
  href: String,
  /// OS 上の実体パス。
  path: PathBuf,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  mime: Option<String>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  bytes: Option<u64>,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  label: Option<String>,
 },
 /// 小さなバイナリをそのまま抱える（base64 化して JSON 可搬）。上限超過時は Phase 1 以降で自動で `File` に降格。
 Inline {
  mime: String,
  /// 実データは base64（標準アルファベット・パディング有り）。
  data_base64: String,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  label: Option<String>,
 },
 /// 構造化 JSON 値。
 ///
 /// 用途:
 /// - 汎用 HTTP / Web API 取得プロセッサがレスポンス本体をそのまま流す場合の第一級ペイロード
 ///   （例: 天気・為替・カレンダー・自作 Web API のレスポンス）。
 /// - 後段プロセッサ（OpenAI Chat のツール呼び出し結果、ルーティング分岐）が構造のまま参照できる。
 ///
 /// 指針:
 /// - 小さな構造化データを想定。数百 KB に達するようなバイナリは `File`、短い binary blob は `Inline`、URL ポインタで足りるなら `Url` を使う。
 /// - Twitch EventSub のように「VAC で使う値がある程度決まっている」プロデューサでは、代わりに `ChannelDatum::meta` へ
 ///   正規化キーで入れる方が下流消費者に親切（キー規約に揃うため）。`Json` は「スキーマが呼び出し時に決まる」汎用 API 用。
 Json {
  value: serde_json::Value,
  #[serde(default, skip_serializing_if = "Option::is_none")]
  label: Option<String>,
 },
}

impl Attachment {
 pub fn url(url: impl Into<String>) -> Self {
  Self::Url {
   url: url.into(),
   mime: None,
   title: None,
  }
 }

 pub fn file(href: impl Into<String>, path: impl Into<PathBuf>) -> Self {
  Self::File {
   href: href.into(),
   path: path.into(),
   mime: None,
   bytes: None,
   label: None,
  }
 }

 pub fn json(value: serde_json::Value) -> Self {
  Self::Json { value, label: None }
 }

 /// `label` 付きで JSON 添付を生成する（汎用 API プロセッサがエンドポイント名などを渡す用途）。
 pub fn json_with_label(value: serde_json::Value, label: impl Into<String>) -> Self {
  Self::Json {
   value,
   label: Some(label.into()),
  }
 }

 /// ラベル／タイトルを設定する（対応する variant でのみ効く）。
 /// `Url` は `title`、それ以外は `label` を設定する。
 pub fn with_label(mut self, label: impl Into<String>) -> Self {
  let l = label.into();
  match &mut self {
   Self::Url { title, .. } => *title = Some(l),
   Self::File { label, .. } | Self::Inline { label, .. } | Self::Json { label, .. } => *label = Some(l),
  }
  self
 }

 /// MIME タイプを設定する（`Url`/`File`/`Inline` にのみ効く。`Json` は無視）。
 pub fn with_mime(mut self, mime: impl Into<String>) -> Self {
  let m = mime.into();
  match &mut self {
   Self::Url { mime, .. } | Self::File { mime, .. } => *mime = Some(m),
   Self::Inline { mime, .. } => *mime = m,
   Self::Json { .. } => {},
  }
  self
 }

 /// 添付種別の短いタグ（ログ用途）。
 pub fn kind_str(&self) -> &'static str {
  match self {
   Self::Url { .. } => "url",
   Self::File { .. } => "file",
   Self::Inline { .. } => "inline",
   Self::Json { .. } => "json",
  }
 }

 /// `Json` variant であれば内部の値を参照する。
 pub fn as_json(&self) -> Option<&serde_json::Value> {
  if let Self::Json { value, .. } = self {
   Some(value)
  } else {
   None
  }
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 use serde_json::json;

 #[test]
 fn json_attachment_roundtrip_preserves_structure() {
  let v = json!({
   "temperature": 12.3,
   "conditions": ["cloudy", "rain"],
   "location": {"city": "Tokyo", "lat": 35.68, "lon": 139.76},
  });
  let att = Attachment::json_with_label(v.clone(), "openweather.current");
  let s = serde_json::to_string(&att).expect("serialize");
  let back: Attachment = serde_json::from_str(&s).expect("deserialize");
  assert_eq!(att, back);
  let roundtripped = back.as_json().expect("Json variant");
  assert_eq!(roundtripped, &v);
 }

 #[test]
 fn json_attachment_is_tagged_as_json_in_serialized_form() {
  let att = Attachment::json(json!({"ok": true}));
  let s = serde_json::to_string(&att).unwrap();
  assert!(s.contains("\"kind\":\"json\""), "got: {}", s);
 }

 #[test]
 fn with_label_sets_title_on_url_and_label_on_others() {
  let u = Attachment::url("https://example.com/a").with_label("example");
  match u {
   Attachment::Url { title: Some(ref t), .. } if t == "example" => {},
   other => panic!("expected Url with title set, got {:?}", other),
  }
  let j = Attachment::json(json!({})).with_label("weather");
  match j {
   Attachment::Json { label: Some(ref l), .. } if l == "weather" => {},
   other => panic!("expected Json with label set, got {:?}", other),
  }
 }

 #[test]
 fn with_mime_ignores_json_variant() {
  let j = Attachment::json(json!({})).with_mime("application/json");
  assert!(matches!(j, Attachment::Json { .. }));
 }
}
