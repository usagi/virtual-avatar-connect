mod fine_tuning;

use super::{CompletedAnd, Processor};
use crate::{Arc, ChannelDatum, Mutex, ProcessorConf, ProcessorKind, SharedChannelData, SharedProcessorConf, SharedState};

use anyhow::{bail, Context, Result};
use async_openai::{
 config::OpenAIConfig,
 types::{
  ChatCompletionRequestAssistantMessageArgs, ChatCompletionRequestMessage, ChatCompletionRequestSystemMessageArgs,
  ChatCompletionRequestUserMessageArgs, CreateChatCompletionRequest, CreateChatCompletionRequestArgs, ResponseFormat,
 },
 Client,
};
use async_trait::async_trait;
use regex::Regex;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct OpenAiChat {
 conf: SharedProcessorConf,
 state: SharedState,
 channel_data: SharedChannelData,
 client: Client<OpenAIConfig>,
 request_template: CreateChatCompletionRequest,
 last_activated: Arc<Mutex<SystemTime>>,
 force_activate_regex: Option<Regex>,
 ignore_regex: Option<Regex>,
}

const ENV_OPENAI_API_KEY: &str = "VAC_OPENAI_API_KEY";
const DEFAULT_MEMORY_CAPACITY: usize = 4;
const DEFAULT_REMOVE_CHARS: &str = "\n\r\t";

#[async_trait]
impl Processor for OpenAiChat {
 const FEATURE: &'static str = "openai-chat";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("OpenAIChat::process() が呼び出されました。");

  let conf = self.conf.read().await;

  let last_activated = self.last_activated.clone();
  let state = self.state.clone();
  let channel_data = self.channel_data.clone();
  let request_template = self.request_template.clone();
  let memory_capacity = conf.memory_capacity.unwrap_or(DEFAULT_MEMORY_CAPACITY);
  let force_activate_regex = self.force_activate_regex.clone();
  let ignore_regex = self.ignore_regex.clone();
  let min_interval_in_secs = conf.min_interval_in_secs;
  let channel_from = conf.channel_from.as_ref().unwrap().clone();
  let channel_to = conf.channel_to.as_ref().unwrap().clone();
  let client = self.client.clone();
  let custom_instructions = conf.custom_instructions.as_ref().cloned();
  let model_for_runtime = conf.model.clone();
  // 再試行時に元の max_tokens (gpt-4 系) または max_completion_tokens (gpt-5 系) として利用するために値だけ先に取り出しておく
  let orig_max_tokens = conf.max_tokens;
  let remove_chars = conf
   .remove_chars
   .as_ref()
   .cloned()
   .unwrap_or_else(|| DEFAULT_REMOVE_CHARS.to_string());
  let fine_tuning = conf.fine_tuning.as_ref().cloned();

  tokio::spawn(async move {
   // 入力を取得
   let reversed_sources = {
    let channel_data = channel_data.read().await;
    // channel_data から id の ChannelDatum の位置を取得
    let index = channel_data.iter().rev().position(|cd| cd.get_id() == id);
    if index.is_none() {
     log::error!("処理対象の入力は既にありませんでした。必要に応じて設定の state_data_capacity を増やすと解決するかもしれません。");
     return Ok(());
    }
    let index = index.unwrap();

    // index の ChannelDatum が .has_flag で IS_FINAL フラグか確認
    if !channel_data.iter().rev().nth(index).unwrap().has_flag(ChannelDatum::FLAG_IS_FINAL) {
     log::trace!("未確定の入力なので、処理をスキップします。");
     return Ok(());
    }

    // index の ChannelDatum から memory_capacity 件の IS_FINAL フラグが有効かつ channel が channel_from または channel_to の ChannelDatum を取得
    channel_data
     .iter()
     .rev()
     .skip(index)
     .filter(|cd| cd.has_flag(ChannelDatum::FLAG_IS_FINAL))
     .filter(|cd| cd.channel == channel_from || cd.channel == channel_to)
     .take(memory_capacity)
     .cloned()
     .collect::<Vec<_>>()
   };

   // トリガーになった id の user 発言を保持(fine-tuning 用)
   let latest_user_content = reversed_sources
    .iter()
    .find(|cd| cd.channel == channel_from)
    .map(|v| v.content.clone())
    .unwrap();

   log::trace!("reversed_sources = {:?}", reversed_sources);

   // 強制無視の正規表現による判定 => match したら確定で無視
   // (強制無視判定は強制応答判定よりも優先される)
   if let Some(ignore_regex) = ignore_regex {
    log::trace!(
     "強制無視の正規表現が設定されています: {:?} match to {:?}",
     ignore_regex,
     reversed_sources
    );
    let target_content = &reversed_sources.iter().next().unwrap().content.trim();
    let ignore = ignore_regex.is_match(target_content);
    if ignore {
     log::debug!(
      "強制無視の正規表現にマッチしたので処理をスキップします。 ignore_regex: {:?} target_content: {:?}",
      ignore_regex,
      target_content
     );
     return Ok(());
    }
   }

   // 強制応答の判定
   if let Some(force_activate_regex) = force_activate_regex {
    log::trace!(
     "強制応答が設定されています: {:?} match to {:?}",
     force_activate_regex,
     reversed_sources
    );
    let target_content = &reversed_sources.iter().next().unwrap().content.trim();
    let force_activate = force_activate_regex.is_match(target_content);
    log::debug!("強制応答の判定結果: {:?} target_content: {:?}", force_activate, target_content);

    // 強制応答ではない場合 => 応答時間による判定
    if !force_activate {
     // 応答時間による判定
     if let Some(min_interval_in_secs) = min_interval_in_secs {
      match last_activated.lock().await.elapsed() {
       Ok(duration) => {
        if duration.as_secs() < min_interval_in_secs {
         log::trace!("応答間隔が短いので処理をスキップします。");
         return Ok(());
        }
       },
       Err(e) => {
        log::error!("応答間隔の判定に失敗しました: {:?}", e);
        bail!("{e:?}");
       },
      }
      *last_activated.lock().await = SystemTime::now();
     }
    }
   }

   // リクエストを生成
  let mut request = request_template;
  // gpt-5 系モデルで system メッセージが存在しない場合はデフォルトの system 指示を追加
  if model_for_runtime.as_ref().map(|m| m.starts_with("gpt-5")).unwrap_or(false) {
   let has_system = request.messages.iter().any(|m| matches!(m, ChatCompletionRequestMessage::System(_)));
   if !has_system {
    if let Some(sys_msg) = ChatCompletionRequestSystemMessageArgs::default()
    .content("You are a helpful assistant. Provide a concise, direct response to the user. 日本語入力には日本語で返答して下さい。")
    .build()
    .ok()
    .map(ChatCompletionRequestMessage::System)
    { request.messages.insert(0, sys_msg); }
   }
  }
   // api_key が表示される可能性があるためソースレベルで一時的な変更を行わない限り request の内容は出力しないよう変更
   // log::trace!("ai req: {:?}", request);
   request.messages.extend(reversed_sources.into_iter().rev().filter_map(|cd| {
    match cd.channel.as_str() {
     channel if channel == &channel_from => ChatCompletionRequestUserMessageArgs::default()
      .content(cd.content.clone())
      .build()
      .ok()
      .map(ChatCompletionRequestMessage::User),
     channel if channel == &channel_to => ChatCompletionRequestAssistantMessageArgs::default()
      .content(cd.content.clone())
      .build()
      .ok()
      .map(ChatCompletionRequestMessage::Assistant),
     _ => ChatCompletionRequestSystemMessageArgs::default()
      .content(cd.content.clone())
      .build()
      .ok()
      .map(ChatCompletionRequestMessage::System),
    }
   }));

   // OpenAIChat に応答をリクエスト
   // api_key が表示される可能性があるためソースレベルで一時的な変更を行わない限り request の内容は出力しないよう変更
   // log::trace!("request = {:?}", request);
   log::debug!("OpenAIChat に応答をリクエストします。");
  let response = match client.chat().create(request.clone()).await {
    Ok(response) => response,
    Err(e) => {
     log::error!("OpenAIChat へのリクエストに失敗しました: {:?}", e);
     let es = e.to_string().to_lowercase();
     if es.contains("billing") || es.contains("quota") || es.contains("limit") || es.contains("exceeded") {
      static MSG: &str = r#"
=================================================================
=================================================================
 OpenAIChat へのリクエストの失敗理由に
  Billing Exceeded Limit Quota
 などのキーワードが含まれています。使用状況やプランを確認して下さい。
 慌てず落ち着いて Usage ページを確認して計画的に人生を楽しみましょう。
 Usage: https://platform.openai.com/account/usage
=================================================================
=================================================================
"#;
      eprint!("{}", MSG);
     }
     bail!("{e:?}");
    },
   };
   log::trace!("response = {:?}", response);

  let mut content = {
    let mut content = response
     .choices
     .first()
     .context("AI からの応答はありましたが回答がありませんでした。")?
     .message
     .content
     .clone()
     .context("AI からの応答はありましたが無言の回答でした。")?;

    // apply remove_chars
    for remove_char in remove_chars.chars() {
     content = content.replace(remove_char, "");
    }
  content
   };

   // gpt-5 系モデルで空文字応答だった場合の再試行ロジック
   if model_for_runtime.as_ref().map(|m| m.starts_with("gpt-5")).unwrap_or(false)
  && content.trim().is_empty()
   {
  log::warn!("gpt-5 系モデルから空の content が返却されました。再試行を行います。(finish_reason = {:?})", response.choices.first().and_then(|c| c.finish_reason.clone()));
  // 再試行用の簡易リクエストを構築（最新ユーザー入力のみ + system 指示）
  if let Some(latest_user) = Some(latest_user_content.clone()) {
   let mut retry_builder = CreateChatCompletionRequestArgs::default();
   if let Some(model) = model_for_runtime.clone() { retry_builder.model(model); }
   // max_completion_tokens を控えめに（元設定より小さく）
  if let Some(orig_max) = orig_max_tokens {
    let reduced = std::cmp::min(orig_max, 128); // 128 以内に制限
    if model_for_runtime.as_ref().map(|m| m.starts_with("gpt-5")).unwrap_or(false) {
     retry_builder.max_completion_tokens(reduced);
    } else {
     retry_builder.max_tokens(reduced);
    }
   }
   // system
   if let Some(sys) = ChatCompletionRequestSystemMessageArgs::default()
    .content("You are a helpful assistant. Provide a concise answer. If prior reasoning used all tokens, output a brief final answer now.")
    .build()
    .ok()
    .map(ChatCompletionRequestMessage::System) { retry_builder.messages(vec![sys]); }
   // user
  if let Some(user_msg) = ChatCompletionRequestUserMessageArgs::default()
   .content(latest_user)
   .build()
   .ok()
   .map(ChatCompletionRequestMessage::User)
  {
   // builder doesn't expose current messages publicly; rebuild from scratch
   let mut msgs: Vec<ChatCompletionRequestMessage> = Vec::new();
   if let Some(sys) = ChatCompletionRequestSystemMessageArgs::default()
    .content("You are a helpful assistant. Provide a concise answer.")
    .build()
    .ok()
    .map(ChatCompletionRequestMessage::System) { msgs.push(sys); }
   msgs.push(user_msg);
   retry_builder.messages(msgs);
  }
  // force text response format for gpt-5 retry as well
  if model_for_runtime.as_ref().map(|m| m.starts_with("gpt-5")).unwrap_or(false) {
   retry_builder.response_format(async_openai::types::ResponseFormat::Text);
  }
  match retry_builder.build() {
    Ok(retry_req) => {
     match client.chat().create(retry_req.clone()).await {
    Ok(retry_res) => {
     log::debug!("gpt-5 系モデル再試行 response = {:?}", retry_res);
     if let Some(retry_choice) = retry_res.choices.first() {
      if let Some(retry_content) = retry_choice.message.content.clone() {
       if !retry_content.trim().is_empty() { content = retry_content; }
      }
     }
    },
    Err(e) => {
     log::warn!("gpt-5 系モデルの再試行に失敗しました: {:?}", e);
    },
     }
    },
    Err(e) => log::warn!("gpt-5 系モデル再試行用リクエストの構築に失敗しました: {:?}", e),
   }
  }
   }

   if let Some(fine_tuning) = fine_tuning {
    use serde::Serialize;
    use tokio::io::AsyncWriteExt;

    #[derive(Serialize)]
    struct Line {
     messages: Vec<Message>,
    }
    #[derive(Serialize)]
    struct Message {
     role: String,
     content: String,
    }

    let train_path = fine_tuning.train_path();

    if train_path.to_lowercase().ends_with(".csv") {
     let f = tokio::fs::OpenOptions::new().create(true).append(true).open(&train_path).await?;
     let is_new_file = f.metadata().await?.len() == 0;
     let mut w = csv_async::AsyncWriter::from_writer(f);
     if is_new_file {
      w.write_record(vec!["user", "assistant"]).await?;
     }
     w.write_record(vec![latest_user_content, content.clone()]).await?;
    } else {
     let mut line = Line { messages: vec![] };
     if let Some(custom_instruction) = custom_instructions {
      line.messages.push(Message {
       role: "system".to_string(),
       content: custom_instruction,
      });
     }
     line.messages.push(Message {
      role: "user".to_string(),
      content: latest_user_content,
     });
     line.messages.push(Message {
      role: "assistant".to_string(),
      content: content.clone(),
     });
     let line = serde_json::to_string(&line)?.replace("\n", "");
     // train_path のファイルへ line を追記
     tokio::fs::OpenOptions::new()
      .create(true)
      .append(true)
      .open(&train_path)
      .await?
      .write_all(format!("{}\n", line).as_bytes())
      .await?;
    };

    log::trace!("fine-tuning 用のファイルに追記しました: {:?}", train_path);
   }

   let datum = ChannelDatum::new(channel_to, content)
    .with_flag(ChannelDatum::FLAG_IS_FINAL)
    .with_flag(&format!("{}({}:{})", Self::FEATURE, channel_from, id));

   {
    let state = state.read().await;
    state.push_channel_datum(datum.clone()).await;
   }

   Ok(())
  });

  log::trace!("OpenAIChat::process() は非同期処理を開始しました。");

  Ok(CompletedAnd::Next)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = OpenAiChat {
   conf: pc.as_shared(),
   state: state.clone(),
   channel_data: state.read().await.channel_data.clone(),
   client: make_client(pc)?,
   request_template: make_request_template(pc)?,
   last_activated: Arc::new(Mutex::new(SystemTime::UNIX_EPOCH)),
   force_activate_regex: None,
   ignore_regex: None,
  };

  if !p.is_established().await {
   bail!("OpenAIChat が正常に設定されていません: {:?}", pc);
  }

  {
   let conf = p.conf.read().await;

   p.force_activate_regex = conf.force_activate_regex_pattern.as_ref().map(|s| Regex::new(s).unwrap());
   p.ignore_regex = conf.ignore_regex_pattern.as_ref().map(|s| Regex::new(s).unwrap());
  }

  Ok(ProcessorKind::OpenAiChat(p))
 }

 async fn is_channel_from(&self, channel_from: &str) -> bool {
  let conf = self.conf.read().await;
  conf.channel_from.as_ref().unwrap() == channel_from
 }

 async fn is_established(&mut self) -> bool {
  let conf = self.conf.read().await;

  if conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }
  if conf.channel_to.is_none() {
   log::error!("channel_to が設定されていません。");
   return false;
  }

  if conf.api_key.is_some() {
   log::warn!("================================================================");
   log::warn!("api_key が設定ファイルで直接設定されています。設定ファイルを共有したり一般に公開する際は不慮の漏出に十分に注意して下さい。または環境変数 {} での設定も検討して下さい。", ENV_OPENAI_API_KEY);
   log::warn!("================================================================");
  }

  log::info!(
   "OpenAIChat は正常に設定されています: channel_from: {:?} channel_to: {:?}",
   conf.channel_from,
   conf.channel_to
  );
  true
 }
}

fn make_client(conf: &ProcessorConf) -> Result<Client<OpenAIConfig>> {
 // 環境変数から読めたら読む、読めなかったら conf から読む
 let api_key = crate::utility::load_from_env_or_conf(ENV_OPENAI_API_KEY, &conf.api_key);
 if let Some(api_key) = api_key {
  Ok(Client::with_config(OpenAIConfig::default().with_api_key(api_key)))
 } else {
  bail!("OpenAI の API KEY が設定されていません。環境変数 VAC_OPENAI_API_KEY を設定するか、設定ファイルに api_key を設定して下さい。");
 }
}

fn make_request_template(conf: &ProcessorConf) -> Result<CreateChatCompletionRequest> {
 let mut builder = CreateChatCompletionRequestArgs::default();

 if let Some(model) = conf.model.as_ref() {
  builder.model(model.clone());
  if model.starts_with("gpt-5") {
   // Workaround: force plain text output format to avoid empty content responses.
   builder.response_format(ResponseFormat::Text);
  }
  if let Some(max_tokens) = conf.max_tokens {
   if model.starts_with("gpt-5") {
    builder.max_completion_tokens(max_tokens);
   } else {
    builder.max_tokens(max_tokens);
   }
  }
 }
 if let Some(temperature) = conf.temperature {
  builder.temperature(temperature);
 }
 if let Some(top_p) = conf.top_p {
  builder.top_p(top_p);
 }
 if let Some(n) = conf.n {
  builder.n(n);
 }
 if let Some(presence_penalty) = conf.presence_penalty {
  builder.presence_penalty(presence_penalty);
 }
 if let Some(frequency_penalty) = conf.frequency_penalty {
  builder.frequency_penalty(frequency_penalty);
 }
 if let Some(user) = conf.user.as_ref() {
  builder.user(user.clone());
 }

 if let Some(custom_instructions) = conf.custom_instructions.as_ref() {
  builder.messages(
   vec![ChatCompletionRequestSystemMessageArgs::default()
    .content(custom_instructions.clone())
    .build()
    .ok()]
   .into_iter()
   .filter_map(|msg| msg.map(ChatCompletionRequestMessage::System))
   .collect::<Vec<_>>(),
  );
 }

 Ok(builder.build()?)
}
