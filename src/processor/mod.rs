mod bouyomichan;
mod coeiroink;
mod command;
mod gas_translation;
mod modify;
mod ocr;
mod openai_chat;
mod os_tts;
mod screenshot;

pub use bouyomichan::Bouyomichan;
pub use coeiroink::CoeiroInk;
pub use command::Command;
pub use gas_translation::GasTranslation;
pub use modify::Modify;
pub use ocr::Ocr;
pub use openai_chat::OpenAiChat;
pub use os_tts::OsTts;
pub use screenshot::Screenshot;

use crate::{ProcessorConf, SharedProcessorConf, SharedState};
use anyhow::Result;
use async_trait::async_trait;

#[async_trait]
pub trait Processor {
 /// Processor の種類を表す他の Processor と重複しない一意の文字列。
 const FEATURE: &'static str;
 /// 処理本体
 async fn process(&self, id: u64) -> Result<CompletedAnd>;
 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind>;
 async fn is_established(&mut self) -> bool;
 async fn is_channel_from(&self, channel_from: &str) -> bool;
 fn conf(&self) -> SharedProcessorConf;
}

// Note: 追加/削除する場合は
//  1. ProcessorKind
//  2. ProcessorKind::is_channel_from
//  3. ProcessorKind::process
//  4. State::init_processors
// も変更すること
#[derive(Debug, Clone)]
pub enum ProcessorKind {
 // common
 Command(Command),
 Modify(Modify),
 Screenshot(Screenshot),
 Ocr(Ocr),

 // ai
 OpenAiChat(OpenAiChat),

 // translation
 GasTranslation(GasTranslation),

 // tts
 OsTts(OsTts),
 CoeiroInk(CoeiroInk),
 Bouyomichan(Bouyomichan),
}

#[derive(Debug, Clone, PartialEq, Copy)]
pub enum CompletedAnd {
 Next,
 Break,
}

impl ProcessorKind {
 pub async fn is_channel_from(&self, channel_from: &str) -> bool {
  match self {
   Self::Command(p) => p.is_channel_from(channel_from),
   Self::Modify(p) => p.is_channel_from(channel_from),
   Self::Screenshot(p) => p.is_channel_from(channel_from),
   Self::Ocr(p) => p.is_channel_from(channel_from),
   Self::OpenAiChat(p) => p.is_channel_from(channel_from),
   Self::GasTranslation(p) => p.is_channel_from(channel_from),
   Self::Bouyomichan(p) => p.is_channel_from(channel_from),
   Self::CoeiroInk(p) => p.is_channel_from(channel_from),
   Self::OsTts(p) => p.is_channel_from(channel_from),
  }.await
 }

 pub async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("ProcessorKind::process() が呼び出されました。");
  match self {
   Self::Command(p) => p.process(id).await,
   Self::Modify(p) => p.process(id).await,
   Self::Screenshot(p) => p.process(id).await,
   Self::Ocr(p) => p.process(id).await,
   Self::OpenAiChat(p) => p.process(id).await,
   Self::GasTranslation(p) => p.process(id).await,
   Self::CoeiroInk(p) => p.process(id).await,
   Self::Bouyomichan(p) => p.process(id).await,
   Self::OsTts(p) => p.process(id).await,
  }
 }
}
