use super::{CompletedAnd, Processor};
use crate::conf::CommandSet;
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedProcessorConf, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::collections::VecDeque;

#[derive(Clone, Debug)]
pub struct Command {
 conf: SharedProcessorConf,
 state: SharedState,
 channel_data: SharedChannelData,
 if_not_command: CompletedAnd,
}

#[async_trait]
impl Processor for Command {
 const FEATURE: &'static str = "command";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("Command::process() が呼び出されました。");

  let conf = self.conf.read().await.clone();

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
   "disable" if args.len() >= 1 => {
    log::info!("disable がコマンドされたので group = {} の Processor を無効化します。", args[0]);
    response1(
     conf,
     self.state.clone(),
     "disable",
     "group = {A} の Processor を無効化しました。",
     args[0],
    )
    .await;
   },
   "enable" => {
    if args.len() >= 1 {
     log::info!("enable がコマンドされたので group = {} の Processor を有効化します。", args[0]);
     response1(
      conf,
      self.state.clone(),
      "enable",
      "group = {A} の Processor を有効化しました。",
      args[0],
     )
     .await;
    }
   },
   "reload" => {
    log::info!("reload がコマンドされたので Virtual Avatar Connect は設定を再読み込みします。");
    response1(
     conf,
     self.state.clone(),
     "reload",
     "group = {A} の Processor の設定を再読み込みしました。",
     args[0],
    )
    .await;
   },
   "set" if args.len() >= 1 => {
    log::info!("set がコマンドされセット名 {:?} の実行が試行されます。", args[0]);
    response1(conf.clone(), self.state.clone(), "set", "セット {A} の実行を試みます。", args[0]).await;
    if let Err(e) = activate_command_set(args[0], &conf.set, self.state.clone()).await {
      log::error!("セットの実行中にエラーが発生しました: {:?}", e);
      response1(
       conf,
       self.state.clone(),
       "set:error",
       "セット {A} の実行中にエラーが発生しました。",
       args[0],
      )
      .await;
    }
   },
   _ => {
    log::warn!("コマンドまたは何かが違うようです: command = {:?} args = {:?}", command, args);
    response0(conf, self.state.clone(), "_", "コマンドまたは何かが違うようです。").await;
   },
  }

  Ok(CompletedAnd::Break)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = Command {
   conf: pc.as_shared(),
   state: state.clone(),
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

  if conf.channel_to.is_some() {
   log::info!(
    "channel_to が設定されているため、コマンドの処理結果が channel_to へ送信されます: channel_to={:?}",
    conf.channel_to,
   );
  }

  if let Some(true) = conf.through_if_not_command {
   log::info!("through_if_not_command が設定されているため、コマンドではない入力はそのまま後続の Processor 群へ流れます。");
  }

  log::info!(
   "Command は正常に設定されています: channel_from={:?} channel_to={:?}",
   conf.channel_from,
   conf.channel_to
  );

  true
 }
}

// 0 変数版
async fn response0(conf: ProcessorConf, state: SharedState, command: &str, default_message: &str) {
 let content = conf
  .response_mod
  .iter()
  .find_map(|v| if v[0] == command { Some(v[1].clone()) } else { None })
  .unwrap_or_else(|| default_message.to_string());
 let cd = ChannelDatum::new(conf.channel_to.unwrap(), content).with_flag(ChannelDatum::FLAG_IS_FINAL);

 let state = state.read().await;
 state.push_channel_datum(cd).await;
}

// 1 変数版
async fn response1(conf: ProcessorConf, state: SharedState, command: &str, default_message: &str, a: &str) {
 let content = conf
  .response_mod
  .iter()
  .find_map(|v| if v[0] == command { Some(v[1].clone()) } else { None })
  .unwrap_or_else(|| default_message.to_string())
  .replace("{A}", a);
 let cd = ChannelDatum::new(conf.channel_to.unwrap(), content).with_flag(ChannelDatum::FLAG_IS_FINAL);

 let state = state.read().await;
 state.push_channel_datum(cd).await;
}

#[async_recursion::async_recursion]
async fn activate_command_set(set_name: &str, command_sets: &Vec<CommandSet>, state: SharedState) -> Result<()> {
 // find
 let command = command_sets
  .iter()
  .find(|&c| c.name == set_name)
  .ok_or_else(|| anyhow::anyhow!("セット名 {:?} が見つかりませんでした。", set_name))?;

 for pre in &command.pre {
  if let Err(e) = activate_command_set(&pre, command_sets, state.clone()).await {
   log::error!("pre 処理でエラーが発生しました: {:?}", e);
  }
 }

 for cc in &command.channel_contents {
  // log::warn!("channel = {:?} に content = {:?} を送信します。", cc.channel, cc.content)
  let cd = ChannelDatum::new(cc.channel.clone(), cc.content.clone()).with_flag(ChannelDatum::FLAG_IS_FINAL);
  let state = state.read().await;
  state.push_channel_datum(cd).await;
 }

 for post in &command.post {
  if let Err(e) = activate_command_set(&post, command_sets, state.clone()).await {
   log::error!("post 処理でエラーが発生しました: {:?}", e);
  }
 }

 Ok(())
}
