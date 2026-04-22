//! Twitch OAuth — **Device Code Flow（公開クライアント）**。
//!
//! Twitch の Authorization Code Flow は `client_secret` 必須・PKCE で代替不可。Implicit Flow は refresh_token 無し。
//! デスクトップアプリ（公開クライアント）が **`client_secret` 無し**で **refresh_token 付き** ユーザーアクセストークンを得るには
//! Device Code Flow（DCF）が公式の唯一の選択肢。本ファイルは DCF と、`refresh_token` を使った自動更新、ローカルファイルへの保存を提供する。
//!
//! # Phase V: broadcaster / moderator の 2 アカウント対応
//!
//! 発話中継・モデレーション Action のために **ボット/モデレーター用アカウント**を別枠で持てるようにした。
//! 内部的には [`OAuthIdent`] でファイル名・スコープ・環境変数を切り替えるだけで、DCF / refresh の本体ロジックは共通。
//!
//! 参考: <https://dev.twitch.tv/docs/authentication/getting-tokens-oauth/#device-code-grant-flow>

use crate::conf::{TwitchEventSubConfig, TwitchModeratorConfig, TwitchTokenKeySpec};
use anyhow::{anyhow, Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::time::Duration;

const DEVICE_URL: &str = "https://id.twitch.tv/oauth2/device";
const TOKEN_URL: &str = "https://id.twitch.tv/oauth2/token";
const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";

const ENV_CLIENT_ID: &str = "VAC_TWITCH_CLIENT_ID";
/// Legacy: broadcaster トークンファイルパス環境変数（Phase V 互換）。
const ENV_BROADCASTER_TOKEN_FILE: &str = "VAC_TWITCH_TOKEN_FILE";
/// Legacy: moderator トークンファイルパス環境変数（Phase V 互換）。
const ENV_MODERATOR_TOKEN_FILE: &str = "VAC_TWITCH_MODERATOR_TOKEN_FILE";

const DEVICE_GRANT_TYPE: &str = "urn:ietf:params:oauth:grant-type:device_code";

/// Virtual Avatar Connect 用 Twitch アプリ（Developer Console の Client ID）。設定・環境変数が無いときの既定。
pub(crate) const TWITCH_VAC_EMBEDDED_CLIENT_ID: &str = "af5rqbsvndkzlg6o9p7ajqc2996bvs";

/// 優先順位: 環境変数 `VAC_TWITCH_CLIENT_ID` → 設定の `client_id`（非空）→ 同梱の [`TWITCH_VAC_EMBEDDED_CLIENT_ID`]。
pub(crate) fn resolve_twitch_client_id(es: &TwitchEventSubConfig) -> String {
 std::env::var(ENV_CLIENT_ID)
  .ok()
  .filter(|s| !s.trim().is_empty())
  .map(|s| s.trim().to_string())
  .or_else(|| {
   es.client_id
    .as_ref()
    .map(|s| s.trim().to_string())
    .filter(|s| !s.is_empty())
  })
  .unwrap_or_else(|| TWITCH_VAC_EMBEDDED_CLIENT_ID.to_string())
}

/// **配信者**の EventSub / Helix 購読と、`vac_twitch_set_category` 等のカテゴリ変更用の既定スコープ。
pub(crate) const DEFAULT_BROADCASTER_OAUTH_SCOPES: &str = concat!(
 "channel:read:redemptions ",
 "bits:read ",
 "channel:read:subscriptions ",
 "channel:read:hype_train ",
 "moderator:read:followers ",
 "user:read:broadcast ",
 "channel:manage:broadcast"
);

/// **モデレーター/ボット**の既定スコープ。発話 (`user:write:chat` + `user:bot`) と、モデレーション系
/// （BAN / timeout / メッセージ削除）。このアカウントは **放送主とは別の Twitch ユーザー**を想定している。
pub(crate) const DEFAULT_MODERATOR_OAUTH_SCOPES: &str = concat!(
 "user:write:chat ",
 "user:bot ",
 "moderator:manage:banned_users ",
 "moderator:manage:chat_messages ",
 "moderator:manage:announcements"
);

// ---------------------------------------------------------------------------
// アカウント抽象
// ---------------------------------------------------------------------------

/// 1 本の OAuth トークンを取り扱うための識別子。保存ファイル・スコープ・認可タイムアウト・ログタグをまとめる。
///
/// Phase ζ-1 で任意の `key` を許容するため `&'static str` 群を `String` 化した。`label` は
/// key 名そのまま（"broadcaster" / "moderator" / ユーザー定義）。
#[derive(Debug, Clone)]
pub struct OAuthIdent {
 pub client_id: String,
 pub scopes: String,
 pub timeout_secs: u64,
 /// 主要な環境変数名（`VAC_TWITCH_TOKEN_FILE_<KEY>`）。token_file_path() はさらに
 /// `token_file_env_aliases` と config dir fallback も見る。
 pub token_file_env: String,
 /// Phase V 互換用の追加環境変数名。broadcaster→`VAC_TWITCH_TOKEN_FILE`、
 /// moderator→`VAC_TWITCH_MODERATOR_TOKEN_FILE`。空なら無視。
 pub token_file_env_aliases: Vec<String>,
 pub token_file_name: String,
 /// ログ識別用。"broadcaster" / "moderator" / ユーザー定義 key。
 pub label: String,
 /// ブラウザ起動コマンドテンプレート（`{url}` 置換）。未指定なら OS 既定。
 pub browser_command: Option<String>,
}

/// 任意の `key` からファイル名として安全な ASCII slug を得る。
///
/// `[a-zA-Z0-9_-]` のみを許容、それ以外は `_` に置換。`.` が含まれるとファイル名衝突の
/// 温床になるので同様に `_` 化する。
fn sanitize_key_for_filename(key: &str) -> String {
 key.chars()
  .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
  .collect()
}

/// 任意の `key` から環境変数サフィックスを得る（`VAC_TWITCH_TOKEN_FILE_<KEY>`）。
fn sanitize_key_for_env(key: &str) -> String {
 key.chars()
  .map(|c| {
   let c = c.to_ascii_uppercase();
   if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }
  })
  .collect()
}

impl OAuthIdent {
 /// Phase ζ-1: 任意 key から `OAuthIdent` を構築する汎用コンストラクタ。
 ///
 /// - `es`: 既定 `client_id` の解決元。EventSub 未設定インスタンスでは `None` で呼べないため、
 ///   呼び出し側でダミー `TwitchEventSubConfig::default_for_oauth_only()` 相当を用意する
 ///   ケースが将来出るかもしれない（現状は全ての既存呼び出しが `es` を持っている）。
 /// - `spec`: `[twitch.tokens.<key>]` の個別設定。`None` なら broadcaster/moderator の
 ///   旧ブロック既定から継承する。
 ///
 /// scopes の優先順位:
 ///   1. `spec.scopes`（非空）
 ///   2. key == "broadcaster" かつ `es.oauth_scopes` が非空 → それ
 ///   3. key == "broadcaster" → `DEFAULT_BROADCASTER_OAUTH_SCOPES`
 ///   4. key == "moderator" （旧 `[twitch.moderator]` ブロックを渡すのは [`for_moderator`] のみ）
 ///   5. その他 key → 空文字（ユーザーが明示必須）
 pub fn for_key(key: &str, es: &TwitchEventSubConfig, spec: Option<&TwitchTokenKeySpec>) -> Self {
  let key = key.trim();
  let label = if key.is_empty() { "unknown".to_string() } else { key.to_string() };

  // Client ID: spec.client_id > env VAC_TWITCH_CLIENT_ID > es.client_id > 同梱既定
  let client_id = spec
   .and_then(|s| s.client_id.as_deref().map(str::trim).filter(|s| !s.is_empty()))
   .map(|s| s.to_string())
   .unwrap_or_else(|| resolve_twitch_client_id(es));

  // Scopes: spec.scopes > ( broadcaster → es.oauth_scopes or DEFAULT_BROADCASTER ) > DEFAULT_MODERATOR for moderator > 空
  let scopes = spec
   .and_then(|s| s.scopes.as_deref().map(str::trim).filter(|s| !s.is_empty()))
   .map(|s| s.to_string())
   .or_else(|| match key {
    "broadcaster" => Some(
     es.oauth_scopes
      .as_deref()
      .map(str::trim)
      .filter(|s| !s.is_empty())
      .unwrap_or(DEFAULT_BROADCASTER_OAUTH_SCOPES)
      .to_string(),
    ),
    "moderator" => Some(DEFAULT_MODERATOR_OAUTH_SCOPES.to_string()),
    _ => None,
   })
   .unwrap_or_default();

  // timeout: spec > es (broadcaster) > default
  let timeout_secs = spec
   .and_then(|s| s.oauth_timeout_secs)
   .unwrap_or_else(|| {
    if key == "broadcaster" {
     es.oauth_timeout_secs
    } else {
     crate::conf::default_twitch_oauth_timeout_secs_value()
    }
   });

  // browser_command: spec > ( broadcaster → es.oauth_browser_command ) > None
  let browser_command = spec
   .and_then(|s| s.oauth_browser_command.as_deref().map(str::trim).filter(|s| !s.is_empty()).map(|s| s.to_string()))
   .or_else(|| {
    if key == "broadcaster" {
     es.oauth_browser_command
      .as_deref()
      .map(str::trim)
      .filter(|s| !s.is_empty())
      .map(|s| s.to_string())
    } else {
     None
    }
   });

  // 保存ファイル名と主要 env
  let safe_key = sanitize_key_for_filename(&label);
  let token_file_name = format!("twitch-token.{}.json", safe_key);
  let token_file_env = format!("VAC_TWITCH_TOKEN_FILE_{}", sanitize_key_for_env(&label));

  // Phase V 互換の legacy env 名
  let mut aliases: Vec<String> = Vec::new();
  match key {
   "broadcaster" => aliases.push(ENV_BROADCASTER_TOKEN_FILE.to_string()),
   "moderator" => aliases.push(ENV_MODERATOR_TOKEN_FILE.to_string()),
   _ => {},
  }

  Self {
   client_id,
   scopes,
   timeout_secs,
   token_file_env,
   token_file_env_aliases: aliases,
   token_file_name,
   label,
   browser_command,
  }
 }

 /// Phase ζ-1: `for_key("broadcaster", es, None)` の薄い shim。既存呼び出し元との互換。
 pub fn for_broadcaster(es: &TwitchEventSubConfig) -> Self {
  Self::for_key("broadcaster", es, None)
 }

 /// Phase ζ-1: `for_key("moderator", es, ...)` の互換 shim。
 ///
 /// `TwitchModeratorConfig` からは `oauth_scopes` / `oauth_timeout_secs` / `oauth_browser_command` /
 /// `user_access_token` を拾って、同等の `TwitchTokenKeySpec` を合成したうえで `for_key` に委譲する。
 pub fn for_moderator(es: &TwitchEventSubConfig, mc: &TwitchModeratorConfig) -> Self {
  let spec = TwitchTokenKeySpec {
   scopes: mc.oauth_scopes.clone(),
   client_id: None,
   oauth_timeout_secs: Some(mc.oauth_timeout_secs),
   oauth_browser_command: mc.oauth_browser_command.clone(),
   login_hint: mc.login.clone(),
   user_access_token: mc.user_access_token.clone(),
  };
  Self::for_key("moderator", es, Some(&spec))
 }
}

/// ブラウザを `ident.browser_command` に従って起動する。未指定なら OS 既定で開く。
pub(crate) fn launch_browser(ident: &OAuthIdent, url: &str) -> bool {
 if let Some(template) = ident.browser_command.as_deref() {
  let cmd_str = if template.contains("{url}") {
   template.replace("{url}", url)
  } else {
   format!("{} {}", template, url)
  };
  let parts = match shlex::split(&cmd_str) {
   Some(p) if !p.is_empty() => p,
   _ => {
    log::warn!(
     "《Twitch》 OAuth[{}]: oauth_browser_command の解析に失敗しました: {:?}",
     ident.label,
     template
    );
    return false;
   },
  };
  let (prog, args) = parts.split_first().unwrap();
  match std::process::Command::new(prog).args(args).spawn() {
   Ok(_child) => {
    log::info!(
     "《Twitch》 OAuth[{}]: カスタムブラウザコマンドを起動しました: {} {:?}",
     ident.label,
     prog,
     args
    );
    return true;
   },
   Err(e) => {
    log::warn!(
     "《Twitch》 OAuth[{}]: カスタムブラウザコマンド `{}` の起動に失敗: {} → OS 既定ブラウザにフォールバックします。",
     ident.label,
     prog,
     e
    );
   },
  }
 }
 webbrowser::open(url).is_ok()
}

/// `GET https://id.twitch.tv/oauth2/validate` が成功すればトークンは有効とみなす。
pub(crate) async fn validate_user_access_token(token: &str) -> Result<()> {
 let client = reqwest::Client::new();
 let res = client
  .get(VALIDATE_URL)
  .header("Authorization", format!("Bearer {}", token.trim()))
  .send()
  .await?;
 if res.status().is_success() {
  return Ok(());
 }
 let status = res.status();
 let body = res.text().await.unwrap_or_default();
 Err(anyhow!("OAuth validate が失敗しました: {} {}", status, body))
}

// ---------------------------------------------------------------------------
// 保存
// ---------------------------------------------------------------------------

/// ローカルファイルに保存するトークンセット。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StoredTokens {
 pub access_token: String,
 #[serde(default)]
 pub refresh_token: Option<String>,
 #[serde(default)]
 pub scope: Option<Vec<String>>,
 /// 取得時の Client ID（別 Client ID に切り替えたとき再認可させる目印）。
 #[serde(default)]
 pub client_id: Option<String>,
}

fn token_file_path(ident: &OAuthIdent) -> Result<PathBuf> {
 if let Ok(s) = std::env::var(&ident.token_file_env) {
  let s = s.trim();
  if !s.is_empty() {
   return Ok(PathBuf::from(s));
  }
 }
 for alias in &ident.token_file_env_aliases {
  if alias.is_empty() {
   continue;
  }
  if let Ok(s) = std::env::var(alias) {
   let s = s.trim();
   if !s.is_empty() {
    return Ok(PathBuf::from(s));
   }
  }
 }
 let base = dirs::config_dir().ok_or_else(|| anyhow!("OS の設定ディレクトリを取得できませんでした"))?;
 Ok(base.join("virtual-avatar-connect").join(&ident.token_file_name))
}

/// 旧ファイル `twitch-token.json` を broadcaster の新ファイル名へマイグレーションする（ベストエフォート）。
///
/// Phase V 以前は 1 アカウント単位 1 ファイルだった。broadcaster 用の token ファイルが存在せず、
/// 旧ファイルだけがあるなら、rename して引き継ぐ。失敗はログ警告で飲み込む（次回の対話認可に任せる）。
fn migrate_legacy_token_file_if_needed(ident: &OAuthIdent) {
 if ident.label != "broadcaster" {
  return;
 }
 let Ok(new_path) = token_file_path(ident) else {
  return;
 };
 if new_path.exists() {
  return;
 }
 let Some(dir) = new_path.parent() else { return };
 let legacy = dir.join("twitch-token.json");
 if !legacy.exists() {
  return;
 }
 if let Err(e) = std::fs::rename(&legacy, &new_path) {
  log::warn!(
   "《Twitch》 OAuth: 旧トークンファイル {} の {} への移行に失敗しました（初回は再認可が必要）: {}",
   legacy.display(),
   new_path.display(),
   e
  );
 } else {
  log::info!(
   "《Twitch》 OAuth: 旧トークンファイルを {} に移行しました（Phase V 2 アカウント化による破壊的変更）。",
   new_path.display()
  );
 }
}

pub(crate) fn load_stored_tokens(ident: &OAuthIdent) -> Option<StoredTokens> {
 migrate_legacy_token_file_if_needed(ident);
 let path = token_file_path(ident).ok()?;
 let bytes = std::fs::read(&path).ok()?;
 serde_json::from_slice::<StoredTokens>(&bytes).ok()
}

pub(crate) fn save_stored_tokens(ident: &OAuthIdent, tokens: &StoredTokens) -> Result<()> {
 let path = token_file_path(ident)?;
 if let Some(dir) = path.parent() {
  std::fs::create_dir_all(dir).with_context(|| format!("トークン保存ディレクトリ作成に失敗: {}", dir.display()))?;
 }
 let bytes = serde_json::to_vec_pretty(tokens).context("トークン JSON シリアライズに失敗")?;
 std::fs::write(&path, bytes).with_context(|| format!("トークン保存に失敗: {}", path.display()))?;
 log::debug!(
  "《Twitch》 OAuth[{}]: トークンを {} に保存しました（このファイルは秘匿してください）。",
  ident.label,
  path.display()
 );
 Ok(())
}

/// 保存済みトークンファイルを削除する。
///
/// - 戻り値 `Ok(Some(path))`: そのパスを削除した
/// - 戻り値 `Ok(None)`: 削除対象のファイルが存在しなかった
/// - 戻り値 `Err(_)`: パス解決に失敗した、または IO エラー
///
/// NOTE: これはファイルを消すだけで、既に起動中の eventsub 接続等がメモリ上に持っている
/// アクセストークンは無効化しない。それらは次回 refresh 時にファイルを読みにいって失敗する
/// ため、結果として再認可が必要になる。即時反映が欲しければ VAC 再起動、あるいは本削除と
/// 併せて Device Code Flow を再実行してください。
pub(crate) fn delete_stored_tokens(ident: &OAuthIdent) -> Result<Option<PathBuf>> {
 let path = token_file_path(ident)?;
 if !path.exists() {
  return Ok(None);
 }
 std::fs::remove_file(&path).with_context(|| format!("トークン削除に失敗: {}", path.display()))?;
 log::info!(
  "《Twitch》 OAuth[{}]: トークンファイル {} を削除しました。",
  ident.label,
  path.display()
 );
 Ok(Some(path))
}

// ---------------------------------------------------------------------------
// Device Code Flow
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize)]
pub(crate) struct DeviceCodeResponse {
 pub device_code: String,
 #[allow(dead_code)]
 pub expires_in: u64,
 pub interval: u64,
 pub user_code: String,
 pub verification_uri: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct TokenResponse {
 pub access_token: String,
 #[serde(default)]
 pub refresh_token: Option<String>,
 #[serde(default)]
 #[allow(dead_code)]
 pub expires_in: Option<u64>,
 #[serde(default)]
 pub scope: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
struct PendingErrorBody {
 #[serde(default)]
 message: Option<String>,
 #[serde(default)]
 status: Option<u32>,
}

pub(crate) async fn start_device_authorization(client_id: &str, scopes: &str) -> Result<DeviceCodeResponse> {
 let body = serde_urlencoded::to_string([("client_id", client_id), ("scopes", scopes)])
  .context("device 要求 body のシリアライズに失敗しました")?;
 let res = reqwest::Client::new()
  .post(DEVICE_URL)
  .header("Content-Type", "application/x-www-form-urlencoded")
  .body(body)
  .send()
  .await
  .context("device エンドポイントへの POST に失敗しました")?;
 if !res.status().is_success() {
  let status = res.status();
  let body = res.text().await.unwrap_or_default();
  return Err(anyhow!("device 認可開始に失敗しました: {} {}", status, body));
 }
 res
  .json::<DeviceCodeResponse>()
  .await
  .context("device レスポンスの JSON 解析に失敗しました")
}

pub(crate) async fn poll_device_token(
 client_id: &str,
 scopes: &str,
 device_code: &str,
 interval_secs: u64,
 deadline: tokio::time::Instant,
) -> Result<TokenResponse> {
 let interval = Duration::from_secs(interval_secs.max(1));
 let mut wait = interval;
 loop {
  if tokio::time::Instant::now() >= deadline {
   return Err(anyhow!("device 認可がタイムアウトしました（ユーザーが期限内に Twitch でコードを入力しませんでした）。"));
  }
  tokio::time::sleep(wait).await;

  let body = serde_urlencoded::to_string([
   ("client_id", client_id),
   ("scopes", scopes),
   ("device_code", device_code),
   ("grant_type", DEVICE_GRANT_TYPE),
  ])
  .context("token 要求 body のシリアライズに失敗しました")?;
  let res = reqwest::Client::new()
   .post(TOKEN_URL)
   .header("Content-Type", "application/x-www-form-urlencoded")
   .body(body)
   .send()
   .await
   .context("token エンドポイントへの POST に失敗しました")?;
  let status = res.status();
  let text = res.text().await.unwrap_or_default();

  if status.is_success() {
   let tr: TokenResponse =
    serde_json::from_str(&text).with_context(|| format!("token JSON の解析に失敗しました: {}", text))?;
   return Ok(tr);
  }

  // 400 系のうち pending / slow_down は継続。それ以外は致命。
  let parsed: Option<PendingErrorBody> = serde_json::from_str(&text).ok();
  let msg = parsed.as_ref().and_then(|b| b.message.as_deref()).unwrap_or("");
  if msg.eq_ignore_ascii_case("authorization_pending") {
   log::debug!("《Twitch》 OAuth: 認可待ち（authorization_pending）…{} 秒後に再試行します。", interval.as_secs());
   wait = interval;
   continue;
  }
  if msg.eq_ignore_ascii_case("slow_down") {
   wait = interval + Duration::from_secs(5);
   log::debug!("《Twitch》 OAuth: slow_down 指示を受けたため間隔を {} 秒に伸ばします。", wait.as_secs());
   continue;
  }
  let st = parsed.as_ref().and_then(|b| b.status).unwrap_or(status.as_u16() as u32);
  return Err(anyhow!("device token 取得に失敗しました: {} {}", st, text));
 }
}

/// **Device Code Flow** によりユーザーアクセストークンを対話的に取得する。
/// 取得成功時はトークンファイルへ保存して `access_token` を返す。
async fn obtain_user_access_token_interactive(ident: &OAuthIdent) -> Result<String> {
 if ident.client_id.is_empty() {
  return Err(anyhow!("《Twitch》 OAuth: Client ID を解決できませんでした。"));
 }

 log::info!(
  "《Twitch》 OAuth[{}]: Device Code Flow を開始します（公開クライアント・client_secret 不要・refresh_token 付き）。",
  ident.label
 );
 let device = start_device_authorization(&ident.client_id, &ident.scopes).await?;

 // ユーザーへの提示。verification_uri は通常 `https://www.twitch.tv/activate?public=true&device-code=<USERCODE>` を含む。
 log::info!(
  "《Twitch》 OAuth[{}]: ブラウザを開きます。**用途に合ったアカウント（{}）**でログイン後、表示された **コード `{}`** を入力して許可してください（自動入力されている場合はそのまま許可ボタンを押してください）。\n  もしブラウザが開かなければ、次の URL を手動で開いてください: {}",
  ident.label,
  ident.label,
  device.user_code,
  device.verification_uri
 );
 if !launch_browser(ident, &device.verification_uri) {
  log::warn!(
   "《Twitch》 OAuth[{}]: ブラウザを自動で開けませんでした。次の URL を手動で開いてコード `{}` を入力してください: {}",
   ident.label,
   device.user_code,
   device.verification_uri
  );
 }

 let timeout = Duration::from_secs(ident.timeout_secs.max(60));
 let deadline = tokio::time::Instant::now() + timeout;
 let tr = poll_device_token(&ident.client_id, &ident.scopes, &device.device_code, device.interval, deadline).await?;

 let stored = StoredTokens {
  access_token: tr.access_token.clone(),
  refresh_token: tr.refresh_token,
  scope: tr.scope,
  client_id: Some(ident.client_id.clone()),
 };
 if let Err(e) = save_stored_tokens(ident, &stored) {
  log::warn!(
   "《Twitch》 OAuth[{}]: トークン保存に失敗しました（次回起動時に再認可が必要になります）: {}",
   ident.label,
   e
  );
 } else {
  log::info!(
   "《Twitch》 OAuth[{}] 完了: ユーザーアクセストークンを保存しました。次回からは保存ファイルから自動読込・自動リフレッシュします（トークン全文はログに出しません）。",
   ident.label
  );
 }

 Ok(stored.access_token)
}

// ---------------------------------------------------------------------------
// Refresh
// ---------------------------------------------------------------------------

/// `refresh_token` で `access_token` を更新する。**Twitch の DCF refresh_token は 1 回限り**で、新しい refresh_token が返るので必ず保存し直す。
async fn refresh_access_token(client_id: &str, refresh_token: &str) -> Result<TokenResponse> {
 let body = serde_urlencoded::to_string([
  ("client_id", client_id),
  ("grant_type", "refresh_token"),
  ("refresh_token", refresh_token),
 ])
 .context("refresh 要求 body のシリアライズに失敗しました")?;
 let res = reqwest::Client::new()
  .post(TOKEN_URL)
  .header("Content-Type", "application/x-www-form-urlencoded")
  .body(body)
  .send()
  .await
  .context("token エンドポイントへの POST に失敗しました")?;
 if !res.status().is_success() {
  let status = res.status();
  let body = res.text().await.unwrap_or_default();
  return Err(anyhow!("refresh に失敗しました: {} {}", status, body));
 }
 res
  .json::<TokenResponse>()
  .await
  .context("refresh レスポンスの JSON 解析に失敗しました")
}

/// 保存トークンを読み出して、可能なら使う／更新する／作り直す（共通ロジック）。
/// 保存済みトークン（およびその refresh）だけで非対話にユーザーアクセストークンを取得する。
/// 取得不能（未保存・refresh 失敗など）の場合は [`None`] を返し、**DCF は決して起動しない**。
///
/// 起動時 init からこれを呼ぶことで、未認可のモデレーターアカウントに 10 分間ブロックされるのを防ぐ。
pub(crate) async fn try_load_valid_token_for(ident: &OAuthIdent) -> Option<String> {
 let stored = load_stored_tokens(ident)?;
 let same_client = stored.client_id.as_deref().map(|c| c == ident.client_id).unwrap_or(true);
 if !same_client {
  log::info!(
   "《Twitch》 OAuth[{}]: 保存トークンの Client ID が現在の Client ID と異なるため利用不可。",
   ident.label
  );
  return None;
 }
 if validate_user_access_token(&stored.access_token).await.is_ok() {
  return Some(stored.access_token);
 }
 let rt = stored.refresh_token.as_deref().filter(|s| !s.trim().is_empty())?;
 log::info!(
  "《Twitch》 OAuth[{}]: アクセストークン無効。refresh_token で更新を試みます（非対話）。",
  ident.label
 );
 match refresh_access_token(&ident.client_id, rt).await {
  Ok(tr) => {
   let new_stored = StoredTokens {
    access_token: tr.access_token.clone(),
    refresh_token: tr.refresh_token.or_else(|| Some(rt.to_string())),
    scope: tr.scope.or(stored.scope),
    client_id: Some(ident.client_id.clone()),
   };
   if let Err(e) = save_stored_tokens(ident, &new_stored) {
    log::warn!("《Twitch》 OAuth[{}]: 更新後のトークン保存に失敗: {}", ident.label, e);
   }
   Some(new_stored.access_token)
  },
  Err(e) => {
   log::warn!(
    "《Twitch》 OAuth[{}]: refresh に失敗（非対話のため DCF には移行しません）: {}",
    ident.label,
    e
   );
   None
  },
 }
}

async fn ensure_token_for(ident: &OAuthIdent) -> Result<String> {
 if let Some(stored) = load_stored_tokens(ident) {
  let same_client = stored
   .client_id
   .as_deref()
   .map(|c| c == ident.client_id)
   .unwrap_or(true);
  if same_client {
   if validate_user_access_token(&stored.access_token).await.is_ok() {
    return Ok(stored.access_token);
   }
   if let Some(rt) = stored.refresh_token.as_deref().filter(|s| !s.trim().is_empty()) {
    log::info!(
     "《Twitch》 OAuth[{}]: アクセストークンが無効でした。refresh_token で更新を試みます。",
     ident.label
    );
    match refresh_access_token(&ident.client_id, rt).await {
     Ok(tr) => {
      let new_stored = StoredTokens {
       access_token: tr.access_token.clone(),
       refresh_token: tr.refresh_token.or_else(|| Some(rt.to_string())),
       scope: tr.scope.or(stored.scope),
       client_id: Some(ident.client_id.clone()),
      };
      if let Err(e) = save_stored_tokens(ident, &new_stored) {
       log::warn!("《Twitch》 OAuth[{}]: 更新後のトークン保存に失敗: {}", ident.label, e);
      } else {
       log::info!(
        "《Twitch》 OAuth[{}]: refresh によりアクセストークンを更新しました。",
        ident.label
       );
      }
      return Ok(new_stored.access_token);
     },
     Err(e) => {
      log::warn!(
       "《Twitch》 OAuth[{}]: refresh に失敗したため Device Code Flow をやり直します: {}",
       ident.label,
       e
      );
     },
    }
   } else {
    log::info!(
     "《Twitch》 OAuth[{}]: 保存トークンに refresh_token がありません。Device Code Flow をやり直します。",
     ident.label
    );
   }
  } else {
   log::info!(
    "《Twitch》 OAuth[{}]: 保存トークンの Client ID が現在の Client ID と異なるため再認可します。",
    ident.label
   );
  }
 }

 obtain_user_access_token_interactive(ident).await
}

/// 配信者アカウントのユーザーアクセストークンを保証する。既存呼び出し元（EventSub / category 変更等）はこちらを使う。
pub(crate) async fn ensure_user_access_token(es: &TwitchEventSubConfig) -> Result<String> {
 ensure_token_for(&OAuthIdent::for_broadcaster(es)).await
}

/// 保存済みモデレータートークンを **非対話に** 取得。起動時の init_processors から使う。
/// DCF を伴わないので、未認可でも起動をブロックしない。
pub(crate) async fn try_load_moderator_access_token(
 es: &TwitchEventSubConfig,
 mc: &TwitchModeratorConfig,
) -> Option<String> {
 // まず手動指定トークン（環境変数 or conf）を試す。
 let manual = std::env::var("VAC_TWITCH_MODERATOR_USER_ACCESS_TOKEN")
  .ok()
  .filter(|s| !s.trim().is_empty())
  .or_else(|| mc.user_access_token.clone().filter(|s| !s.trim().is_empty()));
 if let Some(t) = manual {
  if validate_user_access_token(&t).await.is_ok() {
   return Some(t);
  }
 }
 try_load_valid_token_for(&OAuthIdent::for_moderator(es, mc)).await
}

/// モデレーター/ボットアカウントのユーザーアクセストークンを保証する。`twitch_out` プロセッサーと AI のモデレーション系ツールが使う。
pub(crate) async fn ensure_moderator_access_token(
 es: &TwitchEventSubConfig,
 mc: &TwitchModeratorConfig,
) -> Result<String> {
 if !mc.oauth_auto
  && mc
   .user_access_token
   .as_deref()
   .filter(|s| !s.trim().is_empty())
   .is_none()
  && std::env::var("VAC_TWITCH_MODERATOR_USER_ACCESS_TOKEN")
   .ok()
   .filter(|s| !s.trim().is_empty())
   .is_none()
 {
  return Err(anyhow!(
   "《Twitch》 OAuth[moderator]: oauth_auto=false かつ手動トークン未設定のため起動できません。"
  ));
 }

 // 手動指定された user_access_token があれば validate を試し、通れば採用（既存 broadcaster と同ポリシー）。
 let manual = std::env::var("VAC_TWITCH_MODERATOR_USER_ACCESS_TOKEN")
  .ok()
  .filter(|s| !s.trim().is_empty())
  .or_else(|| mc.user_access_token.clone().filter(|s| !s.trim().is_empty()));
 if let Some(t) = manual {
  if validate_user_access_token(&t).await.is_ok() {
   return Ok(t);
  } else {
   log::warn!(
    "《Twitch》 OAuth[moderator]: 手動指定トークンが無効または期限切れでした。保存トークン / Device Code にフォールバックします。"
   );
  }
 }

 ensure_token_for(&OAuthIdent::for_moderator(es, mc)).await
}
