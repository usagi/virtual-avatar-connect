mod bouyomichan;
mod coeiroink;
mod gas_translation;
mod openai_chat;
mod os_tts;

pub use bouyomichan::Bouyomichan;
pub use coeiroink::CoeiroInk;
pub use gas_translation::GasTranslation;
pub use openai_chat::OpenAiChat;
pub use os_tts::OsTts;

use crate::{ProcessorConf, SharedState};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Processor {
 /// Processor の種類を表す他の Processor と重複しない一意の文字列。
 const FEATURE: &'static str;
 /// 処理本体
 async fn process(&self, id: u64) -> Result<()>;
 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind>;
 async fn is_established(&mut self) -> bool;
 fn is_channel_from(&self, channel_from: &str) -> bool;
}

// Note: 追加/削除する場合は
//  1. ProcessorKind
//  2. ProcessorKind::is_channel_from
//  3. ProcessorKind::process
//  4. State::init_processors
// も変更すること
#[derive(Debug, Clone)]
pub enum ProcessorKind {
 // ai
 OpenAiChat(OpenAiChat),

 // translation
 GasTranslation(GasTranslation),

 // tts
 OsTts(OsTts),
 CoeiroInk(CoeiroInk),
 Bouyomichan(Bouyomichan),
}

impl ProcessorKind {
 pub fn is_channel_from(&self, channel_from: &str) -> bool {
  match self {
   Self::OpenAiChat(p) => p.is_channel_from(channel_from),
   Self::GasTranslation(p) => p.is_channel_from(channel_from),
   Self::Bouyomichan(p) => p.is_channel_from(channel_from),
   Self::CoeiroInk(p) => p.is_channel_from(channel_from),
   Self::OsTts(p) => p.is_channel_from(channel_from),
  }
 }

 pub async fn process(&self, id: u64) -> Result<()> {
  log::debug!("ProcessorKind::process() が呼び出されました。");
  match self {
   Self::OpenAiChat(p) => p.process(id).await,
   Self::GasTranslation(p) => p.process(id).await,
   Self::CoeiroInk(p) => p.process(id).await,
   Self::Bouyomichan(p) => p.process(id).await,
   Self::OsTts(p) => p.process(id).await,
  }
 }
}
