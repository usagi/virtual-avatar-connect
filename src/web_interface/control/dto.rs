//! Control API 蜷代￠縺ｮ **螟門髄縺・* DTO・・SON 陦ｨ迴ｾ・峨・
//!
//! 蜀・Κ蝙具ｼ・AiPersonaConf` 縺ｪ縺ｩ・峨ｒ逶ｴ謗･ serialize 縺吶ｋ縺ｨ縲√ヵ繧｣繝ｼ繝ｫ繝峨・讖溷ｯ・ｧ繧・ｾ梧婿莠呈鋤諤ｧ縺ｮ蛻ｶ蠕｡縺・
//! 髮｣縺励￥縺ｪ繧九ゅ％縺薙〒蝙九ｒ蛻・ｊ逶ｴ縺吶％縺ｨ縺ｧ縲；UI 蛛ｴ縺ｮ螂醍ｴ・ｒ譏守､ｺ繝ｻ螳牙ｮ壼喧縺吶ｋ縲・
//!
//! ﾎｴ-9 (v0.9.x) 縺ｧ V1 processor 螻､繧帝勁蜴ｻ縺励◆縺溘ａ縲～processors` 繝輔ぅ繝ｼ繝ｫ繝峨・繧ｹ繧ｭ繝ｼ繝槭°繧牙炎髯､縲・
//! GUI 縺ｯ Flowgraph 繝弱・繝我ｸ隕ｧ繧・`GET /api/v1/control/flowgraph/*` 邉ｻ邨檎罰縺ｧ蜿門ｾ励☆繧玖ｨｭ險医・

use serde::Serialize;

use crate::conf::Twitch;
use crate::twitch::oauth::{try_load_valid_token_for, OAuthIdent};
use crate::state::{AiRuntime, State};

/// `/api/v1/control/snapshot` 縺ｮ繝ｫ繝ｼ繝・DTO縲・
#[derive(Debug, Serialize)]
pub struct StateSnapshot {
 /// API DTO 縺ｮ繧ｹ繧ｭ繝ｼ繝槭ヰ繝ｼ繧ｸ繝ｧ繝ｳ縲らｴ螢顔噪螟画峩譎ゅ↓繧､繝ｳ繧ｯ繝ｪ繝｡繝ｳ繝医・
 /// v0.9 縺ｧ V1 processor 螻､繧帝勁蜴ｻ縺励◆髫帙↓ schema=2 縺ｫ譖ｴ譁ｰ縲・
 pub schema: u32,
 pub app_version: &'static str,
 pub now: String,
 pub runtime: RuntimeSummary,
 pub ai_personas: Vec<AiPersonaSummary>,
 pub twitch: Option<TwitchSummary>,
}

#[derive(Debug, Serialize)]
pub struct RuntimeSummary {
 pub session_id: String,
 pub root: String,
 pub session_dir: String,
 pub inline_max_bytes: u64,
}

#[derive(Debug, Serialize)]
pub struct AiPersonaSummary {
 pub index: usize,
 pub id: Option<String>,
 pub paused: bool,
}

#[derive(Debug, Serialize)]
pub struct TwitchSummary {
 pub username: String,
 pub channel_to: String,
 pub reads: Option<Vec<String>>,
 pub eventsub_enabled: bool,
 pub moderator_enabled: bool,
 pub moderator_login: Option<String>,
 pub ignore_logins: Vec<String>,
 /// broadcaster 逕ｨ縺ｮ菫晏ｭ倥ヨ繝ｼ繧ｯ繝ｳ縺梧怏蜉ｹ縺九Ａeventsub` 險ｭ螳壹′辟｡縺・↑縺ｩ ident 縺檎ｵ・ａ縺ｪ縺・→縺阪・ false縲・
 pub broadcaster_authorized: bool,
 /// moderator 逕ｨ縺ｮ菫晏ｭ倥ヨ繝ｼ繧ｯ繝ｳ縺梧怏蜉ｹ縺九・
 pub moderator_authorized: Option<bool>,
}

/// `State` 縺九ｉ邏皮ｲ九↑ read-only 縺ｪ snapshot 繧堤ｵ・∩遶九※繧九・
pub async fn snapshot(state: &State) -> StateSnapshot {
 let runtime = RuntimeSummary {
  session_id: state.runtime_paths.session_id.clone(),
  root: state.runtime_paths.root.display().to_string(),
  session_dir: state.runtime_paths.session_dir.display().to_string(),
  inline_max_bytes: state.runtime_paths.inline_max_bytes,
 };

 let ai_personas = {
  let guard = state.ai_runtimes.read().await;
  guard
   .iter()
   .enumerate()
   .map(|(i, rt)| ai_persona_summary(i, rt))
   .collect::<Vec<_>>()
 };

 let twitch = if let Some(t) = state.twitch.as_ref() {
  let ignore_logins = t.ignore_logins.as_ref().cloned().unwrap_or_default();

  // 菫晏ｭ俶ｸ医∩繝医・繧ｯ繝ｳ縺ｮ譛牙柑諤ｧ繧偵メ繧ｧ繝・け縲ゅ％繧後・ GUI 縺ｮ OAuth 繝代ロ繝ｫ縺・
  // 縲後・繧ｿ繝ｳ繧呈款縺吝燕縺九ｉ隱榊庄貂医∩縺九←縺・°縲阪ｒ陦ｨ遉ｺ縺吶ｋ縺溘ａ縺ｫ蠢・ｦ√・
  let (broadcaster_authorized, moderator_authorized) = compute_twitch_authorized(t).await;

  Some(TwitchSummary {
   username: t.username.clone(),
   channel_to: t.effective_channel_to().to_string(),
   reads: t.reads.clone(),
   eventsub_enabled: t.eventsub.as_ref().map(|e| e.enabled).unwrap_or(false),
   moderator_enabled: t.moderator.as_ref().map(|m| m.enabled).unwrap_or(false),
   moderator_login: t.moderator.as_ref().and_then(|m| m.login.clone()),
   ignore_logins,
   broadcaster_authorized,
   moderator_authorized,
  })
 } else {
  None
 };

 StateSnapshot {
  schema: 2,
  app_version: env!("CARGO_PKG_VERSION"),
  now: chrono::Utc::now().to_rfc3339(),
  runtime,
  ai_personas,
  twitch,
 }
}

fn ai_persona_summary(index: usize, rt: &AiRuntime) -> AiPersonaSummary {
 AiPersonaSummary {
  index,
  id: rt.persona_id.clone(),
  paused: rt.is_paused(),
 }
}

/// Twitch 縺ｮ菫晏ｭ俶ｸ医∩繝医・繧ｯ繝ｳ縺梧怏蜉ｹ縺九←縺・°繧貞愛螳壹☆繧九・
async fn compute_twitch_authorized(t: &Twitch) -> (bool, Option<bool>) {
 // eventsub 險ｭ螳壹′辟｡縺代ｌ縺ｰ OAuthIdent 繧堤ｵ・ａ縺ｪ縺・・縺ｧ縲√＞縺壹ｌ繧よ悴隱榊庄謇ｱ縺・・
 let Some(es) = t.eventsub.as_ref() else {
  return (false, t.moderator.as_ref().map(|_| false));
 };

 let b_ident = OAuthIdent::for_broadcaster(es);
 let broadcaster_authorized = if b_ident.client_id.is_empty() {
  false
 } else {
  try_load_valid_token_for(&b_ident).await.is_some()
 };

 let moderator_authorized = if let Some(mc) = t.moderator.as_ref() {
  let m_ident = OAuthIdent::for_moderator(es, mc);
  let ok = if m_ident.client_id.is_empty() {
   false
  } else {
   try_load_valid_token_for(&m_ident).await.is_some()
  };
  Some(ok)
 } else {
  None
 };

 (broadcaster_authorized, moderator_authorized)
}
