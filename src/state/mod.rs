mod channel_datum;

pub use channel_datum::{ChannelData, ChannelDatum, SharedChannelData};

use crate::{processor::*, Arc, Conf, RwLock};
use anyhow::Result;
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

pub type SharedState = Arc<RwLock<State>>;

const DEFAULT_STATE_DATA_CAPACITY: usize = 256;

#[derive(Debug, Clone)]
pub struct State {
 pub state_data_capacity: usize,
 pub state_data_path: Option<PathBuf>,
 pub state_data_auto_save: bool,
 pub state_data_pretty: bool,
 pub channel_data: SharedChannelData,
 pub processors: Vec<ProcessorKind>,
}

impl State {
 pub async fn new(conf: &Conf) -> Result<SharedState> {
  let channel_data = match &conf.state_data_path {
   Some(path) => load_channel_data(path).await?,
   None => Arc::new(RwLock::new(VecDeque::new())),
  };
  log::trace!("ChannelData の初期化が完了しました。");

  let state = Arc::new(RwLock::new(Self {
   state_data_capacity: conf.state_data_capacity.unwrap_or(DEFAULT_STATE_DATA_CAPACITY),
   state_data_path: conf.state_data_path.clone(),
   state_data_auto_save: conf.state_data_auto_save.unwrap_or(false),
   state_data_pretty: conf.state_data_pretty.unwrap_or(false),
   channel_data,
   processors: vec![],
  }));
  log::trace!("State の生成が完了しました。");

  let processors = init_processors(&conf, &state).await?;

  state.write().await.processors.extend(processors);
  log::trace!("State の processors を更新し、 State の初期化が完了しました。");
  Ok(state)
 }

 pub async fn rfind_channel_datum(&self, id: u64) -> Option<ChannelDatum> {
  let channel_data = self.channel_data.read().await;
  let datum = channel_data.iter().rfind(|cd| cd.get_id() == id);
  datum.cloned()
 }

 pub async fn push_channel_data(&self, channel_data: ChannelData) {
  for cd in channel_data.into_iter() {
   self.push_channel_datum(cd).await;
  }
 }

 pub async fn push_channel_datum(&self, cd: ChannelDatum) {
  let id = cd.get_id();
  let channel_from = cd.channel.clone();

  // データ追加
  log::trace!("ChannelDatum を追加します: {:?}", cd);
  {
   let mut channel_data = self.channel_data.write().await;
   channel_data.push_back(cd);
   if channel_data.len() > self.state_data_capacity {
    log::trace!("channel_data の容量が上限を超えたため、先頭の要素を削除します。");
    channel_data.pop_front();
   }
   log::trace!("channel_data の容量: {}", channel_data.len());
  }

  // Processor の実行
  for (i, p) in self.processors.iter().enumerate() {
   log::trace!("Processor を実行します: {:?} / {:?}", i, self.processors.len());
   if p.is_channel_from(&channel_from) {
    p.process(id).await.unwrap();
   }
   log::trace!("Processor の実行が完了しました。")
  }
  log::trace!("Processors の実行が完了しました。");

  if self.state_data_auto_save {
   log::trace!("state_data_auto_save が有効になっているため、保存処理を行います。");
   self.save().await.unwrap();
  }
  log::trace!("push_channel_datum の処理が完了しました。")
 }

 pub async fn save(&self) -> Result<()> {
  if self.state_data_path.is_none() {
   log::warn!("state_data_path が設定されていないため、保存処理を行えませんでした。");
   return Ok(());
  }
  let path = self.state_data_path.as_ref().unwrap();
  save_channel_data(path, &self.channel_data).await?;
  Ok(())
 }

 pub async fn load(&mut self) -> Result<()> {
  let channel_data = load_channel_data(self.state_data_path.as_ref().unwrap()).await?;
  self.channel_data = channel_data;
  Ok(())
 }
}

async fn init_processors(conf: &Conf, state: &SharedState) -> Result<Vec<ProcessorKind>> {
 let mut processors = Vec::new();

 for pc in conf.processors.iter() {
  if pc.feature.is_none() {
   continue;
  }
  let feature = pc.feature.as_ref().unwrap();
  let pk = match feature.to_lowercase().as_str() {
   OpenAiChat::FEATURE => OpenAiChat::new(&pc, state).await?,
   GasTranslation::FEATURE => GasTranslation::new(&pc, state).await?,
   Bouyomichan::FEATURE => Bouyomichan::new(&pc, state).await?,
   CoeiroInk::FEATURE => CoeiroInk::new(&pc, state).await?,
   OsTts::FEATURE => OsTts::new(&pc, state).await?,
   _ => {
    log::warn!("未実装の ProcessorConf が指定されました: {:?}", pc);
    continue;
   },
  };
  processors.push(pk);
 }

 Ok(processors)
}

async fn load_channel_data<P: AsRef<Path>>(path: P) -> Result<SharedChannelData> {
 let path = path.as_ref();
 if !path.exists() {
  log::warn!(
   "指定されたファイルが存在しないため、読み込み処理を行えませんでした。 path = {:?}",
   path
  );
  return Ok(Arc::new(RwLock::new(VecDeque::new())));
 }
 let serialized_channel_data = tokio::fs::read_to_string(path).await?;
 let channel_data = ron::de::from_str(&serialized_channel_data)?;
 Ok(Arc::new(RwLock::new(channel_data)))
}

async fn save_channel_data<P: AsRef<Path>>(path: P, channel_data: &SharedChannelData) -> Result<()> {
 let channel_data = channel_data.read().await;
 let channel_data = channel_data.iter().collect::<VecDeque<_>>();
 let serialized_channel_data = ron::ser::to_string_pretty(&channel_data, ron::ser::PrettyConfig::new().indentor(" ".to_string()))?;
 tokio::fs::write(path, serialized_channel_data).await?;
 Ok(())
}
