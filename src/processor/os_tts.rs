use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;

#[derive(Clone)]
pub struct OsTts {
 conf: ProcessorConf,
 channel_data: SharedChannelData,
 tts: tts::Tts,
}

// Debug を手動実装
impl std::fmt::Debug for OsTts {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  f.debug_struct("OsTts")
   .field("conf", &self.conf)
   .field("channel_data", &self.channel_data)
   .finish()
 }
}

#[async_trait]
impl Processor for OsTts {
 const FEATURE: &'static str = "os-tts";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("OsTts::process() が呼び出されました。");

  let channel_data = self.channel_data.clone();
  let mut tts = self.tts.clone();

  tokio::spawn(async move {
   // 入力を取得
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

   log::debug!("OsTts に音声合成をリクエストします。");
   if let Some(e) = tts.speak(source, false).err() {
    log::error!("音声合成に失敗しました: {:?}", e);
    return Ok(());
   }

   // tts の処理が終わるまで待機
   while tts.is_speaking().unwrap() {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
   }

   Ok(())
  });

  log::trace!("OsTts::process() は非同期処理を開始しました。");

  Ok(CompletedAnd::Next)
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = OsTts {
   conf: pc.clone(),
   channel_data: state.read().await.channel_data.clone(),
   tts: tts::Tts::default()?,
  };

  if !p.is_established().await {
   bail!("OsTts が正常に設定されていません: {:?}", pc);
  }

  Ok(ProcessorKind::OsTts(p))
 }

 fn is_channel_from(&self, channel_from: &str) -> bool {
  self.conf.channel_from.as_ref().unwrap() == channel_from
 }

 async fn is_established(&mut self) -> bool {
  if self.conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }
  let voices = if let Ok(voice) = self.tts.voices() {
   voice
  } else {
   log::error!("音声合成エンジンが見つかりませんでした。");
   return false;
  };

  let mut is_voice_set = false;

  if let Some(ref id) = self.conf.voice_id {
   if let Some(voice) = voices.iter().find(|v| v.id().eq(id)) {
    if let Some(e) = self.tts.set_voice(voice).err() {
     log::error!("voice_id の設定に失敗しました: {:?}", e);
     return false;
    }
    is_voice_set = true;
   }
  }

  // voice_name の場合は voice に部分一致すればよい
  if is_voice_set == false {
   if let Some(name) = &self.conf.voice_name {
    if let Some(voice) = voices.iter().find(|v| v.name().contains(name)) {
     if let Some(e) = self.tts.set_voice(voice).err() {
      log::error!("voice_name の設定に失敗しました: {:?}", e);
      return false;
     }
     is_voice_set = true;
    }
   }
  }

  if is_voice_set == false {
   log::warn!("voice_name も voice_id も設定されていないため OS のデフォルトの音声を使用します。")
  }

  if let Some(mut rate) = self.conf.tts_rate {
   let min = self.tts.min_rate();
   let max = self.tts.max_rate();
   if rate < min || rate > max {
    log::warn!(
     "tts_rate が範囲外のため設定値を clamp します。 {} から {} の間で指定してください。",
     min,
     max
    );
    rate = rate.clamp(min, max);
   }
   if let Some(e) = self.tts.set_rate(rate).err() {
    log::error!("tts_rate の設定に失敗しました: {:?}", e);
    return false;
   }
   log::trace!("tts_rate を設定しました: {}", rate);
  }

  if let Some(mut pitch) = self.conf.tts_pitch {
   let min = self.tts.min_pitch();
   let max = self.tts.max_pitch();
   if pitch < min || pitch > max {
    log::warn!(
     "tts_pitch が範囲外のため設定値を clamp します。 {} から {} の間で指定してください。",
     min,
     max
    );
    pitch = pitch.clamp(min, max);
   }
   if let Some(e) = self.tts.set_pitch(pitch).err() {
    log::error!("tts_pitch の設定に失敗しました: {:?}", e);
    return false;
   }
   log::trace!("tts_pitch を設定しました: {}", pitch);
  }

  if let Some(mut volume) = self.conf.tts_volume {
   let min = self.tts.min_volume();
   let max = self.tts.max_volume();
   if volume < min || volume > max {
    log::warn!(
     "tts_volume が範囲外のため設定値を clamp します。 {} から {} の間で指定してください。",
     min,
     max
    );
    volume = volume.clamp(min, max);
   }
   if let Some(e) = self.tts.set_volume(volume).err() {
    log::error!("tts_volume の設定に失敗しました: {:?}", e);
    return false;
   }
   log::trace!("tts_volume を設定しました: {}", volume);
  }

  log::info!(
   "OsTts は正常に設定されています: channel: {:?} voice: {:?} rate: {:?} pitch: {:?} volume: {:?}",
   self.conf.channel_from,
   self.tts.voice().unwrap().unwrap(),
   self.tts.get_rate(),
   self.tts.get_pitch(),
   self.tts.get_volume(),
  );
  true
 }
}
