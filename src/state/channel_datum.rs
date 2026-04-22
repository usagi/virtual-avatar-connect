use crate::state::channel_attach::{Attachment, DataSource};
use crate::{Arc, RwLock};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

/// VAC 内部チャンネルを流れる単位データ。`content` は従来どおり表示用テキスト。
/// `source` / `meta` / `attachments` は Phase 0 で追加された加算的拡張であり、既存 processor は
/// これらを読まなければ従来通り動作する。
#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ChannelDatum {
 pub channel: String,
 pub content: String,
 pub flags: HashSet<String>,

 /// 由来（プロデューサ・外部イベント種別・表示用当事者名など）。未指定なら `None`。
 #[serde(default, skip_serializing_if = "Option::is_none")]
 pub source: Option<DataSource>,

 /// 構造化された副情報。小さなスカラー・文字列・簡単な JSON 値を想定（重いバイナリは `attachments` へ）。
 #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
 pub meta: BTreeMap<String, serde_json::Value>,

 /// 添付データ（URL / 一時ファイル / 小さなインライン / 生 JSON）。順序を保持する。
 #[serde(default, skip_serializing_if = "Vec::is_empty")]
 pub attachments: Vec<Attachment>,

 #[serde(deserialize_with = "deserialize_and_reset_id_counter")]
 id: u64,
 datetime: DateTime<Utc>,
}

pub type ChannelData = VecDeque<ChannelDatum>;
pub type SharedChannelData = Arc<RwLock<ChannelData>>;

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

impl ChannelDatum {
 pub const FLAG_IS_FINAL: &'static str = "is_final";
 pub const DATA_URLS: &'static str = "data_urls";

 pub fn reset_id_counter(id: u64) {
  ID_COUNTER.store(id, Ordering::Relaxed);
 }

 pub fn get_last_id() -> u64 {
  ID_COUNTER.load(Ordering::Relaxed)
 }

 pub fn new(channel: String, content: String) -> Self {
  Self {
   channel,
   content,
   flags: HashSet::new(),
   source: None,
   meta: BTreeMap::new(),
   attachments: Vec::new(),
   id: ID_COUNTER.fetch_add(1, Ordering::Relaxed) + 1,
   datetime: Utc::now(),
  }
 }

 pub fn move_from(channel_datum: ChannelDatum) -> ChannelDatum {
  Self {
   channel: channel_datum.channel,
   content: channel_datum.content,
   flags: channel_datum.flags,
   source: channel_datum.source,
   meta: channel_datum.meta,
   attachments: channel_datum.attachments,
   id: ID_COUNTER.fetch_add(1, Ordering::Relaxed) + 1,
   datetime: Utc::now(),
  }
 }

 pub fn get_id(&self) -> u64 {
  self.id
 }

 pub fn get_datetime(&self) -> DateTime<Utc> {
  self.datetime
 }

 pub fn with_channel(mut self, channel: String) -> Self {
  self.channel = channel;
  self
 }

 pub fn with_content(mut self, content: String) -> Self {
  self.content = content;
  self
 }

 pub fn with_flag_if(mut self, flag: &str, condition: bool) -> Self {
  if condition {
   self.flags.insert(flag.to_string());
  }
  self
 }

 pub fn with_flag(mut self, flag: &str) -> Self {
  self.flags.insert(flag.to_string());
  self
 }

 pub fn has_flag(&self, flag: &str) -> bool {
  self.flags.contains(flag)
 }

 /// 由来情報を設定する。
 pub fn with_source(mut self, source: DataSource) -> Self {
  self.source = Some(source);
  self
 }

 /// meta に 1 エントリ追加する。値は `serde_json::Value` に変換可能なら何でも入る。
 pub fn with_meta<V: Into<serde_json::Value>>(mut self, key: impl Into<String>, value: V) -> Self {
  self.meta.insert(key.into(), value.into());
  self
 }

 /// meta に複数エントリをマージする（既存キーは上書き）。
 pub fn with_meta_entries<I, K, V>(mut self, entries: I) -> Self
 where
  I: IntoIterator<Item = (K, V)>,
  K: Into<String>,
  V: Into<serde_json::Value>,
 {
  for (k, v) in entries {
   self.meta.insert(k.into(), v.into());
  }
  self
 }

 /// 添付を 1 件追加する。
 pub fn with_attachment(mut self, attachment: Attachment) -> Self {
  self.attachments.push(attachment);
  self
 }

 pub fn source(&self) -> Option<&DataSource> {
  self.source.as_ref()
 }

 pub fn meta_get(&self, key: &str) -> Option<&serde_json::Value> {
  self.meta.get(key)
 }

 pub fn attachments(&self) -> &[Attachment] {
  &self.attachments
 }
}

fn deserialize_and_reset_id_counter<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
 D: serde::Deserializer<'de>,
{
 let id = u64::deserialize(deserializer)?;
 ID_COUNTER.store(id, Ordering::Relaxed);
 Ok(id)
}
