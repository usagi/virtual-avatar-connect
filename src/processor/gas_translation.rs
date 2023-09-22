use super::Processor;
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct GasTranslation {
 conf: ProcessorConf,
 state: SharedState,
 channel_data: SharedChannelData,
 url_base: String,
}

const GOOGLE_APPS_SCRIPT_URL_TEMPLATE: &str =
 "https://script.google.com/macros/s/{script_id}/exec?trans_sourcelang={translate_from}&target={translate_to}&text=";

#[async_trait]
impl Processor for GasTranslation {
 const FEATURE: &'static str = "gas-translation";

 async fn process(&self, id: u64) -> Result<()> {
  log::debug!("GasTranslation::process() が呼び出されました。");

  let state = self.state.clone();
  let channel_data = self.channel_data.clone();
  let url_base = self.url_base.clone();
  let channel_to = self.conf.channel_to.as_ref().unwrap().clone();

  tokio::spawn(async move {
   // 翻訳元を取得
   let source = {
    let channel_data = channel_data.read().await;
    match channel_data.iter().rev().find(|cd| cd.get_id() == id) {
     Some(source) if source.has_flag(ChannelDatum::FLAG_IS_FINAL) => source.content.clone(),
     Some(_) => {
      log::trace!("未確定の入力なので、処理をスキップします。");
      return Ok(());
     },
     None => bail!("指定された id の ChannelDatum が見つかりませんでした: {}", id),
    }
   };
   // 翻訳API投げ
   let url = format!("{}{}", url_base, urlencoding::encode(&source));
   log::debug!("url = {}", url);
   let response = reqwest::get(&url).await?;
   log::trace!("response = {:?}", response);
   let output_content = response.text().await?;
   log::debug!("output_content = {}", output_content);
   // 翻訳結果を書き込み
   let output_channel_datum = ChannelDatum::new(channel_to, output_content)
    .with_flag(ChannelDatum::FLAG_IS_FINAL)
    .with_flag(&format!("{}({}:{},{}/{})", Self::FEATURE, "gas-translation", id, "ja", "en"));
   log::debug!("output_channel_datum = {:?}", output_channel_datum);

   {
    let state = state.read().await;
    state.push_channel_datum(output_channel_datum).await;
   }

   Ok(())
  });

  Ok(())
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = GasTranslation {
   conf: pc.clone(),
   state: state.clone(),
   channel_data: state.read().await.channel_data.clone(),
   url_base: String::new(),
  };
  if !p.is_established().await {
   bail!("GasTranslation が正常に設定されていません: {:?}", pc);
  }

  p.url_base = GOOGLE_APPS_SCRIPT_URL_TEMPLATE
   .replace("{script_id}", p.conf.script_id.as_ref().unwrap())
   .replace("{translate_from}", p.conf.translate_from.as_ref().unwrap())
   .replace("{translate_to}", p.conf.translate_to.as_ref().unwrap());

  Ok(ProcessorKind::GasTranslation(p))
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
  if self.conf.script_id.is_none() {
   log::error!("script_id が設定されていません。");
   return false;
  }
  if self.conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }
  if self.conf.channel_to.is_none() {
   log::error!("channel_to が設定されていません。");
   return false;
  }
  if self.conf.translate_from.is_none() {
   log::error!("translation_from が設定されていません。");
   return false;
  }
  if self.conf.translate_to.is_none() {
   log::error!("translation_to が設定されていません。");
   return false;
  }
  log::info!(
   "GasTranslation は正常に設定されています: channel: {:?} -> {:?} lang: {:?} -> {:?}",
   self.conf.channel_from,
   self.conf.channel_to,
   self.conf.translate_from,
   self.conf.translate_to
  );
  true
 }
}
