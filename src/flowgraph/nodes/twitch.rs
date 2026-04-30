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
	get_optional_bool, get_optional_int, get_optional_string, get_required_bool, get_required_int, get_required_json, get_required_string,
	EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};

const HELIX: &str = "https://api.twitch.tv/helix";
const VALIDATE_URL: &str = "https://id.twitch.tv/oauth2/validate";
const DEFAULT_MAX_CHARS: i64 = 500;
const DEFAULT_TIMEOUT_SECS: u64 = 30;
const STREAM_MARKER_DESCRIPTION_MAX_CHARS: i64 = 140;
const CHANNEL_TITLE_MAX_CHARS: i64 = 140;
const POLL_TITLE_MAX_CHARS: i64 = 60;
const POLL_CHOICE_MAX_CHARS: i64 = 25;
const PREDICTION_TITLE_MAX_CHARS: i64 = 45;
const PREDICTION_OUTCOME_MAX_CHARS: i64 = 25;

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

fn twitch_common_action_inputs() -> Vec<PortSpec> {
	vec![
		PortSpec::exec_input("exec_in", "Exec"),
		PortSpec::input("broadcaster_id", "Broadcaster ID", SocketType::String),
		PortSpec::input("access_token", "Access Token", SocketType::String),
		PortSpec::input("client_id", "Client ID", SocketType::String),
		PortSpec::input("endpoint", "Endpoint", SocketType::String).with_default(SocketValue::String(String::new())),
	]
}

fn twitch_action_outputs(extra: Vec<PortSpec>) -> Vec<PortSpec> {
	let mut outputs = vec![
		PortSpec::exec_output("on_success", "On Success"),
		PortSpec::exec_output("on_error", "On Error"),
		PortSpec::output("response", "Response", SocketType::Json),
		PortSpec::output("error", "Error", SocketType::String),
		PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Json))),
	];
	outputs.extend(extra);
	outputs
}

fn twitch_action_err(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("response", SocketValue::Json(JsonValue::Null))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code("twitch.request")))
		.fire_exec("on_error")
}

fn twitch_action_success(response: JsonValue) -> NodeOutput {
	NodeOutput::new()
		.set_data("response", SocketValue::Json(response.clone()))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Json(response))))
		.fire_exec("on_success")
}

fn helix_base(inputs: &InputMap) -> Result<String, NodeExecError> {
	let endpoint = get_optional_string(inputs, "endpoint", "")?;
	Ok(if endpoint.trim().is_empty() {
		HELIX.to_string()
	} else {
		endpoint.trim().trim_end_matches('/').to_string()
	})
}

fn require_non_empty(value: &str, name: &str) -> Result<(), NodeOutput> {
	if value.trim().is_empty() {
		Err(twitch_action_err(format!("{name} が空です")))
	} else {
		Ok(())
	}
}

fn required_auth(inputs: &InputMap) -> Result<(String, String), NodeExecError> {
	Ok((
		get_required_string(inputs, "access_token")?.trim().to_string(),
		get_required_string(inputs, "client_id")?.trim().to_string(),
	))
}

async fn helix_request_json(
	inputs: &InputMap,
	method: reqwest::Method,
	path_and_query: String,
	body: Option<JsonValue>,
) -> Result<JsonValue, String> {
	let (access_token, client_id) = required_auth(inputs).map_err(|e| e.to_string())?;
	if access_token.is_empty() || client_id.is_empty() {
		return Err("access_token / client_id が空です".into());
	}
	let base = helix_base(inputs).map_err(|e| e.to_string())?;
	let client = http_client()?;
	let url = format!("{base}{path_and_query}");
	let mut req = client
		.request(method, &url)
		.header("Client-Id", client_id)
		.bearer_auth(access_token);
	if let Some(body) = body {
		req = req.json(&body);
	}
	let resp = req.send().await.map_err(|e| format!("Helix request 送信失敗: {e}"))?;
	let status = resp.status();
	let body_text = resp.text().await.unwrap_or_default();
	if !status.is_success() {
		return Err(format!("Helix HTTP {status}: {body_text}"));
	}
	Ok(if body_text.trim().is_empty() {
		JsonValue::Null
	} else {
		serde_json::from_str(&body_text).unwrap_or(JsonValue::String(body_text))
	})
}

fn json_array_of_titles(v: &JsonValue, field_name: &str, min: usize, max: usize, max_chars: i64) -> Result<Vec<JsonValue>, NodeOutput> {
	let Some(arr) = v.as_array() else {
		return Err(twitch_action_err(format!("{field_name} must be a JSON array")));
	};
	if arr.len() < min || arr.len() > max {
		return Err(twitch_action_err(format!("{field_name} must contain {min}..={max} items")));
	}
	let mut out = Vec::with_capacity(arr.len());
	for item in arr {
		let title = if let Some(s) = item.as_str() {
			s
		} else {
			item.get("title").and_then(JsonValue::as_str).unwrap_or("")
		};
		let title = clip_chars(title.trim(), max_chars);
		if title.is_empty() {
			return Err(twitch_action_err(format!("{field_name} contains an empty title")));
		}
		out.push(json!({ "title": title }));
	}
	Ok(out)
}

fn insert_optional_bool(body: &mut serde_json::Map<String, JsonValue>, inputs: &InputMap, key: &str) -> Result<(), NodeExecError> {
	if let Some(v) = inputs.get(key) {
		let value = v.as_bool().map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{key}: {e}")))?;
		body.insert(key.to_string(), JsonValue::Bool(value));
	}
	Ok(())
}

fn insert_optional_int(body: &mut serde_json::Map<String, JsonValue>, inputs: &InputMap, key: &str) -> Result<(), NodeExecError> {
	if let Some(v) = inputs.get(key) {
		let value = v.as_i64().map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{key}: {e}")))?;
		body.insert(key.to_string(), JsonValue::Number(value.into()));
	}
	Ok(())
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
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Json))),
			],
			properties: vec![],
		}
	}
}

fn get_token_fail(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("access_token", SocketValue::String(String::new()))
		.set_data("client_id", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code("twitch.get_token")))
		.fire_exec("on_failure")
}

fn get_token_success(access_token: String, client_id: String) -> NodeOutput {
	let value = json!({
		"access_token": access_token,
		"client_id": client_id,
	});
	NodeOutput::new()
		.set_data(
			"access_token",
			SocketValue::String(value["access_token"].as_str().unwrap_or_default().to_string()),
		)
		.set_data(
			"client_id",
			SocketValue::String(value["client_id"].as_str().unwrap_or_default().to_string()),
		)
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Json(value))))
		.fire_exec("on_success")
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
				Ok(get_token_success(token, client_id_for_output))
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
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Json))),
			],
			properties: vec![],
		}
	}
}

fn validate_err(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("user_id", SocketValue::String(String::new()))
		.set_data("login", SocketValue::String(String::new()))
		.set_data("client_id", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data(
			"result",
			SocketValue::Result(FlowResult::err(msg).with_code("twitch.validate_token")),
		)
		.fire_exec("on_error")
}

fn validate_success(user_id: String, login: String, client_id: String) -> NodeOutput {
	let value = json!({
		"user_id": user_id,
		"login": login,
		"client_id": client_id,
	});
	NodeOutput::new()
		.set_data(
			"user_id",
			SocketValue::String(value["user_id"].as_str().unwrap_or_default().to_string()),
		)
		.set_data(
			"login",
			SocketValue::String(value["login"].as_str().unwrap_or_default().to_string()),
		)
		.set_data(
			"client_id",
			SocketValue::String(value["client_id"].as_str().unwrap_or_default().to_string()),
		)
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Json(value))))
		.fire_exec("on_success")
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
		Ok(validate_success(user_id, login, client_id))
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
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::String))),
			],
			properties: vec![],
		}
	}
}

fn user_id_err(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("user_id", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data(
			"result",
			SocketValue::Result(FlowResult::err(msg).with_code("twitch.user_id_by_login")),
		)
		.fire_exec("on_error")
}

fn user_id_success(user_id: String) -> NodeOutput {
	NodeOutput::new()
		.set_data("user_id", SocketValue::String(user_id.clone()))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::String(user_id))))
		.fire_exec("on_success")
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
		Ok(user_id_success(uid))
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
	PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::String))),
   ],
   properties: vec![],
  }
	}
}

fn chat_send_err(msg: impl Into<String>) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("sent_text", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code("twitch.chat_send")))
		.fire_exec("on_error")
}

fn chat_send_skipped() -> NodeOutput {
	NodeOutput::new()
		.set_data("sent_text", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::String(String::new()))))
		.fire_exec("on_skipped")
}

fn chat_send_success(sent_text: String) -> NodeOutput {
	NodeOutput::new()
		.set_data("sent_text", SocketValue::String(sent_text.clone()))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::String(sent_text))))
		.fire_exec("on_success")
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
		Ok(chat_send_success(text))
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
		PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::String))),
	]
}

fn moderation_err(msg: impl Into<String>, code: &'static str) -> NodeOutput {
	let msg = msg.into();
	NodeOutput::new()
		.set_data("end_time", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(msg.clone()))
		.set_data("result", SocketValue::Result(FlowResult::err(msg).with_code(code)))
		.fire_exec("on_error")
}

fn moderation_success(end_time: String) -> NodeOutput {
	NodeOutput::new()
		.set_data("end_time", SocketValue::String(end_time.clone()))
		.set_data("error", SocketValue::String(String::new()))
		.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::String(end_time))))
		.fire_exec("on_success")
}

/// `duration_secs` が `Some(d)` で d>0 なら timeout、None/0 以下なら永久 ban。
async fn execute_moderation_ban(
	inputs: &InputMap,
	duration_secs: Option<i64>,
	result_code: &'static str,
) -> Result<NodeOutput, NodeExecError> {
	let user_id = get_required_string(inputs, "user_id")?;
	let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
	let moderator_id = get_required_string(inputs, "moderator_id")?;
	let access_token = get_required_string(inputs, "access_token")?;
	let client_id = get_required_string(inputs, "client_id")?;
	let reason_raw = get_optional_string(inputs, "reason", "")?;
	let reason = clip_chars(reason_raw.trim(), MODERATION_REASON_MAX_CHARS);

	if user_id.trim().is_empty() {
		return Ok(moderation_err("user_id が空です", result_code));
	}
	if broadcaster_id.trim().is_empty() || moderator_id.trim().is_empty() {
		return Ok(moderation_err("broadcaster_id / moderator_id が空です", result_code));
	}
	if access_token.trim().is_empty() || client_id.trim().is_empty() {
		return Ok(moderation_err("access_token / client_id が空です", result_code));
	}

	let mut body_data = serde_json::Map::new();
	body_data.insert("user_id".into(), JsonValue::String(user_id.trim().to_string()));
	if !reason.is_empty() {
		body_data.insert("reason".into(), JsonValue::String(reason));
	}
	if let Some(d) = duration_secs {
		if !(1..=TIMEOUT_MAX_SECS).contains(&d) {
			return Ok(moderation_err(
				format!("duration_secs は 1..={TIMEOUT_MAX_SECS} の範囲でなければなりません（指定値: {d}）"),
				result_code,
			));
		}
		body_data.insert("duration".into(), JsonValue::Number(d.into()));
	}
	let body = json!({ "data": JsonValue::Object(body_data) });

	let endpoint = get_optional_string(inputs, "endpoint", "")?;
	let base = if endpoint.trim().is_empty() { HELIX } else { endpoint.trim() };
	let client = match http_client() {
		Ok(c) => c,
		Err(e) => return Ok(moderation_err(e, result_code)),
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
		Err(e) => return Ok(moderation_err(format!("POST /moderation/bans 送信失敗: {e}"), result_code)),
	};
	let status = resp.status();
	let body_text = resp.text().await.unwrap_or_default();
	if !status.is_success() {
		return Ok(moderation_err(
			format!("POST /moderation/bans HTTP {status}: {body_text}"),
			result_code,
		));
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
	Ok(moderation_success(end_time))
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
		execute_moderation_ban(inputs, None, "twitch.ban").await
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
		execute_moderation_ban(inputs, Some(duration), "twitch.timeout").await
	}
}

// ---------------------------------------------------------------------
// Phase sigma: Twitch Helix action nodes
// ---------------------------------------------------------------------

pub struct AdRunNode;
pub struct RaidStartNode;
pub struct RaidCancelNode;
pub struct StreamMarkerCreateNode;
pub struct ClipCreateNode;
pub struct ChannelInfoUpdateNode;
pub struct ChatSettingsUpdateNode;
pub struct ChatClearNode;
pub struct ShieldModeUpdateNode;
pub struct PollCreateNode;
pub struct PollEndNode;
pub struct PredictionCreateNode;
pub struct PredictionEndNode;
pub struct GoalsGetNode;

impl NodeDescriptor for AdRunNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("length_seconds", "Length Seconds", SocketType::Int).with_default(SocketValue::Int(60)));
		NodeSpec {
			feature: "flowgraph.twitch.ad_run".into(),
			title: "Twitch: Run Ad".into(),
			category: "twitch".into(),
			description: Some("Helix POST /channels/commercial で広告を実行する。length_seconds は通常 30/60/90/120/150/180。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for AdRunNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let length = get_optional_int(inputs, "length_seconds", 60)?.clamp(30, 180);
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		let body = json!({ "broadcaster_id": broadcaster_id.trim(), "length": length });
		match helix_request_json(inputs, reqwest::Method::POST, "/channels/commercial".into(), Some(body)).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for RaidStartNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("to_broadcaster_id", "To Broadcaster ID", SocketType::String));
		NodeSpec {
			feature: "flowgraph.twitch.raid_start".into(),
			title: "Twitch: Start Raid".into(),
			category: "twitch".into(),
			description: Some("Helix POST /raids で raid を開始する。実際のraidはTwitch側の90秒カウントダウン後に行われる。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for RaidStartNode {
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
		let from_id = get_required_string(inputs, "broadcaster_id")?;
		let to_id = get_required_string(inputs, "to_broadcaster_id")?;
		if let Err(out) = require_non_empty(&from_id, "broadcaster_id") {
			return Ok(out);
		}
		if let Err(out) = require_non_empty(&to_id, "to_broadcaster_id") {
			return Ok(out);
		}
		let path = format!(
			"/raids?from_broadcaster_id={}&to_broadcaster_id={}",
			urlencoding::encode(from_id.trim()),
			urlencoding::encode(to_id.trim())
		);
		match helix_request_json(inputs, reqwest::Method::POST, path, None).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for RaidCancelNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.twitch.raid_cancel".into(),
			title: "Twitch: Cancel Raid".into(),
			category: "twitch".into(),
			description: Some("Helix DELETE /raids で保留中の raid をキャンセルする。".into()),
			inputs: twitch_common_action_inputs(),
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for RaidCancelNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		let path = format!("/raids?broadcaster_id={}", urlencoding::encode(broadcaster_id.trim()));
		match helix_request_json(inputs, reqwest::Method::DELETE, path, None).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for StreamMarkerCreateNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("description", "Description", SocketType::String).with_default(SocketValue::String(String::new())));
		NodeSpec {
			feature: "flowgraph.twitch.stream_marker_create".into(),
			title: "Twitch: Create Stream Marker".into(),
			category: "twitch".into(),
			description: Some("Helix POST /streams/markers で配信マーカーを追加する。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![PortSpec::output("marker_id", "Marker ID", SocketType::String)]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for StreamMarkerCreateNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let description = clip_chars(
			get_optional_string(inputs, "description", "")?.trim(),
			STREAM_MARKER_DESCRIPTION_MAX_CHARS,
		);
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out.set_data("marker_id", SocketValue::String(String::new())));
		}
		let mut body = json!({ "user_id": broadcaster_id.trim() });
		if !description.is_empty() {
			body["description"] = JsonValue::String(description);
		}
		match helix_request_json(inputs, reqwest::Method::POST, "/streams/markers".into(), Some(body)).await {
			Ok(v) => {
				let marker_id = v
					.get("data")
					.and_then(JsonValue::as_array)
					.and_then(|a| a.first())
					.and_then(|row| row.get("id"))
					.and_then(JsonValue::as_str)
					.unwrap_or("")
					.to_string();
				Ok(twitch_action_success(v).set_data("marker_id", SocketValue::String(marker_id)))
			}
			Err(e) => Ok(twitch_action_err(e).set_data("marker_id", SocketValue::String(String::new()))),
		}
	}
}

impl NodeDescriptor for ClipCreateNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("has_delay", "Has Delay", SocketType::Bool).with_default(SocketValue::Bool(false)));
		NodeSpec {
			feature: "flowgraph.twitch.clip_create".into(),
			title: "Twitch: Create Clip".into(),
			category: "twitch".into(),
			description: Some("Helix POST /clips で現在の配信からclip作成を開始する。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![
				PortSpec::output("clip_id", "Clip ID", SocketType::String),
				PortSpec::output("edit_url", "Edit URL", SocketType::String),
			]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ClipCreateNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let has_delay = get_optional_bool(inputs, "has_delay", false)?;
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out
				.set_data("clip_id", SocketValue::String(String::new()))
				.set_data("edit_url", SocketValue::String(String::new())));
		}
		let path = format!(
			"/clips?broadcaster_id={}&has_delay={}",
			urlencoding::encode(broadcaster_id.trim()),
			has_delay
		);
		match helix_request_json(inputs, reqwest::Method::POST, path, None).await {
			Ok(v) => {
				let first = v.get("data").and_then(JsonValue::as_array).and_then(|a| a.first());
				let clip_id = first
					.and_then(|row| row.get("id"))
					.and_then(JsonValue::as_str)
					.unwrap_or("")
					.to_string();
				let edit_url = first
					.and_then(|row| row.get("edit_url"))
					.and_then(JsonValue::as_str)
					.unwrap_or("")
					.to_string();
				Ok(twitch_action_success(v)
					.set_data("clip_id", SocketValue::String(clip_id))
					.set_data("edit_url", SocketValue::String(edit_url)))
			}
			Err(e) => Ok(twitch_action_err(e)
				.set_data("clip_id", SocketValue::String(String::new()))
				.set_data("edit_url", SocketValue::String(String::new()))),
		}
	}
}

impl NodeDescriptor for ChannelInfoUpdateNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("game_id", "Game ID", SocketType::String).with_default(SocketValue::String(String::new())));
		inputs.push(PortSpec::input("title", "Title", SocketType::String).with_default(SocketValue::String(String::new())));
		inputs.push(
			PortSpec::input("broadcaster_language", "Broadcaster Language", SocketType::String)
				.with_default(SocketValue::String(String::new())),
		);
		inputs.push(
			PortSpec::input("tags", "Tags", SocketType::List(Box::new(SocketType::String))).with_default(SocketValue::List(Vec::new())),
		);
		NodeSpec {
			feature: "flowgraph.twitch.channel_info_update".into(),
			title: "Twitch: Update Channel Info".into(),
			category: "twitch".into(),
			description: Some("Helix PATCH /channels で配信タイトル、カテゴリ、言語、タグを更新する。空入力は送信しない。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ChannelInfoUpdateNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		let mut body = serde_json::Map::new();
		let game_id = get_optional_string(inputs, "game_id", "")?;
		if !game_id.trim().is_empty() {
			body.insert("game_id".into(), JsonValue::String(game_id.trim().to_string()));
		}
		let title = clip_chars(get_optional_string(inputs, "title", "")?.trim(), CHANNEL_TITLE_MAX_CHARS);
		if !title.is_empty() {
			body.insert("title".into(), JsonValue::String(title));
		}
		let language = get_optional_string(inputs, "broadcaster_language", "")?;
		if !language.trim().is_empty() {
			body.insert("broadcaster_language".into(), JsonValue::String(language.trim().to_string()));
		}
		if let Some(tags) = inputs.get("tags").and_then(|v| v.as_list().ok()) {
			let tags: Vec<JsonValue> = tags
				.iter()
				.filter_map(|v| v.as_str().ok())
				.map(str::trim)
				.filter(|s| !s.is_empty())
				.take(10)
				.map(|s| JsonValue::String(s.to_string()))
				.collect();
			if !tags.is_empty() {
				body.insert("tags".into(), JsonValue::Array(tags));
			}
		}
		if body.is_empty() {
			return Ok(twitch_action_err("更新する channel info がありません"));
		}
		let path = format!("/channels?broadcaster_id={}", urlencoding::encode(broadcaster_id.trim()));
		match helix_request_json(inputs, reqwest::Method::PATCH, path, Some(JsonValue::Object(body))).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for ChatSettingsUpdateNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("moderator_id", "Moderator User ID", SocketType::String));
		inputs.push(PortSpec::input("emote_mode", "Emote Mode", SocketType::Bool));
		inputs.push(PortSpec::input("subscriber_mode", "Subscriber Mode", SocketType::Bool));
		inputs.push(PortSpec::input("unique_chat_mode", "Unique Chat Mode", SocketType::Bool));
		inputs.push(PortSpec::input("follower_mode", "Follower Mode", SocketType::Bool));
		inputs.push(PortSpec::input("follower_mode_duration", "Follower Mode Duration", SocketType::Int));
		inputs.push(PortSpec::input("slow_mode", "Slow Mode", SocketType::Bool));
		inputs.push(PortSpec::input("slow_mode_wait_time", "Slow Mode Wait Time", SocketType::Int));
		inputs.push(PortSpec::input("non_moderator_chat_delay", "Non Moderator Delay", SocketType::Bool));
		inputs.push(PortSpec::input(
			"non_moderator_chat_delay_duration",
			"Non Moderator Delay Duration",
			SocketType::Int,
		));
		NodeSpec {
			feature: "flowgraph.twitch.chat_settings_update".into(),
			title: "Twitch: Update Chat Settings".into(),
			category: "twitch".into(),
			description: Some("Helix PATCH /chat/settings で emote/subscriber/follower/slow/unique chat などの設定を更新する。接続された入力だけ送信する。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ChatSettingsUpdateNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let moderator_id = get_required_string(inputs, "moderator_id")?;
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		if let Err(out) = require_non_empty(&moderator_id, "moderator_id") {
			return Ok(out);
		}
		let mut body = serde_json::Map::new();
		insert_optional_bool(&mut body, inputs, "emote_mode")?;
		insert_optional_bool(&mut body, inputs, "subscriber_mode")?;
		insert_optional_bool(&mut body, inputs, "unique_chat_mode")?;
		insert_optional_bool(&mut body, inputs, "follower_mode")?;
		insert_optional_int(&mut body, inputs, "follower_mode_duration")?;
		insert_optional_bool(&mut body, inputs, "slow_mode")?;
		insert_optional_int(&mut body, inputs, "slow_mode_wait_time")?;
		insert_optional_bool(&mut body, inputs, "non_moderator_chat_delay")?;
		insert_optional_int(&mut body, inputs, "non_moderator_chat_delay_duration")?;
		if body.is_empty() {
			return Ok(twitch_action_err("更新する chat settings がありません"));
		}
		let path = format!(
			"/chat/settings?broadcaster_id={}&moderator_id={}",
			urlencoding::encode(broadcaster_id.trim()),
			urlencoding::encode(moderator_id.trim())
		);
		match helix_request_json(inputs, reqwest::Method::PATCH, path, Some(JsonValue::Object(body))).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for ChatClearNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("moderator_id", "Moderator User ID", SocketType::String));
		inputs.push(PortSpec::input("message_id", "Message ID", SocketType::String).with_default(SocketValue::String(String::new())));
		NodeSpec {
			feature: "flowgraph.twitch.chat_clear".into(),
			title: "Twitch: Clear Chat".into(),
			category: "twitch".into(),
			description: Some("Helix DELETE /moderation/chat でチャット全体または指定messageを削除する。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ChatClearNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let moderator_id = get_required_string(inputs, "moderator_id")?;
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		if let Err(out) = require_non_empty(&moderator_id, "moderator_id") {
			return Ok(out);
		}
		let mut path = format!(
			"/moderation/chat?broadcaster_id={}&moderator_id={}",
			urlencoding::encode(broadcaster_id.trim()),
			urlencoding::encode(moderator_id.trim())
		);
		let message_id = get_optional_string(inputs, "message_id", "")?;
		if !message_id.trim().is_empty() {
			path.push_str("&message_id=");
			path.push_str(&urlencoding::encode(message_id.trim()));
		}
		match helix_request_json(inputs, reqwest::Method::DELETE, path, None).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for ShieldModeUpdateNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("moderator_id", "Moderator User ID", SocketType::String));
		inputs.push(PortSpec::input("is_active", "Is Active", SocketType::Bool));
		NodeSpec {
			feature: "flowgraph.twitch.shield_mode_update".into(),
			title: "Twitch: Update Shield Mode".into(),
			category: "twitch".into(),
			description: Some("Helix PUT /moderation/shield_mode で Shield Mode を有効化/無効化する。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ShieldModeUpdateNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let moderator_id = get_required_string(inputs, "moderator_id")?;
		let is_active = get_required_bool(inputs, "is_active")?;
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		if let Err(out) = require_non_empty(&moderator_id, "moderator_id") {
			return Ok(out);
		}
		let path = format!(
			"/moderation/shield_mode?broadcaster_id={}&moderator_id={}",
			urlencoding::encode(broadcaster_id.trim()),
			urlencoding::encode(moderator_id.trim())
		);
		match helix_request_json(inputs, reqwest::Method::PUT, path, Some(json!({ "is_active": is_active }))).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for PollCreateNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("title", "Title", SocketType::String));
		inputs.push(PortSpec::input("choices", "Choices", SocketType::Json));
		inputs.push(PortSpec::input("duration_seconds", "Duration Seconds", SocketType::Int).with_default(SocketValue::Int(60)));
		inputs.push(
			PortSpec::input("channel_points_voting_enabled", "Channel Points Voting", SocketType::Bool)
				.with_default(SocketValue::Bool(false)),
		);
		inputs
			.push(PortSpec::input("channel_points_per_vote", "Channel Points Per Vote", SocketType::Int).with_default(SocketValue::Int(0)));
		NodeSpec {
			feature: "flowgraph.twitch.poll_create".into(),
			title: "Twitch: Create Poll".into(),
			category: "twitch".into(),
			description: Some("Helix POST /polls でpollを作成する。choices は文字列配列または {title} 配列。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![PortSpec::output("poll_id", "Poll ID", SocketType::String)]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for PollCreateNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let title = clip_chars(get_required_string(inputs, "title")?.trim(), POLL_TITLE_MAX_CHARS);
		let choices = match json_array_of_titles(get_required_json(inputs, "choices")?, "choices", 2, 5, POLL_CHOICE_MAX_CHARS) {
			Ok(v) => v,
			Err(out) => return Ok(out.set_data("poll_id", SocketValue::String(String::new()))),
		};
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out.set_data("poll_id", SocketValue::String(String::new())));
		}
		if title.is_empty() {
			return Ok(twitch_action_err("title が空です").set_data("poll_id", SocketValue::String(String::new())));
		}
		let mut body = json!({
			"broadcaster_id": broadcaster_id.trim(),
			"title": title,
			"choices": choices,
			"duration": get_optional_int(inputs, "duration_seconds", 60)?.clamp(15, 1800),
		});
		let points_enabled = get_optional_bool(inputs, "channel_points_voting_enabled", false)?;
		body["channel_points_voting_enabled"] = JsonValue::Bool(points_enabled);
		if points_enabled {
			body["channel_points_per_vote"] = JsonValue::Number(get_optional_int(inputs, "channel_points_per_vote", 0)?.max(0).into());
		}
		match helix_request_json(inputs, reqwest::Method::POST, "/polls".into(), Some(body)).await {
			Ok(v) => {
				let poll_id = v
					.get("data")
					.and_then(JsonValue::as_array)
					.and_then(|a| a.first())
					.and_then(|row| row.get("id"))
					.and_then(JsonValue::as_str)
					.unwrap_or("")
					.to_string();
				Ok(twitch_action_success(v).set_data("poll_id", SocketValue::String(poll_id)))
			}
			Err(e) => Ok(twitch_action_err(e).set_data("poll_id", SocketValue::String(String::new()))),
		}
	}
}

impl NodeDescriptor for PollEndNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("poll_id", "Poll ID", SocketType::String));
		inputs.push(PortSpec::input("status", "Status", SocketType::String).with_default(SocketValue::String("TERMINATED".into())));
		NodeSpec {
			feature: "flowgraph.twitch.poll_end".into(),
			title: "Twitch: End Poll".into(),
			category: "twitch".into(),
			description: Some("Helix PATCH /polls でpollを終了する。status は TERMINATED または ARCHIVED。".into()),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for PollEndNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let poll_id = get_required_string(inputs, "poll_id")?;
		let status = get_optional_string(inputs, "status", "TERMINATED")?.trim().to_ascii_uppercase();
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		if let Err(out) = require_non_empty(&poll_id, "poll_id") {
			return Ok(out);
		}
		if status != "TERMINATED" && status != "ARCHIVED" {
			return Ok(twitch_action_err("status must be TERMINATED or ARCHIVED"));
		}
		let body = json!({ "broadcaster_id": broadcaster_id.trim(), "id": poll_id.trim(), "status": status });
		match helix_request_json(inputs, reqwest::Method::PATCH, "/polls".into(), Some(body)).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for PredictionCreateNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("title", "Title", SocketType::String));
		inputs.push(PortSpec::input("outcomes", "Outcomes", SocketType::Json));
		inputs.push(
			PortSpec::input("prediction_window_seconds", "Prediction Window Seconds", SocketType::Int).with_default(SocketValue::Int(120)),
		);
		NodeSpec {
			feature: "flowgraph.twitch.prediction_create".into(),
			title: "Twitch: Create Prediction".into(),
			category: "twitch".into(),
			description: Some(
				"Helix POST /predictions でChannel Points predictionを作成する。outcomes は文字列配列または {title} 配列。".into(),
			),
			inputs,
			outputs: twitch_action_outputs(vec![PortSpec::output("prediction_id", "Prediction ID", SocketType::String)]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for PredictionCreateNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let title = clip_chars(get_required_string(inputs, "title")?.trim(), PREDICTION_TITLE_MAX_CHARS);
		let outcomes = match json_array_of_titles(
			get_required_json(inputs, "outcomes")?,
			"outcomes",
			2,
			10,
			PREDICTION_OUTCOME_MAX_CHARS,
		) {
			Ok(v) => v,
			Err(out) => return Ok(out.set_data("prediction_id", SocketValue::String(String::new()))),
		};
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out.set_data("prediction_id", SocketValue::String(String::new())));
		}
		if title.is_empty() {
			return Ok(twitch_action_err("title が空です").set_data("prediction_id", SocketValue::String(String::new())));
		}
		let body = json!({
			"broadcaster_id": broadcaster_id.trim(),
			"title": title,
			"outcomes": outcomes,
			"prediction_window": get_optional_int(inputs, "prediction_window_seconds", 120)?.clamp(30, 1800),
		});
		match helix_request_json(inputs, reqwest::Method::POST, "/predictions".into(), Some(body)).await {
			Ok(v) => {
				let prediction_id = v
					.get("data")
					.and_then(JsonValue::as_array)
					.and_then(|a| a.first())
					.and_then(|row| row.get("id"))
					.and_then(JsonValue::as_str)
					.unwrap_or("")
					.to_string();
				Ok(twitch_action_success(v).set_data("prediction_id", SocketValue::String(prediction_id)))
			}
			Err(e) => Ok(twitch_action_err(e).set_data("prediction_id", SocketValue::String(String::new()))),
		}
	}
}

impl NodeDescriptor for PredictionEndNode {
	fn describe(&self) -> NodeSpec {
		let mut inputs = twitch_common_action_inputs();
		inputs.push(PortSpec::input("prediction_id", "Prediction ID", SocketType::String));
		inputs.push(PortSpec::input("status", "Status", SocketType::String).with_default(SocketValue::String("CANCELED".into())));
		inputs.push(
			PortSpec::input("winning_outcome_id", "Winning Outcome ID", SocketType::String)
				.with_default(SocketValue::String(String::new())),
		);
		NodeSpec {
			feature: "flowgraph.twitch.prediction_end".into(),
			title: "Twitch: End Prediction".into(),
			category: "twitch".into(),
			description: Some(
				"Helix PATCH /predictions でpredictionを LOCKED / RESOLVED / CANCELED にする。RESOLVED は winning_outcome_id 必須。".into(),
			),
			inputs,
			outputs: twitch_action_outputs(vec![]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for PredictionEndNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		let prediction_id = get_required_string(inputs, "prediction_id")?;
		let status = get_optional_string(inputs, "status", "CANCELED")?.trim().to_ascii_uppercase();
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out);
		}
		if let Err(out) = require_non_empty(&prediction_id, "prediction_id") {
			return Ok(out);
		}
		if status != "LOCKED" && status != "RESOLVED" && status != "CANCELED" {
			return Ok(twitch_action_err("status must be LOCKED, RESOLVED, or CANCELED"));
		}
		let mut body = json!({ "broadcaster_id": broadcaster_id.trim(), "id": prediction_id.trim(), "status": status });
		let winning = get_optional_string(inputs, "winning_outcome_id", "")?;
		if !winning.trim().is_empty() {
			body["winning_outcome_id"] = JsonValue::String(winning.trim().to_string());
		} else if status == "RESOLVED" {
			return Ok(twitch_action_err("RESOLVED requires winning_outcome_id"));
		}
		match helix_request_json(inputs, reqwest::Method::PATCH, "/predictions".into(), Some(body)).await {
			Ok(v) => Ok(twitch_action_success(v)),
			Err(e) => Ok(twitch_action_err(e)),
		}
	}
}

impl NodeDescriptor for GoalsGetNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.twitch.goals_get".into(),
			title: "Twitch: Get Goals".into(),
			category: "twitch".into(),
			description: Some("Helix GET /goals でチャンネルのcreator goals一覧を取得する。".into()),
			inputs: twitch_common_action_inputs(),
			outputs: twitch_action_outputs(vec![PortSpec::output("goals", "Goals", SocketType::Json)]),
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for GoalsGetNode {
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
		let broadcaster_id = get_required_string(inputs, "broadcaster_id")?;
		if let Err(out) = require_non_empty(&broadcaster_id, "broadcaster_id") {
			return Ok(out.set_data("goals", SocketValue::Json(JsonValue::Array(Vec::new()))));
		}
		let path = format!("/goals?broadcaster_id={}", urlencoding::encode(broadcaster_id.trim()));
		match helix_request_json(inputs, reqwest::Method::GET, path, None).await {
			Ok(v) => {
				let goals = v.get("data").cloned().unwrap_or(JsonValue::Array(Vec::new()));
				Ok(twitch_action_success(v).set_data("goals", SocketValue::Json(goals)))
			}
			Err(e) => Ok(twitch_action_err(e).set_data("goals", SocketValue::Json(JsonValue::Array(Vec::new())))),
		}
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

	fn auth_inputs() -> InputMap {
		[
			("broadcaster_id".to_string(), SocketValue::String("111".into())),
			("access_token".to_string(), SocketValue::String("token".into())),
			("client_id".to_string(), SocketValue::String("client".into())),
			("endpoint".to_string(), SocketValue::String("http://127.0.0.1:1".into())),
		]
		.into_iter()
		.collect()
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

	#[test]
	fn json_array_of_titles_accepts_strings_and_objects() {
		let got = json_array_of_titles(&json!(["a", { "title": "b" }]), "choices", 2, 5, 25).unwrap();
		assert_eq!(got, vec![json!({"title": "a"}), json!({"title": "b"})]);
	}

	#[test]
	fn json_array_of_titles_rejects_bad_count() {
		assert!(json_array_of_titles(&json!(["only one"]), "choices", 2, 5, 25).is_err());
	}

	#[test]
	fn twitch_action_common_outputs_include_result() {
		let outputs = twitch_action_outputs(vec![]);
		let result = outputs.iter().find(|p| p.name == "result").expect("result output");
		assert_eq!(result.ty, SocketType::Result(Box::new(SocketType::Json)));
	}

	#[test]
	fn twitch_action_result_matches_success_and_error() {
		let success = twitch_action_success(json!({ "ok": true }));
		match success.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert_eq!(result.value, Some(Box::new(SocketValue::Json(json!({ "ok": true })))));
				assert_eq!(result.error, None);
			}
			other => panic!("expected result, got {other:?}"),
		}

		let failure = twitch_action_err("boom");
		match failure.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.error.as_deref(), Some("boom"));
				assert_eq!(result.code.as_deref(), Some("twitch.request"));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}

	#[test]
	fn twitch_individual_outputs_include_result() {
		let get_token = get_token_fail("boom");
		match get_token.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("twitch.get_token"));
			}
			other => panic!("expected result, got {other:?}"),
		}

		let validate = validate_success("u".into(), "login".into(), "cid".into());
		match validate.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert!(matches!(result.value.as_deref(), Some(SocketValue::Json(value)) if value["user_id"] == "u"));
			}
			other => panic!("expected result, got {other:?}"),
		}

		let chat = chat_send_success("hello".into());
		match chat.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert_eq!(result.value.as_deref(), Some(&SocketValue::String("hello".into())));
			}
			other => panic!("expected result, got {other:?}"),
		}

		let moderation = moderation_err("nope", "twitch.timeout");
		match moderation.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("twitch.timeout"));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}

	#[tokio::test]
	async fn new_twitch_action_nodes_noop_without_fire() {
		let mut ctx = ExecCtx::default();
		let nodes: Vec<Box<dyn EffectfulNode>> = vec![
			Box::new(AdRunNode),
			Box::new(RaidStartNode),
			Box::new(RaidCancelNode),
			Box::new(StreamMarkerCreateNode),
			Box::new(ClipCreateNode),
			Box::new(ChannelInfoUpdateNode),
			Box::new(ChatSettingsUpdateNode),
			Box::new(ChatClearNode),
			Box::new(ShieldModeUpdateNode),
			Box::new(PollCreateNode),
			Box::new(PollEndNode),
			Box::new(PredictionCreateNode),
			Box::new(PredictionEndNode),
			Box::new(GoalsGetNode),
		];
		for node in nodes {
			let out = node
				.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
				.await
				.unwrap();
			assert!(out.fired_exec.is_empty());
		}
	}

	#[tokio::test]
	async fn channel_info_update_empty_patch_is_error_before_network() {
		let mut ctx = ExecCtx::default();
		let out = ChannelInfoUpdateNode
			.execute(&mut ctx, &InputMap::new(), &auth_inputs(), &fired("exec_in"))
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out.data.get("error").and_then(|v| v.as_str().ok()).unwrap_or("");
		assert!(err.contains("channel info"));
	}

	#[tokio::test]
	async fn chat_settings_update_empty_patch_is_error_before_network() {
		let mut ctx = ExecCtx::default();
		let mut inputs = auth_inputs();
		inputs.insert("moderator_id".into(), SocketValue::String("222".into()));
		let out = ChatSettingsUpdateNode
			.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in"))
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out.data.get("error").and_then(|v| v.as_str().ok()).unwrap_or("");
		assert!(err.contains("chat settings"));
	}

	#[tokio::test]
	async fn prediction_resolved_requires_winning_outcome_id() {
		let mut ctx = ExecCtx::default();
		let mut inputs = auth_inputs();
		inputs.insert("prediction_id".into(), SocketValue::String("pred".into()));
		inputs.insert("status".into(), SocketValue::String("RESOLVED".into()));
		let out = PredictionEndNode
			.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in"))
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_error"));
		let err = out.data.get("error").and_then(|v| v.as_str().ok()).unwrap_or("");
		assert!(err.contains("winning_outcome_id"));
	}

	#[tokio::test]
	async fn poll_create_rejects_too_few_choices() {
		let mut ctx = ExecCtx::default();
		let mut inputs = auth_inputs();
		inputs.insert("title".into(), SocketValue::String("pick one".into()));
		inputs.insert("choices".into(), SocketValue::Json(json!(["only one"])));
		let out = PollCreateNode
			.execute(&mut ctx, &InputMap::new(), &inputs, &fired("exec_in"))
			.await
			.unwrap();
		assert!(out.fired_exec.contains("on_error"));
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
