use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedProcessorConf, SharedState};
use alkana_rs::ALKANA;
use anyhow::{bail, Result};
use async_trait::async_trait;
use std::io::BufRead;

#[derive(Clone, Debug)]
pub struct Modify {
 conf: SharedProcessorConf,
 state: SharedState,

 channel_from: String,
 channel_to: String,

 channel_data: SharedChannelData,
 // to <- from
 dictionary: Vec<(String, String)>,
 // to_replacer <- from_matcher
 regexes: Vec<(String, regex::Regex)>,
}

#[async_trait]
impl Processor for Modify {
 const FEATURE: &'static str = "modify";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("Modify::process() が呼び出されました。");

  let conf = self.conf.read().await;

  // このProcessorの処理完了までにデータが変更されない事を保証するため write ロックを通す
  let mut channel_data = self.channel_data.write().await;

  // ID が一致する最新の &mut ChannelDatum を取得
  let index = match channel_data.iter().rev().position(|cd| cd.get_id() == id) {
   Some(rev_index) => channel_data.len().saturating_sub(rev_index).saturating_sub(1),
   None => bail!("指定された id の ChannelDatum が見つかりませんでした: {}", id),
  };

  log::trace!("cd: {:?} i={:?} l={:?}", channel_data[index], index, channel_data.len());

  let mut content = channel_data[index].content.clone();

  // alkana 変換
  if let Some(true) = conf.alkana {
   // 先に . , : ; などの ASCII 記号文字の前後に空白文字を挿入
   for c in content.clone().chars() {
    if c.is_ascii_punctuation() {
     content = content.replace(&c.to_string(), &format!(" {} ", c));
    }
   }

   // content から word 単位で文字列を取り出し、それが辞書に登録されていれば変換
   for word in content.clone().split_whitespace() {
    if let Some(to) = ALKANA.get_katakana(word) {
     content = content.replace(word, &to);
    }
   }

   // 空白文字を削除
   content = content.replace(" ", "");
  }

  // 辞書による変換
  for (to, from) in &self.dictionary {
   content = content.replace(from, to);
  }

  // 正規表現による変換
  for (replacer, from_regex) in &self.regexes {
   content = from_regex.replace_all(&content, replacer).to_string();
  }

  if self.channel_from == self.channel_to {
   // self.modify が true の場合は、変換後の文字列を元 &mut ChannelDatum の文字列に上書き
   log::debug!("変換後の文字列を元の文字列に上書きします。");
   channel_data[index].content = content;
  } else {
   let channel_data_clone = channel_data[index].clone();
   // unlock
   drop(channel_data);
   let channel_to = self.channel_to.clone();
   let state = self.state.clone();
   log::debug!(
    "変換後の文字列を新しい ChannelDatum として追加します。 {:?} -> {:?}",
    self.channel_from,
    self.channel_to
   );
   tokio::spawn(async move {
    // self.modify が false の場合は、変換後の文字列を新しい ChannelDatum として追加
    state
     .read()
     .await
     .push_channel_datum(
      ChannelDatum::move_from(channel_data_clone)
       .with_channel(channel_to)
       .with_content(content),
     )
     .await;
   });
  }

  Ok(CompletedAnd::Next)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = Modify {
   conf: pc.as_shared(),
   state: state.clone(),
   channel_from: "".to_string(),
   channel_to: "".to_string(),
   channel_data: state.read().await.channel_data.clone(),
   dictionary: vec![],
   regexes: vec![],
  };

  if !p.is_established().await {
   bail!("Modify が正常に設定されていません: {:?}", pc);
  }

  p.channel_from = pc.channel_from.as_ref().unwrap().clone();
  p.channel_to = pc.channel_to.as_ref().unwrap().clone();

  // 辞書ファイルの読み込み -> p.dictionary
  // 辞書ファイルは Google IME の辞書ファイルと同様のフォーマットかつ最初の2列のみを使用する
  for file in pc.dictionary_files.iter() {
   let file = std::path::Path::new(file);
   let file = if file.is_absolute() {
    file.to_path_buf()
   } else {
    std::env::current_dir()?.join(file)
   };
   let file = file.to_str().unwrap();
   let file = std::fs::File::open(file)?;
   let file = std::io::BufReader::new(file);
   for line in file.lines() {
    let line = line?;
    let line = line.trim();
    if line.is_empty() {
     continue;
    }
    let mut line = line.split_whitespace();
    let to = line.next().unwrap().to_string();
    let from = line.next().unwrap().to_string();
    p.dictionary.push((to, from));
   }
  }

  // sort_dictionary が length の場合は長い順にソート
  match &pc.sort_dictionary {
   Some(v) if v.to_lowercase().eq("length") => {
    p.dictionary.sort_by(|a, b| b.1.len().cmp(&a.1.len()));
   },
   _ => (),
  }

  // 正規表現ファイルの読み込み -> p.regexes
  // 正規表現ファイルは Google IME の正規表現ファイルと同様のフォーマットかつ最初の2列のみを使用する
  for file in pc.regex_files.iter() {
   let file = std::path::Path::new(file);
   let file = if file.is_absolute() {
    file.to_path_buf()
   } else {
    std::env::current_dir()?.join(file)
   };
   let file = file.to_str().unwrap();
   let file = std::fs::File::open(file)?;
   let file = std::io::BufReader::new(file);
   for line in file.lines() {
    let line = line?;
    let line = line.trim();
    if line.is_empty() {
     continue;
    }
    // lineを最後の空白文字で分割
    let mut line = line.rsplitn(2, ' ');
    let from_matcher = line.next().unwrap().to_string();
    let from_matcher = regex::Regex::new(&from_matcher)?;
    let to_replacer = line.next().unwrap().to_string();
    log::warn!("{} -> {}", from_matcher, to_replacer);
    p.regexes.push((to_replacer, from_matcher));
   }
  }

  Ok(ProcessorKind::Modify(p))
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

  // dictionary_files に指定されたファイルが存在するか確認
  for file in conf.dictionary_files.iter() {
   if !std::path::Path::new(file).exists() {
    log::error!("dictionary_files に指定されたファイルが存在しません: {}", file);
    return false;
   }
  }

  for file in conf.regex_files.iter() {
   if !std::path::Path::new(file).exists() {
    log::error!("regex_files に指定されたファイルが存在しません: {}", file);
    return false;
   }
  }

  match &conf.sort_dictionary {
   Some(v) if v.to_lowercase().eq("length") => (),
   Some(_) => log::warn!("sort_dictionary に指定された値が不正です。現在は未指定または length 以外の有効な値はありません。"),
   _ => (),
  }

  log::info!(
   "Modify は正常に設定されています: {:?} -> {:?} (modify: {:?}, dictionary_files: {:?}, regex_files: {:?})",
   conf.channel_from,
   conf.channel_to,
   conf.modify,
   conf.dictionary_files,
   conf.regex_files
  );
  true
 }
}
