use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedProcessorConf, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::path::Path;
use tokio::process::Command;

#[derive(Debug, Clone)]
pub struct Bouyomichan {
 conf: SharedProcessorConf,
 channel_data: SharedChannelData,
}

#[async_trait]
impl Processor for Bouyomichan {
 const FEATURE: &'static str = "bouyomichan";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("Bouyomichan::process() が呼び出されました。");
  // channel_data.read ロック
  let content = {
   let channel_data = self.channel_data.read().await;
   let source = channel_data.iter().rev().find(|cd| cd.get_id() == id);
   if source.is_none() {
    bail!("指定された id の ChannelDatum が見つかりませんでした: {}", id);
   }
   let source = source.unwrap();
   // 未確定の入力なら何もしない
   if !source.has_flag(ChannelDatum::FLAG_IS_FINAL) {
    log::trace!("未確定の入力なので、処理をスキップします。");
    return Ok(CompletedAnd::Next);
   }
   source.content.clone()
  };

  let conf = self.conf.read().await;

  let mut args = vec![
   "/T".to_string(),
   content,
   conf.speed.unwrap_or(-1).to_string(),
   conf.tone.unwrap_or(-1).to_string(),
   conf.volume.unwrap_or(-1).to_string(),
   conf.voice.unwrap_or(0).to_string(),
  ];
  match (&conf.address, conf.port) {
   (Some(ip), Some(port)) => args.extend(vec![ip.clone(), port.to_string()]),
   _ => (),
  }

  let command = conf.remote_talk_path.as_ref().unwrap();

  log::debug!("棒読みちゃんにリクエストを送信します。command = {:?}, args = {:?}", command, args);
  Command::new(command).args(args).spawn()?;

  Ok(CompletedAnd::Next)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = Bouyomichan {
   conf: pc.as_shared(),
   channel_data: state.read().await.channel_data.clone(),
  };

  if !p.is_established().await {
   bail!("Bouyomichan が正常に設定されていません: {:?}", pc);
  }

  Ok(ProcessorKind::Bouyomichan(p))
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
  match conf.remote_talk_path {
   Some(ref path) if !Path::new(path).exists() => {
    log::error!("指定されたコマンドが存在しません: {}", path);
    return false;
   },
   None => {
    log::error!("remote_talk_path が設定されていません。");
    return false;
   },
   _ => {},
  }
  log::info!(
   "Bouyomichan は正常に設定されています: channel: {:?} speed: {:?} tone: {:?} volume: {:?} voice: {:?}",
   conf.channel_from,
   conf.speed,
   conf.tone,
   conf.volume,
   conf.voice,
  );
  true
 }
}
