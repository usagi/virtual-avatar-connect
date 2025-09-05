use crate::{Result, SharedConf, SharedState};
use std::sync::{Arc, RwLock};

use anyhow::Context;
use async_openai::{
 config::OpenAIConfig,
 types::{ChatCompletionRequestMessageArgs, CreateChatCompletionRequest, CreateChatCompletionRequestArgs, Role},
 Client,
};
use serde::{Deserialize, Serialize};

const DEFAULT_INTERACTION_RATE: f64 = 1.0;
const DEFAULT_AI_MEMORY_CAPACITY: usize = 4;

#[derive(Debug)]
pub struct Ai {
 client: Client<OpenAIConfig>,
 request_template: CreateChatCompletionRequest,
 memory_capacity: usize,
 intereraction_rate: f64,
}

pub type SharedAi = Arc<RwLock<Ai>>;

#[derive(Debug, Deserialize, Serialize)]
pub struct AiResponse {
 pub message: String,
 pub emotes: Vec<String>,
 pub posings: Vec<String>,
}

impl AiResponse {
 pub fn new(message: String, emotes: Vec<String>, posings: Vec<String>) -> Self {
  Self { message, emotes, posings }
 }
 pub fn new_only_message(message: String) -> Self {
  Self {
   message,
   emotes: vec![],
   posings: vec![],
  }
 }
 pub fn new_ignored() -> Self {
  Self {
   message: "".to_string(),
   emotes: vec![],
   posings: vec![],
  }
 }
}

impl Ai {
 pub fn from_shared_conf(conf: &SharedConf) -> Result<SharedAi> {
  log::info!("AI を生成します。");
  let s = Self {
   client: make_client(conf)?,
   request_template: make_request_template(conf)?,
   memory_capacity: make_memory_capacity(conf)?,
   intereraction_rate: make_interaction_rate(conf)?,
  };
  log::trace!("AI を生成しました: {:?}", s);
  Ok(Arc::new(RwLock::new(s)))
 }

 pub async fn talk(&self, user_input: &str, state: &SharedState) -> Result<AiResponse> {
  log::trace!("AI talk: user_input: {:?}", user_input);

  if rand::random::<f64>() > self.intereraction_rate {
   log::debug!("AI talk: 応答率の判定により AI はユーザー入力を無視しました。");
   return Ok(AiResponse::new_ignored());
  }
  log::debug!("AI talk: 応答率の判定により AI はユーザー入力への応答を試みます。");

  let mut request = self.request_template.clone();
  if let Ok(state) = state.read() {
   let start_index = state.messages.len().saturating_sub(self.memory_capacity);
   request.messages.extend(state.messages.iter().skip(start_index).map(|message| {
    ChatCompletionRequestMessageArgs::default()
     .role(match message.from.as_str() {
      "user" => Role::User,
      "ai" => Role::Assistant,
      _ => Role::System,
     })
     .content(message.content.clone())
     .build()
     .expect("既存のログからの AI のリクエストの作成に失敗しました。")
   }));
  }
  request.messages.push(
   ChatCompletionRequestMessageArgs::default()
    .role(Role::User)
    .content(user_input.to_string())
    .build()
    .expect("ユーザー入力からの AI のリクエストの作成に失敗しました。"),
  );
  log::trace!("AI talk: request: {:?}", request);

  let response = self.client.chat().create(request).await?;
  log::trace!("AI talk: response: {:?}", response);

  let choice = response
   .choices
   .first()
   .context("AI からの応答はありましたが回答がありませんでした。")?;
  let ai_output = choice
   .message
   .content
   .clone()
   .context("AI からの応答を取得しましたが内容がありませんでした。")?;

  // TODO: emotes, posings の抽出
  Ok(AiResponse::new_only_message(ai_output))
 }
}

fn make_client(conf: &SharedConf) -> Result<Client<OpenAIConfig>> {
 let conf = conf.read().map_err(|e| anyhow::anyhow!("{:?}", e))?;
 let client = match conf.ai.api_key.as_ref() {
  Some(api_key) => Client::with_config(OpenAIConfig::default().with_api_key(api_key.clone())),
  _ => Client::new(),
 };
 Ok(client)
}

fn make_request_template(conf: &SharedConf) -> Result<CreateChatCompletionRequest> {
 let conf = conf.read().await;
 let mut builder = CreateChatCompletionRequestArgs::default();

 if let Some(model) = conf.ai.model.as_ref() {
  builder.model(model.clone());
  if let Some(max_tokens) = conf.ai.max_tokens {
   if model.starts_with("gpt-5") {
    builder.max_completion_tokens(max_tokens);
   } else {
    builder.max_tokens(max_tokens);
   }
  }
 }
 if let Some(temperature) = conf.ai.temperature {
  builder.temperature(temperature);
 }
 if let Some(top_p) = conf.ai.top_p {
  builder.top_p(top_p);
 }
 if let Some(n) = conf.ai.n {
  builder.n(n);
 }
 if let Some(presence_penalty) = conf.ai.presence_penalty {
  builder.presence_penalty(presence_penalty);
 }
 if let Some(frequency_penalty) = conf.ai.frequency_penalty {
  builder.frequency_penalty(frequency_penalty);
 }
 if let Some(user) = conf.ai.user.as_ref() {
  builder.user(user.clone());
 }
 if let Some(custom_instructions) = conf.ai.custom_instructions.as_ref() {
  builder.messages(vec![ChatCompletionRequestMessageArgs::default()
   .role(Role::System)
   .content(custom_instructions.clone())
   .build()
   .expect(
    "AI のカスタム設定に失敗しました。設定の [ai] の custom_instructions 関連の項目を見直して下さい。",
   )]);
 }

 Ok(builder.build()?)
}

fn make_memory_capacity(conf: &SharedConf) -> Result<usize> {
 let conf = conf.read().await;
 let memory_capacity = conf.ai.memory_capacity.unwrap_or(DEFAULT_AI_MEMORY_CAPACITY);
 Ok(memory_capacity)
}

fn make_interaction_rate(conf: &SharedConf) -> Result<f64> {
 let conf = conf.read().await;
 let interaction_rate = conf.ai.interaction_rate.unwrap_or(DEFAULT_INTERACTION_RATE);
 Ok(interaction_rate)
}
