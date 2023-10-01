use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct Command {
 conf: ProcessorConf,
 channel_data: SharedChannelData,
 if_not_command: CompletedAnd,
}

#[async_trait]
impl Processor for Command {
 const FEATURE: &'static str = "command";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("Command::process() が呼び出されました。");

  // 入力を取得
  let content = {
   let channel_data = self.channel_data.read().await;
   match channel_data.iter().rev().find(|cd| cd.get_id() == id) {
    Some(source) if source.has_flag(ChannelDatum::FLAG_IS_FINAL) => source.content.clone(),
    Some(_) => {
     log::trace!("未確定の入力なので、処理をスキップします。");
     return Ok(self.if_not_command);
    },
    None => bail!("指定された id の ChannelDatum が見つかりませんでした: {}", id),
   }
  };

  if !content.starts_with("/") {
   log::debug!("コマンドではないので、処理をスキップします: {:?}", content);
   return Ok(self.if_not_command);
  }

  let mut args = content.trim_start_matches("/").split(" ").collect::<VecDeque<&str>>();
  let command = args.pop_front().unwrap_or_default();

  match command {
   "quit" => {
    log::info!("quit がコマンドされたので Virtual Avatar Connect は終了します。");
    std::process::exit(0);
   },
   "disable" => {
    log::info!("disable がコマンドされたので group = {} の Processor を無効化します。", args[0]);
   },
   "enable" => {
    log::info!("enable がコマンドされたので group = {} の Processor を有効化します。", args[0]);
   },
   "reload" => {
    log::info!("reload がコマンドされたので Virtual Avatar Connect は設定を再読み込みします。");
   },
   _ => {
    log::info!("コマンドまたは何かが違うようです。");
   },
  }

  Ok(CompletedAnd::Break)
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = Command {
   conf: pc.clone(),
   channel_data: state.read().await.channel_data.clone(),
   if_not_command: match pc.through_if_not_command {
    Some(true) => CompletedAnd::Next,
    _ => CompletedAnd::Break,
   },
  };

  if !p.is_established().await {
   bail!("Command が正常に設定されていません: {:?}", pc);
  }

  Ok(ProcessorKind::Command(p))
 }

 fn is_channel_from(&self, channel_from: &str) -> bool {
  self.conf.channel_from.as_ref().unwrap() == channel_from
 }

 async fn is_established(&mut self) -> bool {
  if self.conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }

  if self.conf.channel_to.is_some() {
   log::info!(
    "channel_to が設定されているため、コマンドの処理結果が channel_to へ送信されます: channel_to={:?}",
    self.conf.channel_to,
   );
  }

  if let Some(true) = self.conf.through_if_not_command {
   log::info!("through_if_not_command が設定されているため、コマンドではない入力はそのまま後続の Processor 群へ流れます。");
  }

  log::info!(
   "Command は正常に設定されています: channel_from={:?} channel_to={:?}",
   self.conf.channel_from,
   self.conf.channel_to
  );

  true
 }
}
