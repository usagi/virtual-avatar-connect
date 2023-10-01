use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedProcessorConf, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;

#[derive(Debug, Clone)]
pub struct GasTranslation {
 conf: SharedProcessorConf,
 state: SharedState,
 channel_data: SharedChannelData,
 url_base: String,
}

const GOOGLE_APPS_SCRIPT_URL_TEMPLATE: &str =
 "https://script.google.com/macros/s/{script_id}/exec?trans_sourcelang={translate_from}&target={translate_to}&text=";

#[async_trait]
impl Processor for GasTranslation {
 const FEATURE: &'static str = "gas-translation";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("GasTranslation::process() が呼び出されました。");

  let conf = self.conf.read().await.clone();
  let state = self.state.clone();
  let channel_data = self.channel_data.clone();
  let url_base = self.url_base.clone();
  let channel_to = conf.channel_to.as_ref().unwrap().clone();
  let translate_to = conf.translate_to.as_ref().unwrap().clone();

  tokio::spawn(async move {
   // 翻訳元を取得
   let source = {
    let channel_data = channel_data.read().await;
    match channel_data.iter().rev().find(|cd| cd.get_id() == id) {
     Some(source) if source.has_flag(ChannelDatum::FLAG_IS_FINAL) => {
      // 空文字列なら処理をスキップ
      if source.content.is_empty() {
       log::trace!("空文字列なので、処理をスキップします。");
       return Ok(());
      }
      source.content.clone()
     },
     Some(_) => {
      log::trace!("未確定の入力なので、処理をスキップします。");
      return Ok(());
     },
     None => bail!("指定された id の ChannelDatum が見つかりませんでした: {}", id),
    }
   };

   // APIパラメーター決定
   let translate_from = match &conf.translate_from {
    Some(translate_from) => translate_from.clone(),
    _ => {
     // 入力の言語を推定
     let info = match whatlang::detect(&source) {
      Some(info) => info,
      None => {
       log::error!("入力言語の推定に失敗しました。Processorの設定で言語を固定して構わない場合は明示的に設定すると処理効率が向上します。");
       return Ok(());
      },
     };
     let l3 = info.lang().code();
     let l2 = isolang::Language::from_639_3(l3).unwrap().to_639_1().unwrap().to_string();
     log::debug!("推定された言語: lang={:?} info={:?}", l2, info);
     l2
    },
   };
   let url_base = url_base.replace("{translate_from}", &translate_from);
   let url = format!("{}{}", url_base, urlencoding::encode(&source));
   log::debug!("url = {}", url);

   // 翻訳API投げ
   let response = reqwest::get(&url).await?;
   log::trace!("response = {:?}", response);
   let output_content = response.text().await?;
   log::debug!("output_content = {}", output_content);

   // 翻訳結果を書き込み
   let output_channel_datum = ChannelDatum::new(channel_to, output_content)
    .with_flag(ChannelDatum::FLAG_IS_FINAL)
    .with_flag(&format!(
     "{}({}:{},{}/{})",
     Self::FEATURE,
     "gas-translation",
     id,
     translate_from,
     translate_to
    ));
   log::debug!("output_channel_datum = {:?}", output_channel_datum);

   {
    let state = state.read().await;
    state.push_channel_datum(output_channel_datum).await;
   }

   Ok(())
  });

  Ok(CompletedAnd::Next)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = GasTranslation {
   conf: pc.as_shared(),
   state: state.clone(),
   channel_data: state.read().await.channel_data.clone(),
   url_base: String::new(),
  };

  if !p.is_established().await {
   bail!("GasTranslation が正常に設定されていません: {:?}", pc);
  }

  {
   let conf = p.conf.read().await;
   p.url_base = GOOGLE_APPS_SCRIPT_URL_TEMPLATE
    .replace("{script_id}", conf.script_id.as_ref().unwrap())
    .replace("{translate_to}", conf.translate_to.as_ref().unwrap());
  }

  Ok(ProcessorKind::GasTranslation(p))
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
  if conf.script_id.is_none() {
   log::error!("script_id が設定されていません。");
   return false;
  }
  if conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }
  if conf.channel_to.is_none() {
   log::error!("channel_to が設定されていません。");
   return false;
  }
  if conf.translate_from.is_none() {
   log::info!("translation_from が設定さていないため、入力ごとに自動推定を行います。多言語の入力に対応する必要が無い場合は明示的に言語を設定すると処理効率が向上します。");
  }
  if conf.translate_to.is_none() {
   log::error!("translation_to が設定されていません。");
   return false;
  }
  log::info!(
   "GasTranslation は正常に設定されています: channel: {:?} -> {:?} lang: {:?} -> {:?}",
   conf.channel_from,
   conf.channel_to,
   conf.translate_from,
   conf.translate_to
  );
  true
 }
}
