//! Twitch 連携 EffectfulNode 群（δ-4d）。
//!
//! V1 `TwitchOut` が単一 processor に詰め込んでいた責務（token validate / user_id 解決 /
//! chat 送信 / rate limit / strip / clip / refresh-on-error）を、**再利用可能な小さいノード
//! へ分解**する方針で Flowgraph 化したもの。token refresh や broadcaster_id 解決は
//! **graph-level** で `state.latch` + `on_error` ブランチを使って組み立てる設計。
//!
//! ## 含まれるノード
//!
//! - `flowgraph.twitch.validate_token`: `GET https://id.twitch.tv/oauth2/validate`。
//!   token 所有者（bot 本体）の `user_id` と `login` を取得する。
//! - `flowgraph.twitch.user_id_by_login`: Helix `GET /users?login=...`。
//!   broadcaster_login から broadcaster_id を解決する。
//! - `flowgraph.twitch.chat_send`: Helix `POST /chat/messages`。実際のチャット送信。
//!   V1 の `max_chars` / `strip_substrings` は optional 入力で互換。
//! - `flowgraph.twitch.ban`: Helix `POST /moderation/bans`（`duration` なし＝永久 ban）。
//! - `flowgraph.twitch.timeout`: 同 `POST /moderation/bans`（`duration` 指定＝時間制限つき ban）。
//!   API 本体は `ban` と完全共有、`duration_secs` 入力だけが違う。
//!
//! ## 関連ノード
//!
//! - `flowgraph.util.rate_limit`: V1 の 30s 窓レート制御は本ノード上流に挿入して表現する。
//! - `flowgraph.state.latch`: token / broadcaster_id のキャッシュに使う。

use crate::flowgraph::node::{
	get_optional_int, get_optional_string, get_required_int, get_required_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap,
	NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};

const HELIX: &str = "https://api.twitch.tv/helix";
const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const DEFAULT_MAX_CHARS: i64 = 500;
const DEFAULT_TIMEOUT_SECS: u64 = 30;

fn http_client() -> Result<reqwest::Client, String> {
	reqwest::Client::builder()
		.timeout(std::time::Duration::from_secs(DEFAULT_TIMEOUT_SECS))
		.build()
		.map_err(|e| format!("reqwest client: {e}"))
}

/// UTF-8 文字単位で `max_chars` に収まるようにクリップ。`max_chars <= 0` なら変更なし。
fn clip_chars(s: &str, max_chars: i64) -> String {
	if max_chars <= 0 || (s.chars().count() as i64) <= max_chars {
		return s.to_string();
	}
	s.chars().take(max_chars as usize).collect()
}

/// `strip_substrings` 入力（`List<String>`）を取得。欠損 or 型不正なら空。
fn strip_substrings(inputs: &InputMap) -> Vec<String> {
	let Some(v) = inputs.get("strip_substrings") else {
		return Vec::new();
	};
	let Ok(list) = v.as_list() else { return Vec::new() };
	list.iter()
		.filter_map(|e| e.as_str().ok().map(str::to_owned))
		.filter(|s| !s.is_empty())
		.collect()
}

// ---------------------------------------------------------------------
// flowgraph.twitch.get_token
// ---------------------------------------------------------------------
//
// 指定した `token_key`（例: "broadcaster" / "moderator" / conf.twitch.token_keys に
// 列挙された任意のキー）に対して、`try_load_valid_token_for` で保存済みトークンの
// 有効性を確認し、取得できれば `on_success` を発火して `access_token` / `client_id`
// をデータ OUT する。取得できなかった場合は `on_failure` を発火して `error` に
// 理由を入れる。
//
// 本ノードは `ExecCtx::state_handle` が必要。`state_handle` が `None`
// （テスト等）では常に `on_failure` を返す。

pub struct GetTokenNode;

impl NodeDescriptor for GetTokenNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.twitch.get_token".into(),
			title: "Twitch: Get Token".into(),
			category: "twitch".into(),
			description: Some(
				"conf.twitch で定義された token_key から保存済み OAuth トークンを取り出し、on_success / on_failure で分岐する".into(),
			),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("token_key", "Token Key", SocketType::String),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_failure", "On Failure"),
				PortSpec::output("access_token", "Access Token", SocketType::String),
				PortSpec::output("client_id", "Client ID", SocketType::String),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

fn get_token_fail(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("access_token", SocketValue::String(String::new()))
		.set_data("client_id", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_failure")
}

#[async_trait]
impl EffectfulNode for GetTokenNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let key_raw = get_required_string(inputs, "token_key")?;
		let key = key_raw.trim().to_string();
		if key.is_empty() {
			return Ok(get_token_fail("token_key が空です"));
		}

		let Some(weak) = ctx.state_handle.as_ref() else {
			ctx.log("twitch.get_token: state_handle が None のため trigger 不能 (on_failure)");
			return Ok(get_token_fail("state_handle is unavailable"));
		};
		let Some(state_arc) = weak.upgrade() else {
			ctx.log("twitch.get_token: State が既に drop されている (on_failure)");
			return Ok(get_token_fail("state has already been dropped"));
		};

		let (ident, client_id_for_output) = {
			let s = state_arc.read().await;
			let Some(twitch) = s.twitch.as_ref() else {
				return Ok(get_token_fail("[twitch] section is not configured"));
			};
			let Some(es) = twitch.eventsub.as_ref() else {
				return Ok(get_token_fail("[twitch.eventsub] section is not configured"));
			};
			let spec = twitch.token_spec(&key);
			let ident = crate::twitch::oauth::OAuthIdent::for_key(&key, es, spec);
			let cid = ident.client_id.clone();
			(ident, cid)
		};

		if ident.client_id.is_empty() {
			return Ok(get_token_fail(format!(
				"twitch client_id is empty for token_key='{}' (set VAC_TWITCH_CLIENT_ID or [twitch.eventsub].client_id)",
				key
			)));
		}

		match crate::twitch::oauth::try_load_valid_token_for(&ident).await {
			Some(token) => {
				ctx.log(format!("twitch.get_token: resolved token for key='{}'", key));
				Ok(NodeOutput::new()
					.set_data("access_token", SocketValue::String(token))
					.set_data("client_id", SocketValue::String(client_id_for_output))
					.set_data("error", SocketValue::String(String::new()))
					.fire_exec("on_success"))
			}
			None => {
				let msg = format!("no valid stored token for token_key='{}'", key);
				ctx.log(format!("twitch.get_token: {}", msg));
				Ok(get_token_fail(msg))
			}
		}
	}
}

// ---------------------------------------------------------------------
// flowgraph.twitch.validate_token
// ---------------------------------------------------------------------

pub struct ValidateTokenNode;

impl NodeDescriptor for ValidateTokenNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.twitch.validate_token".into(),
			title: "Twitch: Validate Token".into(),
			category: "twitch".into(),
			description: Some("GET https://id.twitch.tv/oauth2/validate で token 所有者の user_id / login / client_id を取得する".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("access_token", "Access Token", SocketType::String),
				PortSpec::input("endpoint", "Endpoint", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("user_id", "User ID", SocketType::String),
				PortSpec::output("login", "Login", SocketType::String),
				PortSpec::output("client_id", "Client ID", SocketType::String),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

fn validate_err(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("user_id", SocketValue::String(String::new()))
		.set_data("login", SocketValue::String(String::new()))
		.set_data("client_id", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

#[async_trait]
impl EffectfulNode for ValidateTokenNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let token = get_required_string(inputs, "access_token")?;
		if token.trim().is_empty() {
			return Ok(validate_err("access_token が空です"));
		}
		let endpoint = get_optional_string(inputs, "endpoint", "")?;
		let url = if endpoint.trim().is_empty() {
			VALIDATE_URL.to_string()
		} else {
			endpoint.trim().to_string()
		};

		let client = match http_client() {
			Ok(c) => c,
			Err(e) => return Ok(validate_err(e)),
		};

		let resp = match client
			.get(&url)
			.header("Authorization", format!("Bearer {}", token.trim()))
			.send()
			.await
		{
			Ok(r) => r,
			Err(e) => return Ok(validate_err(format!("validate 送信失敗: {e}"))),
		};
		let status = resp.status();
		let text = resp.text().await.unwrap_or_default();
		if !status.is_success() {
			return Ok(validate_err(format!("validate HTTP {status}: {text}")));
		}
		let v: JsonValue = match serde_json::from_str(&text) {
			Ok(v) => v,
			Err(e) => return Ok(validate_err(format!("validate JSON 解析失敗: {e}: {text}"))),
		};
		let user_id = v.get("user_id").and_then(JsonValue::as_str).unwrap_or_default().to_string();
		let login = v.get("login").and_then(JsonValue::as_str).unwrap_or_default().to_string();
		let client_id = v.get("client_id").and_then(JsonValue::as_str).unwrap_or_default().to_string();
		if user_id.is_empty() || login.is_empty() {
			return Ok(validate_err(format!("validate レスポンスに user_id/login が欠落: {text}")));
		}
		Ok(NodeOutput::new()
			.set_data("user_id", SocketValue::String(user_id))
			.set_data("login", SocketValue::String(login))
			.set_data("client_id", SocketValue::String(client_id))
			.set_data("error", SocketValue::String(String::new()))
			.fire_exec("on_success"))
	}
}

// ---------------------------------------------------------------------
// flowgraph.twitch.user_id_by_login
// ---------------------------------------------------------------------

pub struct UserIdByLoginNode;

impl NodeDescriptor for UserIdByLoginNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.twitch.user_id_by_login".into(),
			title: "Twitch: User ID by Login".into(),
			category: "twitch".into(),
			description: Some("Helix GET /users?login=... で login から user_id を解決する".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("login", "Login", SocketType::String),
				PortSpec::input("access_token", "Access Token", SocketType::String),
				PortSpec::input("client_id", "Client ID", SocketType::String),
				PortSpec::input("endpoint", "Endpoint", SocketType::String).with_default(SocketValue::String(String::new())),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("user_id", "User ID", SocketType::String),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

fn user_id_err(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("user_id", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

#[async_trait]
impl EffectfulNode for UserIdByLoginNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let login = get_required_string(inputs, "login")?;
		let token = get_required_string(inputs, "access_token")?;
		let client_id = get_required_string(inputs, "client_id")?;
		let normalized = login.trim().trim_start_matches('#').to_lowercase();
		if normalized.is_empty() {
			return Ok(user_id_err("login が空です"));
		}

		let endpoint = get_optional_string(inputs, "endpoint", "")?;
		let base = if endpoint.trim().is_empty() { HELIX } else { endpoint.trim() };
		let client = match http_client() {
			Ok(c) => c,
			Err(e) => return Ok(user_id_err(e)),
		};
		let url = format!("{base}/users?login={}", urlencoding::encode(&normalized));
		let resp = match client
			.get(&url)
			.header("Client-Id", client_id)
			.bearer_auth(token.trim())
			.send()
			.await
		{
			Ok(r) => r,
			Err(e) => return Ok(user_id_err(format!("helix users 送信失敗: {e}"))),
		};
		let status = resp.status();
		let text = resp.text().await.unwrap_or_default();
		if !status.is_success() {
			return Ok(user_id_err(format!("helix users HTTP {status}: {text}")));
		}
		let v: JsonValue = match serde_json::from_str(&text) {
			Ok(v) => v,
			Err(e) => return Ok(user_id_err(format!("helix users JSON 解析失敗: {e}: {text}"))),
		};
		let uid = v
			.get("data")
			.and_then(JsonValue::as_array)
			.and_then(|a| a.first())
			.and_then(|u| u.get("id"))
			.and_then(JsonValue::as_str)
			.map(str::to_owned)
			.unwrap_or_default();
		if uid.is_empty() {
			return Ok(user_id_err(format!("login '{normalized}' の user_id が見つかりません: {text}")));
		}
		Ok(NodeOutput::new()
			.set_data("user_id", SocketValue::String(uid))
			.set_data("error", SocketValue::String(String::new()))
			.fire_exec("on_success"))
	}
}

// ---------------------------------------------------------------------
// flowgraph.twitch.chat_send
// ---------------------------------------------------------------------

pub struct ChatSendNode;

impl NodeDescriptor for ChatSendNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
   feature: "flowgraph.twitch.chat_send".into(),
   title: "Twitch: Chat Send".into(),
   category: "twitch".into(),
   description: Some(
    "Helix POST /chat/messages でチャット送信。max_chars/strip_substrings は optional で V1 互換。レート制限は flowgraph.util.rate_limit を上流に挿入して実現する".into(),
   ),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("text", "Text", SocketType::String),
    PortSpec::input("broadcaster_id", "Broadcaster ID", SocketType::String),
    PortSpec::input("sender_user_id", "Sender User ID", SocketType::String),
    PortSpec::input("access_token", "Access Token", SocketType::String),
    PortSpec::input("client_id", "Client ID", SocketType::String),
    PortSpec::input("max_chars", "Max Chars", SocketType::Int).with_default(SocketValue::Int(DEFAULT_MAX_CHARS)),
    PortSpec::input("strip_substrings", "Strip Substrings", SocketType::List(Box::new(SocketType::String)))
     .with_default(SocketValue::List(Vec::new())),
    PortSpec::input("endpoint", "Endpoint", SocketType::String).with_default(SocketValue::String(String::new())),
   ],
   outputs: vec![
    PortSpec::exec_output("on_success", "On Success"),
    PortSpec::exec_output("on_error", "On Error"),
    PortSpec::exec_output("on_skipped", "On Skipped"),
    PortSpec::output("sent_text", "Sent Text", SocketType::String),
    PortSpec::output("error", "Error", SocketType::String),
   ],
   properties: vec![],
  }
	}
}

fn chat_send_err(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("sent_text", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

fn chat_send_skipped() -> NodeOutput {
	NodeOutput::new()
		.set_data("sent_text", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(String::new()))
		.fire_exec("on_skipped")
}

#[async_trait]
impl EffectfulNode for ChatSendNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let text_raw = get_required_string(inputs, "text")?;
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let sender_user_id = get_required_string(inputs, "sender_user_id")?;
		let access_token = get_required_string(inputs, "access_token")?;
		let client_id = get_required_string(inputs, "client_id")?;
		let max_chars = get_optional_int(inputs, "max_chars", DEFAULT_MAX_CHARS)?.max(1);
		let strip = strip_substrings(inputs);

		let mut text = text_raw.trim().to_string();
		for s in &strip {
			text = text.replace(s, "");
		}
		let text = clip_chars(text.trim(), max_chars);
		if text.is_empty() {
			// V1: 「送信本文が空のためスキップ」はエラーではなく on_skipped 扱いにする
			return Ok(chat_send_skipped());
		}
		if broadcaster_id.trim().is_empty() || sender_user_id.trim().is_empty() {
			return Ok(chat_send_err("broadcaster_id / sender_user_id が空です"));
		}
		if access_token.trim().is_empty() || client_id.trim().is_empty() {
			return Ok(chat_send_err("access_token / client_id が空です"));
		}

		let endpoint = get_optional_string(inputs, "endpoint", "")?;
		let base = if endpoint.trim().is_empty() { HELIX } else { endpoint.trim() };
		let client = match http_client() {
			Ok(c) => c,
			Err(e) => return Ok(chat_send_err(e)),
		};
		let url = format!("{base}/chat/messages");
		let body = json!({
		 "broadcaster_id": broadcaster_id.trim(),
		 "sender_id": sender_user_id.trim(),
		 "message": &text,
		});
		let resp = match client
			.post(&url)
			.header("Client-Id", client_id.trim())
			.bearer_auth(access_token.trim())
			.json(&body)
			.send()
			.await
		{
			Ok(r) => r,
			Err(e) => return Ok(chat_send_err(format!("POST /chat/messages 送信失敗: {e}"))),
		};
		let status = resp.status();
		let body_text = resp.text().await.unwrap_or_default();
		if !status.is_success() {
			return Ok(chat_send_err(format!("POST /chat/messages HTTP {status}: {body_text}")));
		}
		Ok(NodeOutput::new()
			.set_data("sent_text", SocketValue::String(text))
			.set_data("error", SocketValue::String(String::new()))
			.fire_exec("on_success"))
	}
}

// ---------------------------------------------------------------------
// flowgraph.twitch.ban / flowgraph.twitch.timeout
// ---------------------------------------------------------------------
//
// Helix `POST /moderation/bans` は ban / timeout で同一エンドポイント。
// - ban       = body.data.duration 無し（永久）
// - timeout   = body.data.duration = 1..=1209600（秒、最大 2 週間）
//
// 仕様上 `reason` は 500 文字までなので、chat_send と同じく clip_chars で正規化する。

const MODERATION_REASON_MAX_CHARS: i64 = 500;
/// Twitch 公式の timeout 上限（2 週間 = 1,209,600 秒）
const TIMEOUT_MAX_SECS: i64 = 1_209_600;

fn moderation_ban_inputs(with_duration: bool) -> Vec<PortSpec> {
	let mut inputs = vec![
		PortSpec::exec_input("exec_in", "Exec"),
		PortSpec::input("user_id", "Target User ID", SocketType::String),
		PortSpec::input("broadcaster_id", "Broadcaster ID", SocketType::String),
		PortSpec::input("moderator_id", "Moderator User ID", SocketType::String),
		PortSpec::input("access_token", "Access Token", SocketType::String),
		PortSpec::input("client_id", "Client ID", SocketType::String),
		PortSpec::input("reason", "Reason", SocketType::String).with_default(SocketValue::String(String::new())),
		PortSpec::input("endpoint", "Endpoint", SocketType::String).with_default(SocketValue::String(String::new())),
	];
	if with_duration {
		inputs.push(PortSpec::input("duration_secs", "Duration (seconds)", SocketType::Int));
	}
	inputs
}

fn moderation_ban_outputs() -> Vec<PortSpec> {
	vec![
		PortSpec::exec_output("on_success", "On Success"),
		PortSpec::exec_output("on_error", "On Error"),
		PortSpec::output("end_time", "End Time (ISO8601)", SocketType::String),
		PortSpec::output("error", "Error", SocketType::String),
	]
}

fn moderation_err(msg: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("end_time", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.into()))
		.fire_exec("on_error")
}

/// `duration_secs` が `Some(d)` で d>0 なら timeout、None/0 以下なら永久 ban。
async fn execute_moderation_ban(inputs: &InputMap, duration_secs: Option<i64>) -> Result<NodeOutput, NodeExecError> {
	let user_id = get_required_string(inputs, "user_id")?;
	let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
	let moderator_id = get_required_string(inputs, "moderator_id")?;
	let access_token = get_required_string(inputs, "access_token")?;
	let client_id = get_required_string(inputs, "client_id")?;
	let reason_raw = get_optional_string(inputs, "reason", "")?;
	let reason = clip_chars(reason_raw.trim(), MODERATION_REASON_MAX_CHARS);

	if user_id.trim().is_empty() {
		return Ok(moderation_err("user_id が空です"));
	}
	if broadcaster_id.trim().is_empty() || moderator_id.trim().is_empty() {
		return Ok(moderation_err("broadcaster_id / moderator_id が空です"));
	}
	if access_token.trim().is_empty() || client_id.trim().is_empty() {
		return Ok(moderation_err("access_token / client_id が空です"));
	}

	let mut body_data = serde_json::Map::new();
	body_data.insert("user_id".into(), JsonValue::String(user_id.trim().to_string()));
	if !reason.is_empty() {
		body_data.insert("reason".into(), JsonValue::String(reason));
	}
	if let Some(d) = duration_secs {
		if !(1..=TIMEOUT_MAX_SECS).contains(&d) {
			return Ok(moderation_err(format!(
				"duration_secs は 1..={TIMEOUT_MAX_SECS} の範囲でなければなりません（指定値: {d}）"
			)));
		}
		body_data.insert("duration".into(), JsonValue::Number(d.into()));
	}
	let body = json!({ "data": JsonValue::Object(body_data) });

	let endpoint = get_optional_string(inputs, "endpoint", "")?;
	let base = if endpoint.trim().is_empty() { HELIX } else { endpoint.trim() };
	let client = match http_client() {
		Ok(c) => c,
		Err(e) => return Ok(moderation_err(e)),
	};
	let url = format!(
		"{base}/moderation/bans?broadcaster_id={}&moderator_id={}",
		urlencoding::encode(broadcaster_id.trim()),
		urlencoding::encode(moderator_id.trim()),
	);
	let resp = match client
		.post(&url)
		.header("Client-Id", client_id.trim())
		.bearer_auth(access_token.trim())
		.json(&body)
		.send()
		.await
	{
		Ok(r) => r,
		Err(e) => return Ok(moderation_err(format!("POST /moderation/bans 送信失敗: {e}"))),
	};
	let status = resp.status();
	let body_text = resp.text().await.unwrap_or_default();
	if !status.is_success() {
		return Ok(moderation_err(format!("POST /moderation/bans HTTP {status}: {body_text}")));
	}
	// レスポンスから end_time（timeout のみ値あり、ban は null）を拾う。
	let end_time = serde_json::from_str::<JsonValue>(&body_text)
		.ok()
		.as_ref()
		.and_then(|v| v.get("data"))
		.and_then(JsonValue::as_array)
		.and_then(|a| a.first())
		.and_then(|row| row.get("end_time"))
		.and_then(JsonValue::as_str)
		.map(str::to_owned)
		.unwrap_or_default();
	Ok(NodeOutput::new()
		.set_data("end_time", SocketValue::String(end_time))
		.set_data("error", SocketValue::String(String::new()))
		.fire_exec("on_success"))
}

pub struct BanNode;

impl NodeDescriptor for BanNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.twitch.ban".into(),
			title: "Twitch: Ban User".into(),
			category: "twitch".into(),
			description: Some(
				"Helix POST /moderation/bans（永久 ban）。`duration` なしで送る。timeout は `twitch.timeout` ノードを使うこと".into(),
			),
			inputs: moderation_ban_inputs(false),
			outputs: moderation_ban_outputs(),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for BanNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		execute_moderation_ban(inputs, None).await
	}
}

pub struct TimeoutNode;

impl NodeDescriptor for TimeoutNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.twitch.timeout".into(),
			title: "Twitch: Timeout User".into(),
			category: "twitch".into(),
			description: Some(
				"Helix POST /moderation/bans（時間制限 ban）。`duration_secs` は 1..=1209600（最大 2 週間）。永久 ban は `twitch.ban`"
					.into(),
			),
			inputs: moderation_ban_inputs(true),
			outputs: moderation_ban_outputs(),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for TimeoutNode {
	async fn execute(
		&self,
		_ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let duration = get_required_int(inputs, "duration_secs")?;
		execute_moderation_ban(inputs, Some(duration)).await
	}
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	fn fired(port: &str) -> ExecFireSet {
		let mut f = ExecFireSet::new();
		f.insert(port);
		f
	}

	#[test]
	fn clip_chars_respects_boundary() {
		assert_eq!(clip_chars("hello", 10), "hello");
		assert_eq!(clip_chars("hello", 3), "hel");
		assert_eq!(clip_chars("こんにちは", 3), "こんに");
		assert_eq!(clip_chars("anything", 0), "anything"); // 0 以下は no-op
	}

	#[test]
	fn strip_substrings_extracts_and_filters_empty() {
		let mut m = InputMap::new();
		m.insert(
			"strip_substrings".into(),
			SocketValue::List(vec![
				SocketValue::String("aa".into()),
				SocketValue::String("".into()),
				SocketValue::String("bb".into()),
			]),
		);
		let got = strip_substrings(&m);
		assert_eq!(got, vec!["aa", "bb"]);
	}

	#[tokio::test]
	async fn get_token_noop_without_fire() {
		let n = GetTokenNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("token_key".into(), SocketValue::String("broadcaster".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[tokio::test]
	async fn get_token_empty_key_fires_on_failure() {
		let n = GetTokenNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("token_key".into(), SocketValue::String("  ".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_failure"));
		let err = out
			.data
			.get("error")
			.and_then(|v| v.as_str().ok().map(str::to_owned))
			.unwrap_or_default();
		assert!(err.contains("token_key"));
	}

	#[tokio::test]
	async fn get_token_without_state_handle_fires_on_failure() {
		let n = GetTokenNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("token_key".into(), SocketValue::String("broadcaster".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_failure"));
		assert!(ctx.trace.iter().any(|l| l.contains("state_handle")));
	}

	#[tokio::test]
	async fn validate_token_noop_without_fire() {
		let n = ValidateTokenNode;
		let mut ctx = ExecCtx::default();
		let out = n
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[tokio::test]
	async fn validate_token_empty_token_yields_error() {
		let n = ValidateTokenNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("access_token".into(), SocketValue::String("   ".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[tokio::test]
	async fn user_id_empty_login_yields_error() {
		let n = UserIdByLoginNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("login".into(), SocketValue::String("   #  ".into()));
		inputs.insert("access_token".into(), SocketValue::String("tok".into()));
		inputs.insert("client_id".into(), SocketValue::String("cid".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[tokio::test]
	async fn chat_send_empty_after_strip_fires_on_skipped() {
		let n = ChatSendNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("text".into(), SocketValue::String("aaBBaa".into()));
		inputs.insert("broadcaster_id".into(), SocketValue::String("1".into()));
		inputs.insert("sender_user_id".into(), SocketValue::String("2".into()));
		inputs.insert("access_token".into(), SocketValue::String("t".into()));
		inputs.insert("client_id".into(), SocketValue::String("c".into()));
		inputs.insert(
			"strip_substrings".into(),
			SocketValue::List(vec![SocketValue::String("aa".into()), SocketValue::String("BB".into())]),
		);
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_skipped"));
	}

	#[tokio::test]
	async fn chat_send_requires_non_empty_ids() {
		let n = ChatSendNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("text".into(), SocketValue::String("hi".into()));
		inputs.insert("broadcaster_id".into(), SocketValue::String("  ".into()));
		inputs.insert("sender_user_id".into(), SocketValue::String("2".into()));
		inputs.insert("access_token".into(), SocketValue::String("t".into()));
		inputs.insert("client_id".into(), SocketValue::String("c".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out
			.data
			.get("error")
			.and_then(|v| v.as_str().ok().map(str::to_owned))
			.unwrap_or_default();
		assert!(err.contains("broadcaster_id"));
	}

	#[tokio::test]
	async fn chat_send_unreachable_endpoint_is_non_fatal() {
		let n = ChatSendNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("text".into(), SocketValue::String("hello".into()));
		inputs.insert("broadcaster_id".into(), SocketValue::String("1".into()));
		inputs.insert("sender_user_id".into(), SocketValue::String("2".into()));
		inputs.insert("access_token".into(), SocketValue::String("t".into()));
		inputs.insert("client_id".into(), SocketValue::String("c".into()));
		// 未使用な localhost ポートに向ける（connection refused で速やかに on_error 化）
		inputs.insert("endpoint".into(), SocketValue::String("http://127.0.0.1:1".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"), "expected on_error, got: {:?}", out.fired_exec);
	}

	#[tokio::test]
	async fn validate_token_unreachable_endpoint_is_non_fatal() {
		let n = ValidateTokenNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("access_token".into(), SocketValue::String("t".into()));
		inputs.insert("endpoint".into(), SocketValue::String("http://127.0.0.1:1".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"), "expected on_error, got: {:?}", out.fired_exec);
	}

	#[tokio::test]
	async fn user_id_unreachable_endpoint_is_non_fatal() {
		let n = UserIdByLoginNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = InputMap::new();
		inputs.insert("login".into(), SocketValue::String("someone".into()));
		inputs.insert("access_token".into(), SocketValue::String("t".into()));
		inputs.insert("client_id".into(), SocketValue::String("c".into()));
		inputs.insert("endpoint".into(), SocketValue::String("http://127.0.0.1:1".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"), "expected on_error, got: {:?}", out.fired_exec);
	}

	// --- ban / timeout ---

	fn ban_inputs_full() -> InputMap {
		let mut m = InputMap::new();
		m.insert("user_id".into(), SocketValue::String("111".into()));
		m.insert("broadcaster_id".into(), SocketValue::String("222".into()));
		m.insert("moderator_id".into(), SocketValue::String("333".into()));
		m.insert("access_token".into(), SocketValue::String("t".into()));
		m.insert("client_id".into(), SocketValue::String("c".into()));
		m.insert("endpoint".into(), SocketValue::String("http://127.0.0.1:1".into()));
		m
	}

	#[tokio::test]
	async fn ban_noop_without_fire() {
		let n = BanNode;
		let mut ctx = ExecCtx::default();
		let out = n
			.execute(&mut ctx, &InputMap::new(), &ban_inputs_full(), &ExecFireSet::new())
			.await
			.unwrap();
		assert!(out.fired_exec.is_empty());
	}

	#[tokio::test]
	async fn ban_empty_target_user_id_yields_error() {
		let n = BanNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = ban_inputs_full();
		inputs.insert("user_id".into(), SocketValue::String("   ".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out
			.data
			.get("error")
			.and_then(|v| v.as_str().ok().map(str::to_owned))
			.unwrap_or_default();
		assert!(err.contains("user_id"));
	}

	#[tokio::test]
	async fn ban_empty_broadcaster_or_moderator_yields_error() {
		let n = BanNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = ban_inputs_full();
		inputs.insert("moderator_id".into(), SocketValue::String("".into()));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[tokio::test]
	async fn ban_unreachable_endpoint_is_non_fatal() {
		let n = BanNode;
		let mut ctx = ExecCtx::default();
		let out = n
			.execute(&mut ctx, &InputMap::new(), &ban_inputs_full(), &fired("exec_in"))
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_error"), "got: {:?}", out.fired_exec);
	}

	#[tokio::test]
	async fn timeout_requires_duration_in_range() {
		let n = TimeoutNode;
		let mut ctx = ExecCtx::default();

		// 0 秒 → 範囲外
		let mut inputs = ban_inputs_full();
		inputs.insert("duration_secs".into(), SocketValue::Int(0));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out
			.data
			.get("error")
			.and_then(|v| v.as_str().ok().map(str::to_owned))
			.unwrap_or_default();
		assert!(err.contains("duration_secs"));

		// 2 週間超 → 範囲外
		let mut inputs = ban_inputs_full();
		inputs.insert("duration_secs".into(), SocketValue::Int(TIMEOUT_MAX_SECS + 1));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		assert!(out.fired_exec.contains("on_error"));
	}

	#[tokio::test]
	async fn timeout_valid_duration_hits_endpoint_and_is_non_fatal() {
		let n = TimeoutNode;
		let mut ctx = ExecCtx::default();
		let mut inputs = ban_inputs_full();
		inputs.insert("duration_secs".into(), SocketValue::Int(60));
		let out = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await.unwrap();
		// connection refused → on_error に流れる（HTTP モック不要）
		assert!(out.fired_exec.contains("on_error"), "got: {:?}", out.fired_exec);
	}

	#[tokio::test]
	async fn timeout_missing_duration_is_validation_error_from_required_input() {
		let n = TimeoutNode;
		let mut ctx = ExecCtx::default();
		// duration_secs を入れない → get_required_int が MissingRequiredInput を返す
		let inputs = ban_inputs_full();
		let res = n.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in")).await;
		assert!(matches!(res, Err(NodeExecError::MissingRequiredInput(ref k)) if k == "duration_secs"));
	}
}

// ---------------------------------------------------------------------
// δ-4d 統合テスト: ingress.twitch → rate_limit → (allow/deny) → chat_send
// ---------------------------------------------------------------------
//
// 実 HTTP は叩かず、`rate_limit` の gate 動作と graph の繋がりのみを検証する。
// chat_send は到達不能 endpoint で `on_error` を返すので、その error 分岐が来ることを
// rate_limit の on_allow 経路として観測する。

#[cfg(test)]
mod integration_tests {
	use super::*;
	use crate::flowgraph::engine::{create_trigger_bus, FlowgraphBuilder, PortRef};
	use crate::flowgraph::node::{NodeImpl, TriggerEvent};
	use crate::flowgraph::nodes::ingress::TwitchIngressNode;
	use crate::flowgraph::nodes::literal::{IntLiteralNode, StringLiteralNode};
	use crate::flowgraph::nodes::log::LogNode;
	use crate::flowgraph::nodes::rate_limit::RateLimitNode;
	use std::sync::Arc;
	use std::time::Duration;

	fn props_int(v: i64) -> InputMap {
		let mut m = InputMap::new();
		m.insert("value".into(), SocketValue::Int(v));
		m
	}

	fn props_str(s: &str) -> InputMap {
		let mut m = InputMap::new();
		m.insert("value".into(), SocketValue::String(s.into()));
		m
	}

	/// 3 連打の chat で rate_limit(max=2) が 3 発目を deny することを確認する。
	#[tokio::test]
	async fn twitch_ingress_gated_by_rate_limit_then_chat_send_branch() {
		let mut b = FlowgraphBuilder::new();

		b.add_node("in", NodeImpl::pure(Arc::new(TwitchIngressNode)), InputMap::new());
		b.add_node("rl", NodeImpl::stateful(Arc::new(RateLimitNode)), InputMap::new());
		b.add_node("max", NodeImpl::pure(Arc::new(IntLiteralNode)), props_int(2));
		b.add_node("win", NodeImpl::pure(Arc::new(IntLiteralNode)), props_int(60_000));
		b.add_node("log_ok", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.add_node("log_ng", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.add_node("lit_ok", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("ALLOW"));
		b.add_node("lit_ng", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("DENY"));

		b.connect(PortRef::new("max", "value"), PortRef::new("rl", "max_count"));
		b.connect(PortRef::new("win", "value"), PortRef::new("rl", "window_ms"));
		b.connect(PortRef::new("lit_ok", "value"), PortRef::new("log_ok", "value"));
		b.connect(PortRef::new("lit_ng", "value"), PortRef::new("log_ng", "value"));
		b.connect_exec(PortRef::new("in", "exec_out"), PortRef::new("rl", "exec_in"));
		b.connect_exec(PortRef::new("rl", "on_allow"), PortRef::new("log_ok", "exec_in"));
		b.connect_exec(PortRef::new("rl", "on_deny"), PortRef::new("log_ng", "exec_in"));

		let mut prog = b.build().expect("build");
		let (handle, rx) = create_trigger_bus();
		let external = handle.clone();
		let mut ctx = ExecCtx::default();

		let sender = tokio::spawn(async move {
			for i in 0..3 {
				tokio::time::sleep(Duration::from_millis(30)).await;
				let _ = external.send(
					TriggerEvent::new("in")
						.with_exec("__trigger__")
						.with_override("__content__", SocketValue::String(format!("chat {i}"))),
				);
			}
		});
		let shutdown = tokio::time::sleep(Duration::from_millis(300));
		prog.run_forever_with_bus(&mut ctx, handle, rx, shutdown, None)
			.await
			.expect("run_forever_with_bus");
		sender.await.unwrap();

		let allow_n = ctx.trace.iter().filter(|l| l.contains("ALLOW")).count();
		let deny_n = ctx.trace.iter().filter(|l| l.contains("DENY")).count();
		assert_eq!(allow_n, 2, "expected 2 ALLOW traces, got {allow_n}: {:?}", ctx.trace);
		assert_eq!(deny_n, 1, "expected 1 DENY trace, got {deny_n}: {:?}", ctx.trace);
	}

	/// chat_send の on_error 経路が繋がっていることの煙テスト。
	/// 到達不能 endpoint で on_error を強制。
	#[tokio::test]
	async fn twitch_ingress_into_chat_send_error_branch() {
		let mut b = FlowgraphBuilder::new();

		b.add_node("in", NodeImpl::pure(Arc::new(TwitchIngressNode)), InputMap::new());
		b.add_node("cs", NodeImpl::effectful(Arc::new(ChatSendNode)), InputMap::new());
		b.add_node("bid", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("1"));
		b.add_node("sid", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("2"));
		b.add_node("tok", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("t"));
		b.add_node("cid", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("c"));
		b.add_node("ep", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("http://127.0.0.1:1"));
		b.add_node("log_err", NodeImpl::effectful(Arc::new(LogNode)), InputMap::new());
		b.add_node("lit_err", NodeImpl::pure(Arc::new(StringLiteralNode)), props_str("GOT_ERROR"));

		b.connect(PortRef::new("bid", "value"), PortRef::new("cs", "broadcaster_id"));
		b.connect(PortRef::new("sid", "value"), PortRef::new("cs", "sender_user_id"));
		b.connect(PortRef::new("tok", "value"), PortRef::new("cs", "access_token"));
		b.connect(PortRef::new("cid", "value"), PortRef::new("cs", "client_id"));
		b.connect(PortRef::new("ep", "value"), PortRef::new("cs", "endpoint"));
		b.connect(PortRef::new("in", "content"), PortRef::new("cs", "text"));
		b.connect(PortRef::new("lit_err", "value"), PortRef::new("log_err", "value"));
		b.connect_exec(PortRef::new("in", "exec_out"), PortRef::new("cs", "exec_in"));
		b.connect_exec(PortRef::new("cs", "on_error"), PortRef::new("log_err", "exec_in"));

		let mut prog = b.build().expect("build");
		let (handle, rx) = create_trigger_bus();
		let external = handle.clone();
		let mut ctx = ExecCtx::default();

		let sender = tokio::spawn(async move {
			tokio::time::sleep(Duration::from_millis(30)).await;
			let _ = external.send(
				TriggerEvent::new("in")
					.with_exec("__trigger__")
					.with_override("__content__", SocketValue::String("hello".into())),
			);
		});
		let shutdown = tokio::time::sleep(Duration::from_millis(500));
		prog.run_forever_with_bus(&mut ctx, handle, rx, shutdown, None)
			.await
			.expect("run_forever_with_bus");
		sender.await.unwrap();

		assert!(
			ctx.trace.iter().any(|l| l.contains("GOT_ERROR")),
			"expected GOT_ERROR trace, got: {:?}",
			ctx.trace
		);
	}
}
