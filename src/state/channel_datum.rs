use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug, Deserialize, Serialize, Default, Clone)]
pub struct ChannelDatum {
 pub channel: String,
 pub content: String,
 pub flags: HashSet<String>,
 #[serde(deserialize_with = "deserialize_and_reset_id_counter")]
 id: u64,
 datetime: DateTime<Utc>,
}

pub type ChannelData = VecDeque<ChannelDatum>;
pub type SharedChannelData = Arc<RwLock<ChannelData>>;

static ID_COUNTER: AtomicU64 = AtomicU64::new(0);

impl ChannelDatum {
 pub const FLAG_IS_FINAL: &str = "is_final";

 pub fn reset_id_counter(id: u64) {
  ID_COUNTER.store(id, Ordering::Relaxed);
 }

 pub fn new(channel: String, content: String) -> Self {
  Self {
   channel,
   content,
   flags: HashSet::new(),
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
}

fn deserialize_and_reset_id_counter<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
 D: serde::Deserializer<'de>,
{
 let id = u64::deserialize(deserializer)?;
 ID_COUNTER.store(id, Ordering::Relaxed);
 Ok(id)
}
