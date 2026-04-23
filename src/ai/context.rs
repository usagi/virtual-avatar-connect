//! AI サービス用のコンテキスト組み立て（チャンネル履歴 → Chat API メッセージ）。
//!
//! 旧 `src/processor/openai_chat/context.rs` を Phase I で `src/ai/` 配下へ昇格。
//! `channel_from` / `channel_to` の 2 本足表現を `ObserveSet`（triggers[] + channel_utterance + include_all/additional/exclude）に一般化した。

use super::config::OpenAiFewShotTurn;
use super::model_policy;
use super::observe::ObserveSet;
use crate::ai::openai_responses::types::input::{InputContent, InputItem};
use crate::ChannelDatum;
use async_openai::types::chat::{
 ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
 ChatCompletionRequestUserMessageArgs,
};

use std::collections::VecDeque;

/// トリガー ID が見つからない、または未確定（`is_final` でない）場合。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MemoryWindowError {
 TriggerNotFound,
 TriggerNotFinal,
}

/// `channel_data` は先頭が古く末尾が新しい `VecDeque` とする（`SharedChannelData` と同じ前提）。
/// `observe` に従って memory window に載せるチャンネルを選別する。
pub(crate) fn collect_memory_window(
 channel_data: &VecDeque<ChannelDatum>,
 trigger_id: u64,
 memory_capacity: usize,
 observe: &ObserveSet,
) -> Result<Vec<ChannelDatum>, MemoryWindowError> {
 let index = channel_data
  .iter()
  .rev()
  .position(|cd| cd.get_id() == trigger_id)
  .ok_or(MemoryWindowError::TriggerNotFound)?;

 if !channel_data
  .iter()
  .rev()
  .nth(index)
  .expect("index は position で得たので存在する")
  .has_flag(ChannelDatum::FLAG_IS_FINAL)
 {
  return Err(MemoryWindowError::TriggerNotFinal);
 }

 let base = channel_data
  .iter()
  .rev()
  .skip(index)
  .filter(|cd| cd.has_flag(ChannelDatum::FLAG_IS_FINAL))
  .filter(|cd| observe.is_observed(&cd.channel));

 Ok(base.take(memory_capacity).cloned().collect())
}

/// 軽量なトークン近似: `chars_per_approx_token` 文字を 1 トークンとみなす（`ceil(chars / n)`）。
#[inline]
pub(crate) fn approx_tokens_for_text(s: &str, chars_per_approx_token: u8) -> usize {
 let n = s.chars().count();
 let d = usize::from(chars_per_approx_token.max(1));
 (n + d - 1) / d
}

/// `datums` は **新しい順**（`[0]` が最新）。
pub(crate) fn shrink_memory_window_by_chars(datums: &mut Vec<ChannelDatum>, max_chars: Option<usize>) -> Vec<ChannelDatum> {
 let mut dropped = Vec::new();
 let Some(limit) = max_chars else { return dropped };
 loop {
  let total: usize = datums.iter().map(|d| d.content.chars().count()).sum();
  if total <= limit {
   break;
  }
  if datums.len() <= 1 {
   break;
  }
  if let Some(d) = datums.pop() {
   dropped.push(d);
  }
 }
 dropped
}

pub(crate) fn shrink_memory_window_by_approx_tokens(
 datums: &mut Vec<ChannelDatum>,
 max_tokens: Option<usize>,
 chars_per_approx_token: u8,
) -> Vec<ChannelDatum> {
 let mut dropped = Vec::new();
 let Some(limit) = max_tokens else { return dropped };
 let cpp = chars_per_approx_token.max(1);
 loop {
  let total: usize = datums.iter().map(|d| approx_tokens_for_text(&d.content, cpp)).sum();
  if total <= limit {
   break;
  }
  if datums.len() <= 1 {
   break;
  }
  if let Some(d) = datums.pop() {
   dropped.push(d);
  }
 }
 dropped
}

pub(crate) fn format_dropped_turns_for_overflow_summary(dropped: &[ChannelDatum]) -> String {
 let mut out = String::new();
 for d in dropped {
  out.push('[');
  out.push_str(&d.channel);
  out.push_str("] ");
  out.push_str(&d.content);
  out.push('\n');
 }
 out
}

pub(crate) fn truncate_overflow_summary_input(s: &str, max_chars: usize) -> String {
 let n = s.chars().count();
 if n <= max_chars {
  return s.to_string();
 }
 s.chars().skip(n.saturating_sub(max_chars)).collect()
}

/// `datums` は **新しい順**（`[0]` が最新）。Chat Completions 用に古い順へ並べ替えてメッセージ列を返す。
pub(crate) fn memory_datums_to_chat_messages(
 datums_newest_first: &[ChannelDatum],
 observe: &ObserveSet,
) -> Vec<ChatCompletionRequestMessage> {
 datums_newest_first
  .iter()
  .rev()
  .filter_map(|cd| datum_to_chat_message(cd, observe))
  .collect()
}

fn datum_to_chat_message(cd: &ChannelDatum, observe: &ObserveSet) -> Option<ChatCompletionRequestMessage> {
 if cd.channel == observe.channel_utterance {
  return ChatCompletionRequestAssistantMessageArgs::default()
   .content(cd.content.clone())
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::Assistant);
 }
 if observe.is_trigger(&cd.channel) {
  return ChatCompletionRequestUserMessageArgs::default()
   .content(cd.content.clone())
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::User);
 }
 // 補助観測チャンネル（`[actor@channel]` or `[channel]` ラベル付き system）
 let label = match cd.source.as_ref().and_then(|s| s.actor.as_deref()) {
  Some(a) if !a.is_empty() => format!("[{}@{}]", a, cd.channel),
  _ => format!("[{}]", cd.channel),
 };
 let body = format!("{} {}", label, cd.content);
 ChatCompletionRequestSystemMessageArgs::default()
  .content(body)
  .build()
  .ok()
  .map(ChatCompletionRequestMessage::System)
}

/// OpenAI に送る `messages` を一括で組み立てる。
///
/// ## 順序（固定）
///
/// 1. **人格** — `custom_instructions`（system）、任意で `system_instructions_extra`（system）
/// 2. **gpt-5 系**で system が 1 本も無いときだけ、既定 system を **先頭に** insert
/// 3. **長期** — `memory_summary` 連結文（system）
/// 4. **few-shot** — `openai_few_shot` を `role` どおり user / assistant / system
/// 5. **アンカー** — `persona_anchor`（system）で履歴直前に口調を再掲
/// 6. **短期窓** — メモリ窓の user / assistant（補助チャンネルは `[名前]` 付き system 等）
pub(crate) fn assemble_openai_chat_messages(
 model_id: Option<&str>,
 custom_instructions: Option<&str>,
 system_instructions_extra: Option<&str>,
 memory_summary_combined: Option<&str>,
 few_shot: &[OpenAiFewShotTurn],
 persona_anchor: Option<&str>,
 datums_newest_first: &[ChannelDatum],
 observe: &ObserveSet,
) -> Vec<ChatCompletionRequestMessage> {
 let mut messages: Vec<ChatCompletionRequestMessage> = Vec::new();

 if let Some(s) = custom_instructions.map(str::trim).filter(|t| !t.is_empty()) {
  if let Some(m) = ChatCompletionRequestSystemMessageArgs::default()
   .content(s.to_string())
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::System)
  {
   messages.push(m);
  }
 }
 if let Some(s) = system_instructions_extra.map(str::trim).filter(|t| !t.is_empty()) {
  if let Some(m) = ChatCompletionRequestSystemMessageArgs::default()
   .content(s.to_string())
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::System)
  {
   messages.push(m);
  }
 }

 ensure_gpt5_default_system_if_missing_vec(&mut messages, model_id);

 if let Some(text) = memory_summary_combined.map(str::trim).filter(|t| !t.is_empty()) {
  if let Some(m) = ChatCompletionRequestSystemMessageArgs::default()
   .content(format!("長期メモリ・要約（参考）:\n{text}"))
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::System)
  {
   messages.push(m);
  }
 }

 messages.extend(few_shot_turns_to_messages(few_shot));

 if let Some(s) = persona_anchor.map(str::trim).filter(|t| !t.is_empty()) {
  if let Some(m) = ChatCompletionRequestSystemMessageArgs::default()
   .content(s.to_string())
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::System)
  {
   messages.push(m);
  }
 }

 messages.extend(memory_datums_to_chat_messages(datums_newest_first, observe));

 messages
}

fn few_shot_turns_to_messages(turns: &[OpenAiFewShotTurn]) -> Vec<ChatCompletionRequestMessage> {
 let mut out = Vec::with_capacity(turns.len());
 for t in turns {
  let role = t.role.to_lowercase();
  let content = t.content.trim();
  if content.is_empty() {
   continue;
  }
  match role.as_str() {
   "user" => {
    if let Some(m) = ChatCompletionRequestUserMessageArgs::default()
     .content(content.to_string())
     .build()
     .ok()
     .map(ChatCompletionRequestMessage::User)
    {
     out.push(m);
    }
   },
   "assistant" => {
    if let Some(m) = ChatCompletionRequestAssistantMessageArgs::default()
     .content(content.to_string())
     .build()
     .ok()
     .map(ChatCompletionRequestMessage::Assistant)
    {
     out.push(m);
    }
   },
   "system" => {
    if let Some(m) = ChatCompletionRequestSystemMessageArgs::default()
     .content(content.to_string())
     .build()
     .ok()
     .map(ChatCompletionRequestMessage::System)
    {
     out.push(m);
    }
   },
   _ => {
    log::warn!("openai_few_shot: 未知の role {:?} をスキップします。", t.role);
   },
  }
 }
 out
}

// ============================================================
// χ-4: Responses API 用の input 組み立て。Chat Completions 版と同じ順序規則に従う。
// ============================================================

/// gpt-5 / o-series の reasoning model で推奨される instruction role。
///
/// OpenAI Responses API では `"developer"` role が reasoning model で特別扱いされる
/// （`"system"` より優先度が高く、`store=true` のときも漏らさない等）。非 reasoning model
/// でも受理されるが、VAC は従来挙動との整合を取るため gpt-5 族のみ `"developer"` を使う。
#[inline]
pub(crate) fn instruction_role_for_model(model_id: Option<&str>) -> &'static str
{
 if model_policy::is_gpt5_family(model_id)
 {
  "developer"
 }
 else
 {
  "system"
 }
}

/// Responses API の `input[]` を組み立てる。[`assemble_openai_chat_messages`] の Responses 版。
///
/// ## 順序（Chat Completions 版と同一）
///
/// 1. `custom_instructions`（instruction role）+ `system_instructions_extra`（同）
/// 2. gpt-5 系で instruction が 1 本も無いなら既定 instruction を先頭 insert
/// 3. `memory_summary_combined`（system）
/// 4. `openai_few_shot` を role 通りに
/// 5. `persona_anchor`（system）
/// 6. memory window の user / assistant（補助チャンネルは `[actor@channel]` 付き system）
///
/// ## 差分
///
/// - Chat Completions の `role: "system"` に当たるもののうち、**ペルソナ設定の
///   instructions** は gpt-5 系で `role: "developer"` にする。既定 instruction の
///   先頭 insert / memory_summary / few_shot の system / persona_anchor は従来どおり
///   `role: "system"` のままにして、Responses API のセマンティクスに素直に合わせる。
pub(crate) fn assemble_openai_responses_input(
 model_id: Option<&str>,
 custom_instructions: Option<&str>,
 system_instructions_extra: Option<&str>,
 memory_summary_combined: Option<&str>,
 few_shot: &[OpenAiFewShotTurn],
 persona_anchor: Option<&str>,
 datums_newest_first: &[ChannelDatum],
 observe: &ObserveSet,
) -> Vec<InputItem>
{
 let instruction_role = instruction_role_for_model(model_id);
 let mut items: Vec<InputItem> = Vec::new();

 if let Some(s) = custom_instructions.map(str::trim).filter(|t| !t.is_empty())
 {
  items.push(InputItem::message(instruction_role, s));
 }
 if let Some(s) = system_instructions_extra.map(str::trim).filter(|t| !t.is_empty())
 {
  items.push(InputItem::message(instruction_role, s));
 }

 ensure_gpt5_default_instruction_if_missing(&mut items, model_id, instruction_role);

 if let Some(text) = memory_summary_combined.map(str::trim).filter(|t| !t.is_empty())
 {
  items.push(InputItem::message(
   "system",
   format!("長期メモリ・要約（参考）:\n{text}"),
  ));
 }

 items.extend(few_shot_turns_to_input_items(few_shot));

 if let Some(s) = persona_anchor.map(str::trim).filter(|t| !t.is_empty())
 {
  items.push(InputItem::message("system", s));
 }

 items.extend(memory_datums_to_input_items(datums_newest_first, observe));

 items
}

fn few_shot_turns_to_input_items(turns: &[OpenAiFewShotTurn]) -> Vec<InputItem>
{
 let mut out = Vec::with_capacity(turns.len());
 for t in turns
 {
  let role = t.role.to_lowercase();
  let content = t.content.trim();
  if content.is_empty()
  {
   continue;
  }
  match role.as_str()
  {
   "user" | "assistant" | "system" | "developer" => out.push(InputItem::message(role, content)),
   _ => log::warn!("openai_few_shot: 未知の role {:?} をスキップします。", t.role),
  }
 }
 out
}

fn memory_datums_to_input_items(datums_newest_first: &[ChannelDatum], observe: &ObserveSet) -> Vec<InputItem>
{
 datums_newest_first
  .iter()
  .rev()
  .filter_map(|cd| datum_to_input_item(cd, observe))
  .collect()
}

fn datum_to_input_item(cd: &ChannelDatum, observe: &ObserveSet) -> Option<InputItem>
{
 if cd.channel == observe.channel_utterance
 {
  return Some(InputItem::message("assistant", cd.content.clone()));
 }
 if observe.is_trigger(&cd.channel)
 {
  return Some(InputItem::message("user", cd.content.clone()));
 }
 let label = match cd.source.as_ref().and_then(|s| s.actor.as_deref())
 {
  Some(a) if !a.is_empty() => format!("[{}@{}]", a, cd.channel),
  _ => format!("[{}]", cd.channel),
 };
 Some(InputItem::message("system", format!("{} {}", label, cd.content)))
}

fn ensure_gpt5_default_instruction_if_missing(
 items: &mut Vec<InputItem>,
 model_id: Option<&str>,
 instruction_role: &str,
)
{
 if !model_policy::needs_default_system_when_missing(model_id)
 {
  return;
 }
 let has_any_instruction = items.iter().any(|item| match item
 {
  InputItem::Message { role, .. } => role == "system" || role == "developer",
  _ => false,
 });
 if has_any_instruction
 {
  return;
 }
 items.insert(
  0,
  InputItem::message(
   instruction_role,
   "You are a helpful assistant. Provide a concise, direct response to the user. 日本語入力には日本語で返答して下さい。",
  ),
 );
}

/// `InputItem::Message` の content が文字列であることを前提に参照を返す。
/// テスト / デバッグ専用のヘルパ。
#[cfg(test)]
pub(crate) fn input_item_text(item: &InputItem) -> Option<&str>
{
 match item
 {
  InputItem::Message {
   content: InputContent::Text(s),
   ..
  } => Some(s),
  _ => None,
 }
}

pub(crate) fn ensure_gpt5_default_system_if_missing_vec(
 messages: &mut Vec<ChatCompletionRequestMessage>,
 model_id: Option<&str>,
) {
 if !model_policy::needs_default_system_when_missing(model_id) {
  return;
 }
 let has_system = messages.iter().any(|m| matches!(m, ChatCompletionRequestMessage::System(_)));
 if has_system {
  return;
 }
 if let Some(sys_msg) = ChatCompletionRequestSystemMessageArgs::default()
  .content("You are a helpful assistant. Provide a concise, direct response to the user. 日本語入力には日本語で返答して下さい。")
  .build()
  .ok()
  .map(ChatCompletionRequestMessage::System)
 {
  messages.insert(0, sys_msg);
 }
}

#[cfg(test)]
mod tests {
 use super::*;
 use crate::ai::config::ObservePolicyConf;
 use crate::ai::openai_responses::types::input::InputItem;
 use async_openai::types::chat::{
  ChatCompletionRequestAssistantMessageContent, ChatCompletionRequestSystemMessageContent, ChatCompletionRequestUserMessageContent,
 };

 fn datum_with_final(channel: &str, content: &str) -> ChannelDatum {
  ChannelDatum::new(channel.to_string(), content.to_string()).with_flag(ChannelDatum::FLAG_IS_FINAL)
 }

 fn simple_observe(trigger: &str, utterance: &str) -> ObserveSet {
  ObserveSet::from_conf(
   utterance,
   &ObservePolicyConf {
    triggers: vec![trigger.to_string()],
    ..Default::default()
   },
  )
 }

 #[test]
 fn collect_memory_window_respects_capacity_and_channels() {
  ChannelDatum::reset_id_counter(0);
  let mut q: VecDeque<ChannelDatum> = VecDeque::new();
  q.push_back(datum_with_final("from_ch", "a"));
  q.push_back(datum_with_final("to_ch", "b"));
  q.push_back(datum_with_final("from_ch", "c"));
  q.push_back(datum_with_final("noise", "x"));
  let trigger_id = q.back().unwrap().get_id();
  let obs = simple_observe("from_ch", "to_ch");
  let win = collect_memory_window(&q, trigger_id, 10, &obs).expect("ok");
  assert_eq!(win.len(), 3);
  assert_eq!(win[0].content, "c");
 }

 #[test]
 fn collect_memory_window_errors_when_trigger_missing() {
  let q: VecDeque<ChannelDatum> = VecDeque::new();
  let obs = simple_observe("u", "a");
  assert!(matches!(
   collect_memory_window(&q, 999, 4, &obs),
   Err(MemoryWindowError::TriggerNotFound)
  ));
 }

 #[test]
 fn collect_memory_window_errors_when_not_final() {
  ChannelDatum::reset_id_counter(0);
  let mut q: VecDeque<ChannelDatum> = VecDeque::new();
  q.push_back(ChannelDatum::new("from_ch".to_string(), "t".to_string()));
  let id = q.back().unwrap().get_id();
  let obs = simple_observe("from_ch", "to_ch");
  assert!(matches!(
   collect_memory_window(&q, id, 4, &obs),
   Err(MemoryWindowError::TriggerNotFinal)
  ));
 }

 #[test]
 fn memory_datums_order_oldest_first_in_messages() {
  let u = |s| datum_with_final("from_ch", s);
  let a = |s| datum_with_final("to_ch", s);
  let datums = vec![u("u3"), a("a2"), u("u1")];
  let obs = simple_observe("from_ch", "to_ch");
  let msgs = memory_datums_to_chat_messages(&datums, &obs);
  assert_eq!(msgs.len(), 3);
  match (&msgs[0], &msgs[1], &msgs[2]) {
   (
    ChatCompletionRequestMessage::User(u0),
    ChatCompletionRequestMessage::Assistant(a0),
    ChatCompletionRequestMessage::User(u1),
   ) => {
    assert!(matches!(
     &u0.content,
     ChatCompletionRequestUserMessageContent::Text(s) if s == "u1"
    ));
    assert!(matches!(
     a0.content,
     Some(ChatCompletionRequestAssistantMessageContent::Text(ref s)) if s == "a2"
    ));
    assert!(matches!(
     &u1.content,
     ChatCompletionRequestUserMessageContent::Text(s) if s == "u3"
    ));
   },
   _ => panic!("unexpected message kinds: {:?}", msgs),
  }
 }

 #[test]
 fn ensure_gpt5_inserts_system_only_when_needed() {
  let mut msgs: Vec<ChatCompletionRequestMessage> = vec![];
  ensure_gpt5_default_system_if_missing_vec(&mut msgs, Some("gpt-5-mini"));
  assert!(matches!(msgs.first(), Some(ChatCompletionRequestMessage::System(_))));

  let mut msgs2 = vec![ChatCompletionRequestSystemMessageArgs::default()
   .content("x".to_string())
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::System)
   .unwrap()];
  ensure_gpt5_default_system_if_missing_vec(&mut msgs2, Some("gpt-5-mini"));
  assert_eq!(msgs2.len(), 1);
 }

 #[test]
 fn unknown_channel_maps_to_system_message() {
  let d = datum_with_final("extra", "ctx");
  let obs = ObserveSet::from_conf(
   "to_ch",
   &ObservePolicyConf {
    triggers: vec!["from_ch".to_string()],
    include_additional: vec!["extra".to_string()],
    ..Default::default()
   },
  );
  let msgs = memory_datums_to_chat_messages(&[d], &obs);
  assert_eq!(msgs.len(), 1);
  assert!(matches!(msgs[0], ChatCompletionRequestMessage::System(_)));
 }

 #[test]
 fn shrink_memory_window_drops_oldest_until_under_cap() {
  ChannelDatum::reset_id_counter(0);
  let mut q: VecDeque<ChannelDatum> = VecDeque::new();
  q.push_back(datum_with_final("from_ch", "aaaa"));
  q.push_back(datum_with_final("to_ch", "bbbb"));
  q.push_back(datum_with_final("from_ch", "cccc"));
  let trigger_id = q.back().unwrap().get_id();
  let obs = simple_observe("from_ch", "to_ch");
  let mut win = collect_memory_window(&q, trigger_id, 10, &obs).expect("ok");
  assert_eq!(win.len(), 3);
  let dropped = shrink_memory_window_by_chars(&mut win, Some(8));
  assert_eq!(dropped.len(), 1);
  assert_eq!(win.len(), 2);
  assert_eq!(win[0].content, "cccc");
  assert_eq!(win[1].content, "bbbb");
 }

 #[test]
 fn shrink_memory_window_by_tokens_drops_oldest() {
  let mut win = vec![datum_with_final("from_ch", "aaaaaaaa"), datum_with_final("to_ch", "bbbbbbbb")];
  let dropped = shrink_memory_window_by_approx_tokens(&mut win, Some(3), 4);
  assert_eq!(dropped.len(), 1);
  assert_eq!(win.len(), 1);
 }

 #[test]
 fn format_dropped_turns_and_truncate_input() {
  let a = datum_with_final("from_ch", "hello");
  let b = datum_with_final("to_ch", "reply");
  let s = format_dropped_turns_for_overflow_summary(&[a, b]);
  assert!(s.contains("[from_ch]"));
  assert!(s.contains("[to_ch]"));
  let long = "あ".repeat(10);
  let t = truncate_overflow_summary_input(&long, 5);
  assert_eq!(t.chars().count(), 5);
 }

 #[test]
 fn approx_tokens_respects_chars_per_token() {
  assert_eq!(approx_tokens_for_text("abcd", 4), 1);
  assert_eq!(approx_tokens_for_text("abcd", 2), 2);
  assert_eq!(approx_tokens_for_text("abcde", 4), 2);
 }

 #[test]
 fn collect_memory_window_includes_all_channels_when_enabled() {
  ChannelDatum::reset_id_counter(0);
  let mut q: VecDeque<ChannelDatum> = VecDeque::new();
  q.push_back(datum_with_final("from_ch", "a"));
  q.push_back(datum_with_final("noise", "n"));
  let trigger_id = q.back().unwrap().get_id();
  let obs = ObserveSet::from_conf(
   "to_ch",
   &ObservePolicyConf {
    triggers: vec!["from_ch".to_string()],
    include_all: true,
    ..Default::default()
   },
  );
  let win = collect_memory_window(&q, trigger_id, 10, &obs).expect("ok");
  assert_eq!(win.len(), 2);
  assert_eq!(win[0].content, "n");
 }

 #[test]
 fn assemble_order_persona_summary_fewshot_anchor_then_user() {
  let u = datum_with_final("from_ch", "最新");
  let few = vec![
   OpenAiFewShotTurn {
    role: "user".to_string(),
    content: "fsu".to_string(),
   },
   OpenAiFewShotTurn {
    role: "assistant".to_string(),
    content: "fsa".to_string(),
   },
  ];
  let obs = simple_observe("from_ch", "to_ch");
  let msgs = assemble_openai_chat_messages(
   Some("gpt-4o"),
   Some("人格"),
   None,
   Some("要約"),
   &few,
   Some("アンカー"),
   std::slice::from_ref(&u),
   &obs,
  );
  assert_eq!(msgs.len(), 6);
  assert!(matches!(&msgs[0], ChatCompletionRequestMessage::System(_)));
  assert!(matches!(&msgs[1], ChatCompletionRequestMessage::System(_)));
  assert!(matches!(&msgs[2], ChatCompletionRequestMessage::User(_)));
  assert!(matches!(&msgs[3], ChatCompletionRequestMessage::Assistant(_)));
  assert!(matches!(&msgs[4], ChatCompletionRequestMessage::System(_)));
  assert!(matches!(&msgs[5], ChatCompletionRequestMessage::User(_)));
 }

 #[test]
 fn auxiliary_channels_get_bracket_label() {
  let d = ChannelDatum::new("twitch".to_string(), "hello".to_string()).with_flag(ChannelDatum::FLAG_IS_FINAL);
  let obs = ObserveSet::from_conf(
   "to_ch",
   &ObservePolicyConf {
    triggers: vec!["from_ch".to_string()],
    include_additional: vec!["twitch".to_string()],
    ..Default::default()
   },
  );
  let msgs = memory_datums_to_chat_messages(&[d], &obs);
  match &msgs[0] {
   ChatCompletionRequestMessage::System(s) => match &s.content {
    ChatCompletionRequestSystemMessageContent::Text(t) => {
     assert!(t.contains("[twitch]"));
     assert!(t.contains("hello"));
    },
    _ => panic!("unexpected system content"),
   },
   _ => panic!("expected system"),
  }
 }

 // ============================================================
 // χ-4: Responses 版 input assembly テスト
 // ============================================================

 fn role_of(item: &InputItem) -> Option<&str> {
  match item {
   InputItem::Message { role, .. } => Some(role.as_str()),
   _ => None,
  }
 }

 #[test]
 fn responses_assemble_order_persona_summary_fewshot_anchor_then_user() {
  let u = datum_with_final("from_ch", "最新");
  let few = vec![
   OpenAiFewShotTurn {
    role: "user".to_string(),
    content: "fsu".to_string(),
   },
   OpenAiFewShotTurn {
    role: "assistant".to_string(),
    content: "fsa".to_string(),
   },
  ];
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(
   Some("gpt-4o"),
   Some("人格"),
   None,
   Some("要約"),
   &few,
   Some("アンカー"),
   std::slice::from_ref(&u),
   &obs,
  );
  assert_eq!(items.len(), 6);
  assert_eq!(role_of(&items[0]), Some("system"), "非 gpt-5 は system のまま");
  assert_eq!(role_of(&items[1]), Some("system"));
  assert_eq!(role_of(&items[2]), Some("user"));
  assert_eq!(role_of(&items[3]), Some("assistant"));
  assert_eq!(role_of(&items[4]), Some("system"));
  assert_eq!(role_of(&items[5]), Some("user"));
  assert_eq!(input_item_text(&items[5]).unwrap(), "最新");
 }

 #[test]
 fn responses_assemble_uses_developer_role_for_gpt5_instructions() {
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(
   Some("gpt-5-mini"),
   Some("人格本体"),
   Some("追加指示"),
   None,
   &[],
   None,
   &[],
   &obs,
  );
  assert_eq!(items.len(), 2);
  assert_eq!(role_of(&items[0]), Some("developer"));
  assert_eq!(role_of(&items[1]), Some("developer"));
  assert_eq!(input_item_text(&items[0]).unwrap(), "人格本体");
 }

 #[test]
 fn responses_assemble_inserts_default_instruction_for_gpt5_when_missing() {
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(Some("gpt-5-mini"), None, None, None, &[], None, &[], &obs);
  assert_eq!(items.len(), 1);
  assert_eq!(role_of(&items[0]), Some("developer"));
  let text = input_item_text(&items[0]).unwrap();
  assert!(text.contains("helpful assistant"));
  assert!(text.contains("日本語"));
 }

 #[test]
 fn responses_assemble_skips_default_instruction_for_non_gpt5() {
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(Some("gpt-4o"), None, None, None, &[], None, &[], &obs);
  assert!(items.is_empty());
 }

 #[test]
 fn responses_assemble_skips_default_when_any_instruction_present() {
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(Some("gpt-5-mini"), Some("人格"), None, None, &[], None, &[], &obs);
  assert_eq!(items.len(), 1);
  assert_eq!(role_of(&items[0]), Some("developer"));
  assert_eq!(input_item_text(&items[0]).unwrap(), "人格");
 }

 #[test]
 fn responses_assemble_auxiliary_channels_use_bracket_label() {
  use crate::state::DataSource;
  let d = ChannelDatum::new("twitch".to_string(), "hello".to_string())
   .with_flag(ChannelDatum::FLAG_IS_FINAL)
   .with_source(DataSource::new("twitch.eventsub").with_actor("alice"));
  let obs = ObserveSet::from_conf(
   "to_ch",
   &ObservePolicyConf {
    triggers: vec!["from_ch".to_string()],
    include_additional: vec!["twitch".to_string()],
    ..Default::default()
   },
  );
  let items = assemble_openai_responses_input(None, None, None, None, &[], None, &[d], &obs);
  assert_eq!(items.len(), 1);
  assert_eq!(role_of(&items[0]), Some("system"));
  let text = input_item_text(&items[0]).unwrap();
  assert!(text.contains("[alice@twitch]"), "got: {text}");
  assert!(text.contains("hello"));
 }

 #[test]
 fn responses_assemble_memory_window_oldest_first() {
  let u = |s| datum_with_final("from_ch", s);
  let a = |s| datum_with_final("to_ch", s);
  let datums = vec![u("u3"), a("a2"), u("u1")];
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(None, None, None, None, &[], None, &datums, &obs);
  assert_eq!(items.len(), 3);
  assert_eq!(role_of(&items[0]), Some("user"));
  assert_eq!(input_item_text(&items[0]).unwrap(), "u1");
  assert_eq!(role_of(&items[1]), Some("assistant"));
  assert_eq!(input_item_text(&items[1]).unwrap(), "a2");
  assert_eq!(role_of(&items[2]), Some("user"));
  assert_eq!(input_item_text(&items[2]).unwrap(), "u3");
 }

 #[test]
 fn responses_assemble_memory_summary_uses_system_role() {
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(
   Some("gpt-5-mini"),
   None,
   None,
   Some("要約テキスト"),
   &[],
   None,
   &[],
   &obs,
  );
  assert!(items.iter().any(|i| role_of(i) == Some("system")
   && input_item_text(i).map(|t| t.contains("要約テキスト")).unwrap_or(false)));
 }

 #[test]
 fn responses_assemble_few_shot_skips_empty_and_unknown_roles() {
  let few = vec![
   OpenAiFewShotTurn {
    role: "user".to_string(),
    content: "  ".to_string(),
   },
   OpenAiFewShotTurn {
    role: "tool".to_string(),
    content: "tool output".to_string(),
   },
   OpenAiFewShotTurn {
    role: "developer".to_string(),
    content: "dev note".to_string(),
   },
  ];
  let obs = simple_observe("from_ch", "to_ch");
  let items = assemble_openai_responses_input(None, None, None, None, &few, None, &[], &obs);
  assert_eq!(items.len(), 1);
  assert_eq!(role_of(&items[0]), Some("developer"));
  assert_eq!(input_item_text(&items[0]).unwrap(), "dev note");
 }

 #[test]
 fn instruction_role_for_model_selects_developer_for_gpt5() {
  assert_eq!(instruction_role_for_model(Some("gpt-5")), "developer");
  assert_eq!(instruction_role_for_model(Some("gpt-5-mini")), "developer");
  assert_eq!(instruction_role_for_model(Some("gpt-4o")), "system");
  assert_eq!(instruction_role_for_model(None), "system");
 }

 #[test]
 fn auxiliary_channels_label_uses_source_actor_when_present() {
  use crate::state::DataSource;
  let d = ChannelDatum::new("twitch".to_string(), "[Twitch] ギフトサブ: alice が 5 件 (Tier1)".to_string())
   .with_flag(ChannelDatum::FLAG_IS_FINAL)
   .with_source(DataSource::new("twitch.eventsub").with_subtype("channel.subscription.gift").with_actor("alice"));
  let obs = ObserveSet::from_conf(
   "to_ch",
   &ObservePolicyConf {
    triggers: vec!["from_ch".to_string()],
    include_additional: vec!["twitch".to_string()],
    ..Default::default()
   },
  );
  let msgs = memory_datums_to_chat_messages(&[d], &obs);
  match &msgs[0] {
   ChatCompletionRequestMessage::System(s) => match &s.content {
    ChatCompletionRequestSystemMessageContent::Text(t) => {
     assert!(t.contains("[alice@twitch]"), "got: {}", t);
    },
    _ => panic!("unexpected system content"),
   },
   _ => panic!("expected system"),
  }
 }
}
