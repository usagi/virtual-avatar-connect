//! Chat Completions 縺ｮ `tools` JSON 隱ｭ縺ｿ霎ｼ縺ｿ縺ｨ縲∫ｵ・∩霎ｼ縺ｿ繝・・繝ｫ縺ｮ螳溯｡後・
//!
//! Phase IV 縺ｧ **Actions**・亥憶菴懃畑縺ｮ縺ゅｋ function-call・峨ｒ蟆主・縺励◆縲ゅヤ繝ｼ繝ｫ譛ｬ菴薙・ `run_function_tool` 縺ｫ繝・ぅ繧ｹ繝代ャ繝√＆繧後・
//! [`ToolContext`] 邨檎罰縺ｧ繧｢繝励Μ迥ｶ諷具ｼ・SharedState`・峨ｄ Twitch 隱崎ｨｼ諠・ｱ縺ｫ繧｢繧ｯ繧ｻ繧ｹ縺吶ｋ縲・
//!
//! - `vac_ping`: 逍朱夂｢ｺ隱搾ｼ亥憶菴懃畑縺ｪ縺暦ｼ・
//! - `vac_emit_effect`: 莉ｻ諢上・ VAC 繝√Ε繝ｳ繝阪Ν縺ｸ `ChannelDatum` 繧・push 縺吶ｋ縲・rowser Source 蛛ｴ縺ｧ諡ｾ縺｣縺ｦ貍泌・縺ｫ菴ｿ縺・Φ螳壹・
//! - `vac_twitch_set_category`: Helix `PATCH /channels` 縺ｧ驟堺ｿ｡繧ｫ繝・ざ繝ｪ繝ｼ繧貞､画峩縺吶ｋ縲りｦ・`channel:manage:broadcast` 繧ｹ繧ｳ繝ｼ繝励・
//! - `vac_twitch_chat_say`: Helix `POST /chat/messages` 縺ｧ繝√Ε繝・ヨ逋ｺ隧ｱ・医Δ繝・Ξ繝ｼ繧ｿ繝ｼ/繝懊ャ繝亥哨・峨・
//! - `vac_twitch_ban_user`: Helix `POST /moderation/bans` 縺ｧ BAN / timeout・・duration_secs` 謖・ｮ壹〒 timeout・峨・
//! - `vac_twitch_unban_user`: Helix `DELETE /moderation/bans`縲・
//! - `vac_twitch_delete_message`: Helix `DELETE /moderation/chat` 縺ｧ迚ｹ螳壹Γ繝・そ繝ｼ繧ｸ蜑企勁縲・

use anyhow::{bail, Context, Result};
use async_openai::types::chat::{
 ChatCompletionMessageToolCalls, ChatCompletionRequestAssistantMessage, ChatCompletionRequestAssistantMessageContent,
 ChatCompletionResponseMessage, ChatCompletionToolChoiceOption, ChatCompletionTools, ToolChoiceOptions,
};
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::Arc;

use crate::ai::openai_responses::types::{
 input::InputItem,
 request::{NamedToolChoice, Tool, ToolChoice, ToolChoiceMode},
 response::FunctionCallView,
};
use crate::conf::{TwitchEventSubConfig, TwitchModeratorConfig};
use crate::state::{ChannelDatum, DataSource};
use crate::SharedState;

/// Phase IV: 蜑ｯ菴懃畑繧剃ｼｴ縺・ヤ繝ｼ繝ｫ縺ｫ貂｡縺吝ｮ溯｡後さ繝ｳ繝・く繧ｹ繝医・
#[derive(Clone)]
pub(crate) struct ToolContext {
 pub(crate) state: SharedState,
 pub(crate) persona_label: String,
 /// `vac_emit_effect` 縺ｧ `channel` 蠑墓焚縺檎┌縺・→縺阪↓菴ｿ縺・里螳壹・ VAC 繝√Ε繝ｳ繝阪Ν蜷阪・
 pub(crate) effect_channel: String,
 /// Helix 蜻ｼ縺ｳ蜃ｺ縺礼ｳｻ繝・・繝ｫ逕ｨ縲よ悴險ｭ螳壹・繝壹Ν繧ｽ繝翫〒縺ｯ Twitch 邉ｻ繝・・繝ｫ繧堤┌蜉ｹ蛹悶☆繧九・
 pub(crate) twitch_eventsub: Option<Arc<TwitchEventSubConfig>>,
 /// Phase V-c: 繝｢繝・Ξ繝ｼ繧ｷ繝ｧ繝ｳ邉ｻ・・AN / timeout / delete / chat・峨ヤ繝ｼ繝ｫ縺悟ｿ・ｦ√→縺吶ｋ繝懊ャ繝医い繧ｫ繧ｦ繝ｳ繝郁ｨｭ螳壹・
 pub(crate) twitch_moderator: Option<Arc<TwitchModeratorConfig>>,
 /// 繝・ヵ繧ｩ繝ｫ繝医・騾∽ｿ｡蜈磯・菫｡閠・login縲Ｄhat_say / ban 邉ｻ縺ｧ `broadcaster_login` 蠑墓焚縺檎怐逡･縺輔ｌ縺溘→縺阪・繝輔か繝ｼ繝ｫ繝舌ャ繧ｯ縲・
 pub(crate) twitch_default_broadcaster_login: Option<String>,
}

/// `openai_tools_json` の JSON をパースして Responses API の [`Tool`] 配列に変換する。
///
/// # 受け付ける書式（両対応）
///
/// 1. **Chat Completions 形式**（旧来、後方互換）
///    ```json
///    [{"type":"function","function":{"name":"vac_ping","description":"...","parameters":{...},"strict":true}}]
///    ```
/// 2. **Responses 形式**（推奨、χ 以降）
///    ```json
///    [{"type":"function","name":"vac_ping","description":"...","parameters":{...},"strict":true}]
///    ```
/// 3. **Hosted tools**（Responses 新機能）
///    ```json
///    [{"type":"web_search"}, {"type":"file_search","vector_store_ids":["vs_..."]}]
///    ```
///
/// 1 件ごとに自動判定（`type:"function"` で `function` sub-object を持つなら 1 番と見做す）
/// するため、ユーザが既存 conf を書き換えずに χ 移行できる。
pub(crate) fn parse_tools_json(text: &str) -> Result<Vec<Tool>> {
 let t = text.trim();
 if t.is_empty() {
  return Ok(Vec::new());
 }
 let raw: Vec<Value> =
  serde_json::from_str(t).context("openai_tools_json の JSON を配列としてパースできませんでした")?;
 let mut out = Vec::with_capacity(raw.len());
 for (i, v) in raw.into_iter().enumerate() {
  out.push(tool_from_json_any_format(v).with_context(|| format!("openai_tools_json[{i}] の解釈に失敗"))?);
 }
 Ok(out)
}

/// 1 件分の JSON を受けて、Chat Completions / Responses どちらの形式でも Responses `Tool` に正規化する。
fn tool_from_json_any_format(v: Value) -> Result<Tool> {
 // Chat Completions 形式判定: `type:"function"` かつ `function` sub-object 持ち
 if let Value::Object(map) = &v {
  if map.get("type").and_then(|t| t.as_str()) == Some("function")
   && map.get("function").map(|f| f.is_object()).unwrap_or(false)
  {
   let function = map.get("function").and_then(|f| f.as_object()).unwrap();
   let name = function
    .get("name")
    .and_then(|n| n.as_str())
    .context("function.name が未指定")?
    .to_string();
   let description = function.get("description").and_then(|d| d.as_str()).map(str::to_string);
   let parameters = function.get("parameters").cloned().unwrap_or_else(|| json!({"type":"object"}));
   let strict = function.get("strict").and_then(|s| s.as_bool());
   return Ok(Tool::Function {
    name,
    description,
    parameters,
    strict,
   });
  }
  if map.get("type").and_then(|t| t.as_str()) == Some("custom")
   && map.get("custom").map(|f| f.is_object()).unwrap_or(false)
  {
   let custom = map.get("custom").and_then(|f| f.as_object()).unwrap();
   let name = custom
    .get("name")
    .and_then(|n| n.as_str())
    .context("custom.name が未指定")?
    .to_string();
   let description = custom.get("description").and_then(|d| d.as_str()).map(str::to_string);
   let format = custom.get("format").cloned();
   return Ok(Tool::Custom {
    name,
    description,
    format,
   });
  }
 }
 serde_json::from_value::<Tool>(v).context("Tool として解釈できない JSON")
}

/// 宣言された tool 名の集合を返す。hosted tools は固定名（`"web_search"` 等）。
pub(crate) fn declared_tool_names(tools: &[Tool]) -> HashSet<String> {
 tools.iter().map(|t| t.name().to_string()).collect()
}

/// VAC が **ローカル実行する** tool 名（`Function` / `Custom`）の集合。
/// hosted tools（web_search / file_search / code_interpreter）は除外。
pub(crate) fn locally_dispatched_tool_names(tools: &[Tool]) -> HashSet<String> {
 tools
  .iter()
  .filter(|t| t.is_locally_dispatched())
  .map(|t| t.name().to_string())
  .collect()
}

/// `openai_tool_choice` 文字列を [`ToolChoice`] に変換する。
pub(crate) fn parse_tool_choice(s: &str) -> Result<ToolChoice> {
 match s.trim().to_lowercase().as_str() {
  "none" => Ok(ToolChoice::Mode(ToolChoiceMode::None)),
  "auto" => Ok(ToolChoice::Mode(ToolChoiceMode::Auto)),
  "required" => Ok(ToolChoice::Mode(ToolChoiceMode::Required)),
  other => {
   // `function:<name>` / `custom:<name>` の named 指定に対応。
   if let Some(rest) = other.strip_prefix("function:") {
    let name = rest.trim();
    if name.is_empty() {
     bail!("openai_tool_choice の function: 後に名前がありません");
    }
    return Ok(ToolChoice::Named(NamedToolChoice::Function { name: name.to_string() }));
   }
   if let Some(rest) = other.strip_prefix("custom:") {
    let name = rest.trim();
    if name.is_empty() {
     bail!("openai_tool_choice の custom: 後に名前がありません");
    }
    return Ok(ToolChoice::Named(NamedToolChoice::Custom { name: name.to_string() }));
   }
   bail!(
    "openai_tool_choice は none / auto / required / function:<name> / custom:<name> のいずれかにしてください: {:?}",
    s
   )
  },
 }
}

// ---------- χ-5 までの Chat Completions 互換 bridge（移行期間限定） ----------

/// Responses API の [`Tool`] 配列を Chat Completions の `ChatCompletionTools` 配列に変換する。
///
/// hosted tools（web_search / file_search / code_interpreter）は Chat Completions では
/// 対応していないので **サイレントに除外** し、呼び出し側で warn ログを出せるよう個数を返す。
///
/// 戻り値: `(変換済 tools, 除外した hosted tools の名前)`
pub(crate) fn tools_to_chat_completions(tools: &[Tool]) -> Result<(Vec<ChatCompletionTools>, Vec<String>)> {
 let mut cc_json = Vec::new();
 let mut skipped = Vec::new();
 for t in tools {
  match t {
   Tool::Function {
    name,
    description,
    parameters,
    strict,
   } => {
    let mut function = json!({"name": name, "parameters": parameters});
    if let Some(d) = description {
     function["description"] = Value::String(d.clone());
    }
    if let Some(s) = strict {
     function["strict"] = Value::Bool(*s);
    }
    cc_json.push(json!({"type": "function", "function": function}));
   },
   Tool::Custom {
    name,
    description,
    format,
   } => {
    let mut custom = json!({"name": name});
    if let Some(d) = description {
     custom["description"] = Value::String(d.clone());
    }
    if let Some(f) = format {
     custom["format"] = f.clone();
    }
    cc_json.push(json!({"type": "custom", "custom": custom}));
   },
   Tool::WebSearch { .. } | Tool::FileSearch { .. } | Tool::CodeInterpreter { .. } => {
    skipped.push(t.name().to_string());
   },
  }
 }
 let converted: Vec<ChatCompletionTools> = serde_json::from_value(Value::Array(cc_json))
  .context("Responses Tool を ChatCompletionTools に変換する際に失敗")?;
 Ok((converted, skipped))
}

/// [`ToolChoice`] を Chat Completions の `ChatCompletionToolChoiceOption` に変換する。
pub(crate) fn tool_choice_to_chat_completions(choice: &ToolChoice) -> ChatCompletionToolChoiceOption {
 match choice {
  ToolChoice::Mode(ToolChoiceMode::Auto) => ChatCompletionToolChoiceOption::Mode(ToolChoiceOptions::Auto),
  ToolChoice::Mode(ToolChoiceMode::None) => ChatCompletionToolChoiceOption::Mode(ToolChoiceOptions::None),
  ToolChoice::Mode(ToolChoiceMode::Required) => ChatCompletionToolChoiceOption::Mode(ToolChoiceOptions::Required),
  ToolChoice::Named(NamedToolChoice::Function { name }) => {
   let v = json!({"type": "function", "function": {"name": name}});
   serde_json::from_value(v).unwrap_or(ChatCompletionToolChoiceOption::Mode(ToolChoiceOptions::Auto))
  },
  ToolChoice::Named(NamedToolChoice::Custom { name }) => {
   let v = json!({"type": "custom", "custom": {"name": name}});
   serde_json::from_value(v).unwrap_or(ChatCompletionToolChoiceOption::Mode(ToolChoiceOptions::Auto))
  },
 }
}

pub(crate) fn response_message_to_assistant_request(m: &ChatCompletionResponseMessage) -> ChatCompletionRequestAssistantMessage {
 ChatCompletionRequestAssistantMessage {
  content: m.content.as_ref().map(|s| ChatCompletionRequestAssistantMessageContent::Text(s.clone())),
  refusal: m.refusal.clone(),
  name: None,
  audio: None,
  tool_calls: m.tool_calls.clone(),
  ..Default::default()
 }
}

pub(crate) async fn run_function_tool(name: &str, arguments: &str, ctx: &ToolContext) -> String {
 match name {
  "vac_ping" => {
   let _ = arguments;
   r#"{"ok":true,"tool":"vac_ping"}"#.to_string()
  },
  "vac_emit_effect" => action_emit_effect(arguments, ctx).await,
  "vac_twitch_set_category" => action_twitch_set_category(arguments, ctx).await,
  "vac_twitch_chat_say" => action_twitch_chat_say(arguments, ctx).await,
  "vac_twitch_ban_user" => action_twitch_ban_user(arguments, ctx).await,
  "vac_twitch_unban_user" => action_twitch_unban_user(arguments, ctx).await,
  "vac_twitch_delete_message" => action_twitch_delete_message(arguments, ctx).await,
  _ => json_tool_error("unknown tool", name),
 }
}

fn json_tool_error(key: &str, name: &str) -> String {
 let esc = name.replace('\\', "\\\\").replace('"', "\\\"");
 format!(r#"{{"error":"{}","name":"{}"}}"#, key, esc)
}

/// tool 縺ｮ謌ｻ繧雁､繧・`serde_json::Value` 縺九ｉ譁・ｭ怜・蛹悶☆繧九・繝ｫ繝代・縲・
fn json_ok(value: Value) -> String {
 serde_json::to_string(&value).unwrap_or_else(|_| r#"{"ok":true}"#.to_string())
}

fn json_err(message: impl Into<String>) -> String {
 let m = message.into();
 serde_json::to_string(&json!({"ok": false, "error": m})).unwrap_or_else(|_| r#"{"ok":false,"error":"serialization_failed"}"#.to_string())
}

/// Responses API ネイティブな tool call ディスパッチ。
///
/// 入力は `OutputItem::FunctionCall` から取った [`FunctionCallView`]（非所有参照）。
/// 出力は次ラウンドの `input[]` に積むための [`InputItem::FunctionCallOutput`]。
///
/// - `declared` に入っていない名前は即座に `{"error":"tool not declared...","name":"..."}` を返す
/// - 宣言済みなら [`run_function_tool`] に委譲
pub(crate) async fn dispatch_tool_call(
 call: FunctionCallView<'_>,
 declared: &HashSet<String>,
 ctx: &ToolContext,
) -> InputItem {
 let output = if !declared.contains(call.name) {
  json_tool_error("tool not declared in openai_tools_json", call.name)
 }
 else {
  run_function_tool(call.name, call.arguments, ctx).await
 };
 InputItem::function_call_output(call.call_id, output)
}

/// χ-5 までの Chat Completions 互換 dispatch（移行期間限定）。
///
/// `completion.rs::create_chat_completion_resolve_tools` が Chat Completions の
/// `ChatCompletionRequestToolMessage` を作るのに必要な `(id, output)` タプルを返す。
/// χ-5 で service.rs / completion.rs が `create_stream` / `dispatch_tool_call` に
/// 全面移行した時点で削除する。
pub(crate) async fn dispatch_cc_tool_call(
 tc: &ChatCompletionMessageToolCalls,
 declared: &HashSet<String>,
 ctx: &ToolContext,
) -> Option<(String, String)> {
 match tc {
  ChatCompletionMessageToolCalls::Function(f) => {
   if !declared.contains(&f.function.name) {
    return Some((
     f.id.clone(),
     json_tool_error("tool not declared in openai_tools_json", &f.function.name),
    ));
   }
   let out = run_function_tool(&f.function.name, &f.function.arguments, ctx).await;
   Some((f.id.clone(), out))
  },
  ChatCompletionMessageToolCalls::Custom(c) => {
   if !declared.contains(&c.custom_tool.name) {
    return Some((c.id.clone(), json_tool_error("tool not declared in openai_tools_json", &c.custom_tool.name)));
   }
   Some((
    c.id.clone(),
    r#"{"error":"custom tools are not executed by VAC yet"}"#.to_string(),
   ))
  },
 }
}

// ---------- Phase IV: Action impls ----------

/// `vac_emit_effect`: 莉ｻ諢上・ VAC 繝√Ε繝ｳ繝阪Ν縺ｸ `ChannelDatum` 繧・push 縺吶ｋ縲・
///
/// 蠑墓焚 JSON:
/// ```json
/// {"channel":"effect","content":"sparkle","meta":{"intensity":3}}
/// ```
/// - `channel`: 逵∫払譎ゅ・ `ToolContext::effect_channel`
/// - `content`: 蠢・医よｼ泌・隴伜挨蟄舌ｄ繝・く繧ｹ繝医ゅヶ繝ｩ繧ｦ繧ｶ繧ｽ繝ｼ繧ｹ蛛ｴ縺ｧ隗｣驥医☆繧九・
/// - `meta`: 莉ｻ諢上ＡChannelDatum::meta` 縺ｫ key=value 縺ｨ縺励※繧ｳ繝斐・縺吶ｋ・・Value::Object` 縺ｮ縺ｿ・峨・
async fn action_emit_effect(arguments: &str, ctx: &ToolContext) -> String {
 let args: Value = match serde_json::from_str(arguments) {
  Ok(v) => v,
  Err(_) if arguments.trim().is_empty() => Value::Object(Default::default()),
  Err(e) => return json_err(format!("invalid arguments JSON: {}", e)),
 };

 let content = match args.get("content").and_then(|v| v.as_str()) {
  Some(s) if !s.trim().is_empty() => s.to_string(),
  _ => return json_err("content is required (non-empty string)"),
 };
 let channel = args
  .get("channel")
  .and_then(|v| v.as_str())
  .map(|s| s.trim().to_string())
  .filter(|s| !s.is_empty())
  .unwrap_or_else(|| ctx.effect_channel.clone());

 let mut cd = ChannelDatum::new(channel.clone(), content.clone())
  .with_flag(ChannelDatum::FLAG_IS_FINAL)
  .with_source(DataSource::new("ai").with_subtype("action.emit_effect").with_actor(&ctx.persona_label));

 if let Some(meta) = args.get("meta").and_then(|v| v.as_object()) {
  for (k, v) in meta {
   let vs = match v {
    Value::String(s) => s.clone(),
    other => other.to_string(),
   };
   cd = cd.with_meta(k, vs);
  }
 }

 let id = cd.get_id();
 log::info!(
  "縲晦I[{}]縲・action vac_emit_effect 竊・channel={} content={:?} (datum_id={})",
  ctx.persona_label,
  channel,
  content,
  id
 );
 ctx.state.read().await.push_channel_datum(cd).await;
 json_ok(json!({"ok": true, "tool": "vac_emit_effect", "channel": channel, "datum_id": id}))
}

/// `vac_twitch_set_category`: Helix `PATCH /channels` 縺ｧ驟堺ｿ｡繧ｫ繝・ざ繝ｪ繝ｼ繧貞､画峩縺吶ｋ縲・
///
/// 蠑墓焚 JSON:
/// ```json
/// {"game_name":"Just Chatting"}
/// ```
/// 縺ｾ縺溘・ `{"game_id":"509658"}`縲ゆｸ｡譁ｹ謖・ｮ壽凾縺ｯ `game_id` 繧貞━蜈医☆繧九・
///
/// 隕√せ繧ｳ繝ｼ繝・ `channel:manage:broadcast`・・twitch_oauth` 縺ｮ譌｢螳壹せ繧ｳ繝ｼ繝励↓蜷ｫ繧√※縺ゅｋ・峨・
async fn action_twitch_set_category(arguments: &str, ctx: &ToolContext) -> String {
 let Some(es) = ctx.twitch_eventsub.clone() else {
  return json_err("Twitch eventsub config is not available for this persona");
 };

 let args: Value = match serde_json::from_str(arguments) {
  Ok(v) => v,
  Err(e) => return json_err(format!("invalid arguments JSON: {}", e)),
 };

 let game_id_arg = args
  .get("game_id")
  .and_then(|v| v.as_str())
  .map(|s| s.trim().to_string())
  .filter(|s| !s.is_empty());
 let game_name_arg = args
  .get("game_name")
  .and_then(|v| v.as_str())
  .map(|s| s.trim().to_string())
  .filter(|s| !s.is_empty());
 if game_id_arg.is_none() && game_name_arg.is_none() {
  return json_err("either game_id or game_name is required");
 }

 let access_token = match crate::twitch::oauth::ensure_user_access_token(&es).await {
  Ok(t) => t,
  Err(e) => return json_err(format!("failed to get Twitch user access token: {}", e)),
 };
 let client_id = crate::twitch::oauth::resolve_twitch_client_id(&es);
 let broadcaster_login = es
  .broadcaster_login
  .clone()
  .filter(|s| !s.trim().is_empty())
  .unwrap_or_default();
 if broadcaster_login.is_empty() {
  return json_err("twitch.eventsub.broadcaster_login is not configured");
 }

 let broadcaster_id = match helix_user_id(&client_id, &access_token, &broadcaster_login).await {
  Ok(id) => id,
  Err(e) => return json_err(format!("failed to resolve broadcaster_id: {}", e)),
 };

 let game_id = match (game_id_arg.clone(), game_name_arg.clone()) {
  (Some(id), _) => id,
  (None, Some(name)) => match helix_resolve_game_id(&client_id, &access_token, &name).await {
   Ok(Some(id)) => id,
   Ok(None) => return json_err(format!("game not found: {}", name)),
   Err(e) => return json_err(format!("failed to resolve game_id: {}", e)),
  },
  (None, None) => unreachable!(),
 };

 match helix_patch_channel_game_id(&client_id, &access_token, &broadcaster_id, &game_id).await {
  Ok(_) => {
   log::info!(
    "縲晦I[{}]縲・action vac_twitch_set_category 竊・broadcaster_id={} game_id={} (requested game_name={:?})",
    ctx.persona_label,
    broadcaster_id,
    game_id,
    game_name_arg
   );
   json_ok(json!({
    "ok": true,
    "tool": "vac_twitch_set_category",
    "broadcaster_id": broadcaster_id,
    "game_id": game_id,
    "game_name": game_name_arg,
   }))
  },
  Err(e) => json_err(format!("helix patch channels failed: {}", e)),
 }
}

// ---------- Phase V-c: Moderation actions ----------

/// 繝｢繝・Ξ繝ｼ繧ｿ繝ｼ邉ｻ繝・・繝ｫ蜈ｱ騾壹・蜑榊・逅・Ａ(eventsub, moderator, client_id, moderator_access_token, sender_user_id, broadcaster_login, broadcaster_id)` 繧定ｿ斐☆縲・
async fn resolve_twitch_moderation(
 args: &Value,
 ctx: &ToolContext,
) -> std::result::Result<(String, String, String, String, String), String> {
 let Some(es) = ctx.twitch_eventsub.clone() else {
  return Err("Twitch eventsub config is not available for this persona".into());
 };
 let Some(mc) = ctx.twitch_moderator.clone() else {
  return Err("Twitch moderator (bot) account config is not available for this persona".into());
 };
 if !mc.enabled {
  return Err("Twitch moderator account is disabled (twitch.moderator.enabled = false)".into());
 }

 let client_id = crate::twitch::oauth::resolve_twitch_client_id(&es);
 let access_token = match crate::twitch::oauth::ensure_moderator_access_token(&es, &mc).await {
  Ok(t) => t,
  Err(e) => return Err(format!("failed to get moderator access token: {}", e)),
 };
 let sender_user_id = match helix_validate_user_id(&access_token).await {
  Ok(id) => id,
  Err(e) => return Err(format!("failed to resolve moderator user_id via validate: {}", e)),
 };

 let broadcaster_login = args
  .get("broadcaster_login")
  .and_then(|v| v.as_str())
  .map(|s| s.trim().trim_start_matches('#').to_lowercase())
  .filter(|s| !s.is_empty())
  .or_else(|| {
   ctx.twitch_default_broadcaster_login
    .as_deref()
    .map(|s| s.trim().trim_start_matches('#').to_lowercase())
    .filter(|s| !s.is_empty())
  })
  .ok_or_else(|| "broadcaster_login is required (or set [twitch.eventsub] / [twitch].username as fallback)".to_string())?;

 let broadcaster_id = match helix_user_id(&client_id, &access_token, &broadcaster_login).await {
  Ok(id) => id,
  Err(e) => return Err(format!("failed to resolve broadcaster_id: {}", e)),
 };

 Ok((client_id, access_token, sender_user_id, broadcaster_login, broadcaster_id))
}

/// 蠑墓焚 `{user_id? | user_login?}` 縺九ｉ Twitch user_id 繧定ｧ｣豎ｺ縺吶ｋ縲・
async fn resolve_target_user_id(
 args: &Value,
 client_id: &str,
 bearer: &str,
) -> std::result::Result<String, String> {
 if let Some(id) = args.get("user_id").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()) {
  return Ok(id.to_string());
 }
 if let Some(login) = args.get("user_login").and_then(|v| v.as_str()).map(|s| s.trim().trim_start_matches('@').to_lowercase()).filter(|s| !s.is_empty()) {
  return helix_user_id(client_id, bearer, &login)
   .await
   .map_err(|e| format!("failed to resolve user_id from user_login={}: {}", login, e));
 }
 Err("either user_id or user_login is required".into())
}

/// `vac_twitch_chat_say`: Helix `POST /chat/messages` 縺ｧ繝懊ャ繝亥哨逋ｺ隧ｱ縲・
async fn action_twitch_chat_say(arguments: &str, ctx: &ToolContext) -> String {
 let args: Value = match serde_json::from_str(arguments) {
  Ok(v) => v,
  Err(_) if arguments.trim().is_empty() => Value::Object(Default::default()),
  Err(e) => return json_err(format!("invalid arguments JSON: {}", e)),
 };
 let content = match args.get("content").and_then(|v| v.as_str()) {
  Some(s) if !s.trim().is_empty() => s.trim().to_string(),
  _ => return json_err("content is required (non-empty string)"),
 };
 let reply_parent_message_id = args
  .get("reply_parent_message_id")
  .and_then(|v| v.as_str())
  .map(|s| s.trim().to_string())
  .filter(|s| !s.is_empty());

 let (client_id, bearer, sender_id, _login, broadcaster_id) = match resolve_twitch_moderation(&args, ctx).await {
  Ok(v) => v,
  Err(e) => return json_err(e),
 };

 let url = "https://api.twitch.tv/helix/chat/messages";
 let mut body = json!({
  "broadcaster_id": broadcaster_id,
  "sender_id": sender_id,
  "message": content,
 });
 if let Some(rp) = &reply_parent_message_id {
  body["reply_parent_message_id"] = Value::String(rp.clone());
 }
 let resp = match reqwest::Client::new()
  .post(url)
  .header("Client-Id", &client_id)
  .bearer_auth(&bearer)
  .json(&body)
  .send()
  .await
 {
  Ok(r) => r,
  Err(e) => return json_err(format!("request failed: {}", e)),
 };
 let status = resp.status();
 let text = resp.text().await.unwrap_or_default();
 if !status.is_success() {
  return json_err(format!("POST /chat/messages failed: {} {}", status, text));
 }
 log::info!(
  "縲晦I[{}]縲・action vac_twitch_chat_say 竊・broadcaster_id={} sender_id={} len={}",
  ctx.persona_label,
  broadcaster_id,
  sender_id,
  content.chars().count()
 );
 json_ok(json!({"ok": true, "tool": "vac_twitch_chat_say", "broadcaster_id": broadcaster_id, "sender_id": sender_id}))
}

/// `vac_twitch_ban_user`: BAN 縺ｾ縺溘・ timeout・・duration_secs` 謖・ｮ壽凾・峨・
async fn action_twitch_ban_user(arguments: &str, ctx: &ToolContext) -> String {
 let args: Value = match serde_json::from_str(arguments) {
  Ok(v) => v,
  Err(e) => return json_err(format!("invalid arguments JSON: {}", e)),
 };
 let reason = args
  .get("reason")
  .and_then(|v| v.as_str())
  .map(|s| s.trim().to_string())
  .filter(|s| !s.is_empty());
 let duration_secs = args.get("duration_secs").and_then(|v| v.as_u64());

 let (client_id, bearer, moderator_id, _login, broadcaster_id) = match resolve_twitch_moderation(&args, ctx).await {
  Ok(v) => v,
  Err(e) => return json_err(e),
 };
 let target_id = match resolve_target_user_id(&args, &client_id, &bearer).await {
  Ok(id) => id,
  Err(e) => return json_err(e),
 };

 let url = format!(
  "https://api.twitch.tv/helix/moderation/bans?broadcaster_id={}&moderator_id={}",
  urlencoding::encode(&broadcaster_id),
  urlencoding::encode(&moderator_id)
 );
 let mut inner = serde_json::Map::new();
 inner.insert("user_id".to_string(), Value::String(target_id.clone()));
 if let Some(d) = duration_secs {
  inner.insert("duration".to_string(), Value::from(d));
 }
 if let Some(r) = reason.clone() {
  inner.insert("reason".to_string(), Value::String(r));
 }
 let body = json!({"data": Value::Object(inner)});

 let resp = match reqwest::Client::new()
  .post(&url)
  .header("Client-Id", &client_id)
  .bearer_auth(&bearer)
  .json(&body)
  .send()
  .await
 {
  Ok(r) => r,
  Err(e) => return json_err(format!("request failed: {}", e)),
 };
 let status = resp.status();
 let text = resp.text().await.unwrap_or_default();
 if !status.is_success() {
  return json_err(format!("POST /moderation/bans failed: {} {}", status, text));
 }
 log::info!(
  "縲晦I[{}]縲・action vac_twitch_ban_user 竊・broadcaster_id={} target={} duration_secs={:?} reason={:?}",
  ctx.persona_label,
  broadcaster_id,
  target_id,
  duration_secs,
  reason
 );
 json_ok(json!({
  "ok": true,
  "tool": "vac_twitch_ban_user",
  "broadcaster_id": broadcaster_id,
  "user_id": target_id,
  "duration_secs": duration_secs,
 }))
}

/// `vac_twitch_unban_user`: BAN 隗｣髯､縲Ｕimeout 荳ｭ縺ｮ隗｣髯､縺ｫ繧ゆｽｿ縺医ｋ縲・
async fn action_twitch_unban_user(arguments: &str, ctx: &ToolContext) -> String {
 let args: Value = match serde_json::from_str(arguments) {
  Ok(v) => v,
  Err(e) => return json_err(format!("invalid arguments JSON: {}", e)),
 };
 let (client_id, bearer, moderator_id, _login, broadcaster_id) = match resolve_twitch_moderation(&args, ctx).await {
  Ok(v) => v,
  Err(e) => return json_err(e),
 };
 let target_id = match resolve_target_user_id(&args, &client_id, &bearer).await {
  Ok(id) => id,
  Err(e) => return json_err(e),
 };

 let url = format!(
  "https://api.twitch.tv/helix/moderation/bans?broadcaster_id={}&moderator_id={}&user_id={}",
  urlencoding::encode(&broadcaster_id),
  urlencoding::encode(&moderator_id),
  urlencoding::encode(&target_id)
 );
 let resp = match reqwest::Client::new()
  .delete(&url)
  .header("Client-Id", &client_id)
  .bearer_auth(&bearer)
  .send()
  .await
 {
  Ok(r) => r,
  Err(e) => return json_err(format!("request failed: {}", e)),
 };
 let status = resp.status();
 if !status.is_success() {
  let text = resp.text().await.unwrap_or_default();
  return json_err(format!("DELETE /moderation/bans failed: {} {}", status, text));
 }
 log::info!(
  "縲晦I[{}]縲・action vac_twitch_unban_user 竊・broadcaster_id={} target={}",
  ctx.persona_label,
  broadcaster_id,
  target_id,
 );
 json_ok(json!({"ok": true, "tool": "vac_twitch_unban_user", "broadcaster_id": broadcaster_id, "user_id": target_id}))
}

/// `vac_twitch_delete_message`: 迚ｹ螳壹Γ繝・そ繝ｼ繧ｸ縺ｮ蜑企勁縲Ａmessage_id` 蠢・医・
async fn action_twitch_delete_message(arguments: &str, ctx: &ToolContext) -> String {
 let args: Value = match serde_json::from_str(arguments) {
  Ok(v) => v,
  Err(e) => return json_err(format!("invalid arguments JSON: {}", e)),
 };
 let message_id = match args.get("message_id").and_then(|v| v.as_str()).map(|s| s.trim()).filter(|s| !s.is_empty()) {
  Some(m) => m.to_string(),
  None => return json_err("message_id is required"),
 };
 let (client_id, bearer, moderator_id, _login, broadcaster_id) = match resolve_twitch_moderation(&args, ctx).await {
  Ok(v) => v,
  Err(e) => return json_err(e),
 };

 let url = format!(
  "https://api.twitch.tv/helix/moderation/chat?broadcaster_id={}&moderator_id={}&message_id={}",
  urlencoding::encode(&broadcaster_id),
  urlencoding::encode(&moderator_id),
  urlencoding::encode(&message_id)
 );
 let resp = match reqwest::Client::new()
  .delete(&url)
  .header("Client-Id", &client_id)
  .bearer_auth(&bearer)
  .send()
  .await
 {
  Ok(r) => r,
  Err(e) => return json_err(format!("request failed: {}", e)),
 };
 let status = resp.status();
 if !status.is_success() {
  let text = resp.text().await.unwrap_or_default();
  return json_err(format!("DELETE /moderation/chat failed: {} {}", status, text));
 }
 log::info!(
  "縲晦I[{}]縲・action vac_twitch_delete_message 竊・broadcaster_id={} message_id={}",
  ctx.persona_label,
  broadcaster_id,
  message_id
 );
 json_ok(json!({"ok": true, "tool": "vac_twitch_delete_message", "broadcaster_id": broadcaster_id, "message_id": message_id}))
}

/// 繝医・繧ｯ繝ｳ謇譛芽・・ user_id 繧貞叙蠕励☆繧具ｼ・alidate 繝ｬ繧ｹ繝昴Φ繧ｹ縺ｫ蜷ｫ縺ｾ繧後ｋ・峨・
async fn helix_validate_user_id(bearer: &str) -> Result<String> {
 let resp = reqwest::Client::new()
  .get("https://id.twitch.tv/oauth2/validate")
  .header("Authorization", format!("Bearer {}", bearer.trim()))
  .send()
  .await?;
 let status = resp.status();
 let text = resp.text().await.unwrap_or_default();
 if !status.is_success() {
  bail!("validate failed: {} {}", status, text);
 }
 let v: Value = serde_json::from_str(&text).context("validate response JSON parse failed")?;
 v.get("user_id")
  .and_then(|u| u.as_str())
  .map(|s| s.to_string())
  .ok_or_else(|| anyhow::anyhow!("validate: user_id not found"))
}

async fn helix_user_id(client_id: &str, bearer: &str, login: &str) -> Result<String> {
 let url = format!("https://api.twitch.tv/helix/users?login={}", urlencoding::encode(login));
 let resp = reqwest::Client::new()
  .get(&url)
  .header("Client-Id", client_id)
  .bearer_auth(bearer)
  .send()
  .await?
  .error_for_status()?;
 let v: Value = resp.json().await?;
 v.get("data")
  .and_then(|d| d.as_array())
  .and_then(|a| a.first())
  .and_then(|u| u.get("id"))
  .and_then(|i| i.as_str())
  .map(|s| s.to_string())
  .context("helix users: id not found")
}

async fn helix_resolve_game_id(client_id: &str, bearer: &str, name: &str) -> Result<Option<String>> {
 let url = format!("https://api.twitch.tv/helix/games?name={}", urlencoding::encode(name));
 let resp = reqwest::Client::new()
  .get(&url)
  .header("Client-Id", client_id)
  .bearer_auth(bearer)
  .send()
  .await?
  .error_for_status()?;
 let v: Value = resp.json().await?;
 Ok(v.get("data")
  .and_then(|d| d.as_array())
  .and_then(|a| a.first())
  .and_then(|g| g.get("id"))
  .and_then(|i| i.as_str())
  .map(|s| s.to_string()))
}

async fn helix_patch_channel_game_id(client_id: &str, bearer: &str, broadcaster_id: &str, game_id: &str) -> Result<()> {
 let url = format!(
  "https://api.twitch.tv/helix/channels?broadcaster_id={}",
  urlencoding::encode(broadcaster_id)
 );
 let body = json!({"game_id": game_id});
 let resp = reqwest::Client::new()
  .patch(&url)
  .header("Client-Id", client_id)
  .bearer_auth(bearer)
  .json(&body)
  .send()
  .await?;
 if !resp.status().is_success() {
  let status = resp.status();
  let text = resp.text().await.unwrap_or_default();
  bail!("helix PATCH /channels failed: {} {}", status, text);
 }
 Ok(())
}

#[cfg(test)]
mod tests {
 use super::*;

 // ---------- parse_tools_json ----------

 #[test]
 fn parse_tools_accepts_chat_completions_legacy_format() {
  let j = r#"[{"type":"function","function":{"name":"vac_ping","description":"ping","parameters":{"type":"object","properties":{}}}}]"#;
  let v = parse_tools_json(j).unwrap();
  assert_eq!(v.len(), 1);
  match &v[0] {
   Tool::Function {
    name, description, ..
   } => {
    assert_eq!(name, "vac_ping");
    assert_eq!(description.as_deref(), Some("ping"));
   },
   other => panic!("expected Function, got {other:?}"),
  }
  assert!(declared_tool_names(&v).contains("vac_ping"));
 }

 #[test]
 fn parse_tools_accepts_responses_flat_format() {
  let j = r#"[{"type":"function","name":"vac_ping","description":"ping","parameters":{"type":"object","properties":{}},"strict":true}]"#;
  let v = parse_tools_json(j).unwrap();
  assert_eq!(v.len(), 1);
  match &v[0] {
   Tool::Function { name, strict, .. } => {
    assert_eq!(name, "vac_ping");
    assert_eq!(*strict, Some(true));
   },
   other => panic!("expected Function, got {other:?}"),
  }
 }

 #[test]
 fn parse_tools_preserves_strict_flag_from_legacy_format() {
  let j = r#"[{"type":"function","function":{"name":"vac_ping","parameters":{"type":"object"},"strict":true}}]"#;
  let v = parse_tools_json(j).unwrap();
  match &v[0] {
   Tool::Function { strict, .. } => assert_eq!(*strict, Some(true)),
   _ => panic!("expected Function"),
  }
 }

 #[test]
 fn parse_tools_accepts_hosted_web_search() {
  let j = r#"[{"type":"web_search","search_context_size":"medium"}]"#;
  let v = parse_tools_json(j).unwrap();
  match &v[0] {
   Tool::WebSearch {
    search_context_size, ..
   } => {
    assert_eq!(search_context_size.as_deref(), Some("medium"));
   },
   other => panic!("expected WebSearch, got {other:?}"),
  }
  assert!(declared_tool_names(&v).contains("web_search"));
  assert!(!v[0].is_locally_dispatched());
 }

 #[test]
 fn parse_tools_accepts_hosted_file_search() {
  let j = r#"[{"type":"file_search","vector_store_ids":["vs_1","vs_2"],"max_num_results":5}]"#;
  let v = parse_tools_json(j).unwrap();
  match &v[0] {
   Tool::FileSearch {
    vector_store_ids,
    max_num_results,
    ..
   } => {
    assert_eq!(vector_store_ids, &vec!["vs_1".to_string(), "vs_2".to_string()]);
    assert_eq!(*max_num_results, Some(5));
   },
   other => panic!("expected FileSearch, got {other:?}"),
  }
 }

 #[test]
 fn parse_tools_accepts_mixed_local_and_hosted() {
  let j = r#"[
   {"type":"function","function":{"name":"vac_ping","parameters":{"type":"object"}}},
   {"type":"web_search"},
   {"type":"function","name":"vac_other","parameters":{"type":"object"}}
  ]"#;
  let v = parse_tools_json(j).unwrap();
  assert_eq!(v.len(), 3);
  let local = locally_dispatched_tool_names(&v);
  assert!(local.contains("vac_ping"));
  assert!(local.contains("vac_other"));
  assert!(!local.contains("web_search"));
  assert!(declared_tool_names(&v).contains("web_search"));
 }

 #[test]
 fn parse_tools_rejects_malformed_function() {
  let j = r#"[{"type":"function","function":{}}]"#;
  assert!(parse_tools_json(j).is_err());
 }

 #[test]
 fn parse_tools_empty_string_returns_empty_vec() {
  assert!(parse_tools_json("").unwrap().is_empty());
  assert!(parse_tools_json("   ").unwrap().is_empty());
 }

 // ---------- parse_tool_choice ----------

 #[test]
 fn parse_tool_choice_auto_none_required() {
  assert!(matches!(parse_tool_choice("auto").unwrap(), ToolChoice::Mode(ToolChoiceMode::Auto)));
  assert!(matches!(parse_tool_choice("none").unwrap(), ToolChoice::Mode(ToolChoiceMode::None)));
  assert!(matches!(parse_tool_choice("required").unwrap(), ToolChoice::Mode(ToolChoiceMode::Required)));
 }

 #[test]
 fn parse_tool_choice_named_function() {
  match parse_tool_choice("function:vac_ping").unwrap() {
   ToolChoice::Named(NamedToolChoice::Function { name }) => assert_eq!(name, "vac_ping"),
   other => panic!("expected Named Function, got {other:?}"),
  }
 }

 #[test]
 fn parse_tool_choice_named_custom() {
  match parse_tool_choice("custom:freeform").unwrap() {
   ToolChoice::Named(NamedToolChoice::Custom { name }) => assert_eq!(name, "freeform"),
   other => panic!("expected Named Custom, got {other:?}"),
  }
 }

 #[test]
 fn parse_tool_choice_rejects_unknown() {
  assert!(parse_tool_choice("xyz").is_err());
  assert!(parse_tool_choice("function:").is_err());
 }

 // ---------- bridge: tools_to_chat_completions ----------

 #[test]
 fn bridge_converts_function_tool_preserving_strict() {
  let tools = vec![Tool::Function {
   name: "vac_ping".into(),
   description: Some("ping".into()),
   parameters: json!({"type": "object"}),
   strict: Some(true),
  }];
  let (cc, skipped) = tools_to_chat_completions(&tools).unwrap();
  assert_eq!(cc.len(), 1);
  assert!(skipped.is_empty());
  match &cc[0] {
   ChatCompletionTools::Function(ft) => {
    assert_eq!(ft.function.name, "vac_ping");
    assert_eq!(ft.function.strict, Some(true));
   },
   _ => panic!("expected CC Function"),
  }
 }

 #[test]
 fn bridge_reports_skipped_hosted_tools() {
  let tools = vec![
   Tool::Function {
    name: "vac_ping".into(),
    description: None,
    parameters: json!({"type": "object"}),
    strict: None,
   },
   Tool::WebSearch {
    user_location: None,
    search_context_size: None,
   },
   Tool::FileSearch {
    vector_store_ids: vec!["vs_1".into()],
    max_num_results: None,
    filters: None,
   },
  ];
  let (cc, skipped) = tools_to_chat_completions(&tools).unwrap();
  assert_eq!(cc.len(), 1, "Function だけ CC 変換される");
  assert_eq!(skipped, vec!["web_search".to_string(), "file_search".to_string()]);
 }

 #[test]
 fn bridge_tool_choice_roundtrip() {
  use async_openai::types::chat::ChatCompletionToolChoiceOption as CC;
  assert!(matches!(
   tool_choice_to_chat_completions(&ToolChoice::Mode(ToolChoiceMode::Auto)),
   CC::Mode(ToolChoiceOptions::Auto)
  ));
  assert!(matches!(
   tool_choice_to_chat_completions(&ToolChoice::Mode(ToolChoiceMode::None)),
   CC::Mode(ToolChoiceOptions::None)
  ));
  assert!(matches!(
   tool_choice_to_chat_completions(&ToolChoice::Mode(ToolChoiceMode::Required)),
   CC::Mode(ToolChoiceOptions::Required)
  ));
 }

 // ---------- dispatch_tool_call 周辺（State 依存のないロジックのみ） ----------

 // dispatch_tool_call 本体の async test は SharedState 構築コストが高いため
 // χ-5 の service/completion 統合後に integration test として追加する。
 // ここでは json_tool_error が declared 外呼び出しで返す形状だけ検証する。

 #[test]
 fn json_tool_error_is_valid_json_with_name() {
  let s = json_tool_error("tool not declared in openai_tools_json", "unauthorized_fn");
  let v: Value = serde_json::from_str(&s).unwrap();
  assert_eq!(v["error"], "tool not declared in openai_tools_json");
  assert_eq!(v["name"], "unauthorized_fn");
 }

 #[test]
 fn tool_name_helper() {
  assert_eq!(
   Tool::Function {
    name: "f".into(),
    description: None,
    parameters: json!({}),
    strict: None,
   }
   .name(),
   "f"
  );
  assert_eq!(
   Tool::WebSearch {
    user_location: None,
    search_context_size: None,
   }
   .name(),
   "web_search"
  );
 }

 #[test]
 fn json_err_is_valid_json() {
  let s = json_err("boom");
  let v: Value = serde_json::from_str(&s).unwrap();
  assert_eq!(v["ok"], false);
  assert_eq!(v["error"], "boom");
 }
}
