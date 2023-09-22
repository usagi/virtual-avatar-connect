use super::Processor;
use crate::{Arc, ChannelDatum, Mutex, ProcessorConf, ProcessorKind, SharedChannelData, SharedState};
use anyhow::{anyhow, bail, Context, Result};
use async_openai::{
 config::OpenAIConfig,
 types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequest, CreateChatCompletionRequestArgs, Role},
 Client,
};
use async_trait::async_trait;
use regex::Regex;
use std::time::SystemTime;

#[derive(Debug, Clone)]
pub struct OpenAiChat {
 conf: ProcessorConf,
 state: SharedState,
 channel_data: SharedChannelData,
 client: Client<OpenAIConfig>,
 request_template: CreateChatCompletionRequest,
 last_activated: Arc<Mutex<SystemTime>>,
 force_activate_regex: Option<Regex>,
 ignore_regex: Option<Regex>,
}

const DEFAULT_MEMORY_CAPACITY: usize = 4;

#[async_trait]
impl Processor for OpenAiChat {
 const FEATURE: &'static str = "openai-chat";

 async fn process(&self, id: u64) -> Result<()> {
  log::debug!("OpenAIChat::process() が呼び出されました。");

  let last_activated = self.last_activated.clone();
  let state = self.state.clone();
  let channel_data = self.channel_data.clone();
  let request_template = self.request_template.clone();
  let memory_capacity = self.conf.memory_capacity.unwrap_or(DEFAULT_MEMORY_CAPACITY);
  let force_activate_regex = self.force_activate_regex.clone();
  let ignore_regex = self.ignore_regex.clone();
  let min_interval_in_secs = self.conf.min_interval_in_secs;
  let channel_from = self.conf.channel_from.as_ref().unwrap().clone();
  let channel_to = self.conf.channel_to.as_ref().unwrap().clone();
  let client = self.client.clone();

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

   // log::error!("reversed_sources = {:?}", reversed_sources);
   // log::debug!("debug end");
   // return Ok(());

   // 強制無視の正規表現による判定 => match したら確定で無視
   // (強制無視判定は強制応答判定よりも優先される)
   if let Some(ignore_regex) = ignore_regex {
    let target_content = &reversed_sources.iter().next().unwrap().content;
    let ignore = ignore_regex.is_match(target_content);
    if ignore {
     log::debug!("強制無視の正規表現にマッチしたので処理をスキップします。");
     return Ok(());
    }
   }

   // 強制応答の判定
   if let Some(force_activate_regex) = force_activate_regex {
    let target_content = &reversed_sources.iter().next().unwrap().content;
    let force_activate = force_activate_regex.is_match(target_content);

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
     }
    }
   }

   // リクエストを生成
   let mut request = request_template;
   request.messages.extend(reversed_sources.into_iter().rev().filter_map(|cd| {
    ChatCompletionRequestMessageArgs::default()
     .role(match cd.channel.as_str() {
      channel if channel == &channel_from => Role::User,
      channel if channel == &channel_to => Role::Assistant,
      _ => Role::System,
     })
     .content(cd.content.clone())
     .build()
     .ok()
   }));

   // OpenAIChat に応答をリクエスト
   log::trace!("request = {:?}", request);
   log::debug!("OpenAIChat に応答をリクエストします。");
   let response = client.chat().create(request).await.map_err(|e| anyhow!("{e:?}"))?;
   log::trace!("response = {:?}", response);

   let content = response
    .choices
    .first()
    .context("AI からの応答はありましたが回答がありませんでした。")?
    .message
    .content
    .clone()
    .context("AI からの応答はありましたが無言の回答でした。")?;

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

  Ok(())
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = OpenAiChat {
   conf: pc.clone(),
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

  p.force_activate_regex = p.conf.force_activate_regex_pattern.as_ref().map(|s| Regex::new(s).unwrap());
  p.ignore_regex = p.conf.ignore_regex_pattern.as_ref().map(|s| Regex::new(s).unwrap());

  Ok(ProcessorKind::OpenAiChat(p))
 }

 fn is_channel_from(&self, channel_from: &str) -> bool {
  self.conf.channel_from.as_ref().unwrap() == channel_from
 }

 async fn is_established(&mut self) -> bool {
  if self.conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }
  if self.conf.channel_to.is_none() {
   log::error!("channel_to が設定されていません。");
   return false;
  }
  if self.conf.api_key.is_none() {
   log::error!("api_key が設定されていません。");
   return false;
  }

  log::info!(
   "OpenAIChat は正常に設定されています: channel_from: {:?} channel_to: {:?}",
   self.conf.channel_from,
   self.conf.channel_to
  );
  true
 }
}

fn make_client(conf: &ProcessorConf) -> Result<Client<OpenAIConfig>> {
 let client = match conf.api_key.as_ref() {
  Some(api_key) => Client::with_config(OpenAIConfig::default().with_api_key(api_key.clone())),
  _ => Client::new(),
 };
 Ok(client)
}

fn make_request_template(conf: &ProcessorConf) -> Result<CreateChatCompletionRequest> {
 let mut builder = CreateChatCompletionRequestArgs::default();

 if let Some(model) = conf.model.as_ref() {
  builder.model(model.clone());
 }
 if let Some(max_tokens) = conf.max_tokens {
  builder.max_tokens(max_tokens);
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
  builder.messages(vec![ChatCompletionRequestMessageArgs::default()
   .role(Role::System)
   .content(custom_instructions.clone())
   .build()?]);
 }

 Ok(builder.build()?)
}
