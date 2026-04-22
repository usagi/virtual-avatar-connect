//! AI サービスが観測（observe）する Datum の通知型と、`ObservePolicyConf` を素に作るフィルタ補助。

use crate::ai::config::ObservePolicyConf;

/// AI サービスの event loop に流す観測通知。Datum の ID とチャンネル名のみ（本体はロード時に取得）。
#[derive(Clone, Debug)]
pub struct Observation {
 pub datum_id: u64,
 pub channel: String,
}

/// 文脈窓（memory window）や役割（user / assistant / system）付与のための、ペルソナ観点のフィルタ集合。
#[derive(Debug, Clone)]
pub struct ObserveSet {
 pub triggers: Vec<String>,
 pub channel_utterance: String,
 pub include_all: bool,
 pub include_additional: Vec<String>,
 pub exclude: Vec<String>,
}

impl ObserveSet {
 pub fn from_conf(channel_utterance: &str, policy: &ObservePolicyConf) -> Self {
  Self {
   triggers: policy.triggers.clone(),
   channel_utterance: channel_utterance.to_string(),
   include_all: policy.include_all,
   include_additional: policy.include_additional.clone(),
   exclude: policy.exclude.clone(),
  }
 }

 /// この発言を memory window に入れるか。
 pub fn is_observed(&self, channel: &str) -> bool {
  if self.exclude.iter().any(|c| c == channel) {
   return false;
  }
  if self.include_all {
   return true;
  }
  if channel == self.channel_utterance {
   return true;
  }
  if self.triggers.iter().any(|c| c == channel) {
   return true;
  }
  if self.include_additional.iter().any(|c| c == channel) {
   return true;
  }
  false
 }

 /// 発火のトリガーとなるチャンネルか（evaluate 起動の判定に使う）。
 pub fn is_trigger(&self, channel: &str) -> bool {
  self.triggers.iter().any(|c| c == channel)
 }
}
