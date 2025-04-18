use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedAudioSink, SharedChannelData, SharedProcessorConf, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;
use regex::Regex;
use rodio::Decoder;
use serde::{Deserialize, Serialize};
use std::io::Cursor;
use std::path::PathBuf;

#[derive(Debug, Clone)]
pub struct CoeiroInk {
 conf: SharedProcessorConf,
 channel_data: SharedChannelData,
 synthesis_or_predict_request_url: String,
 synthesis_or_predict_request_template: SynthesisOrPredictRequest,
 audio_file_store_path: Option<String>,
 split_regex: Option<Regex>,
 audio_sink: SharedAudioSink,
}

const DEFAULT_VOLUME_SCALE: f64 = 1.00;
const DEFAULT_PITCH_SCALE: f64 = 0.00;
const DEFAULT_INTONATION_SCALE: f64 = 1.00;
const DEFAULT_PRE_PHONEME_LENGTH: f64 = 0.10;
const DEFAULT_POST_PHONEME_LENGTH: f64 = 0.10;
const DEFAULT_OUTPUT_SAMPLING_RATE: u32 = 48000;

#[async_trait]
impl Processor for CoeiroInk {
 const FEATURE: &'static str = "coeiroink";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("CoeiroInk::process() が呼び出されました。");

  let channel_data = self.channel_data.clone();
  let synthesis_or_predict_request_template = self.synthesis_or_predict_request_template.clone();
  let synthesis_or_predict_request_url = self.synthesis_or_predict_request_url.clone();
  let audio_file_store_path = self.audio_file_store_path.clone();
  let split_regex = self.split_regex.clone();
  let audio_sink = self.audio_sink.clone();

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

   // split_sentence による分割
   let sources = match split_regex {
    // 正規表現で分割
    Some(split_regex) => split_regex
     .split(&source)
     .map(|s| s.trim().to_string())
     .filter(|s| !s.is_empty())
     .collect::<Vec<String>>(),
    // 分割しない
    _ => vec![source],
   };
   let sources_len = sources.len();

   let locked_audio_sink = audio_sink.lock().await;

   for (num, source) in sources.into_iter().enumerate() {
    let path = audio_file_store_path
     .as_ref()
     .map(|p| format!("{}_{}_{}.wav", &p, num, sources_len));

    // CoeiroInk に音声合成をリクエスト -> WAV ペイロードを取得
    log::debug!("CoeiroInk に音声合成をリクエストします。 {} / {}", num + 1, sources_len);
    let request_payload = synthesis_or_predict_request_template.build_with_text(source);
    let response = reqwest::Client::new()
     .post(&synthesis_or_predict_request_url)
     .json(&request_payload)
     .send()
     .await;

    let response = match response {
     Ok(response) => response,
     Err(e) => {
      log::error!("CoeiroInk への音声合成リクエストに失敗しました: {:?}", e);
      continue;
     },
    };

    let audio_data = response.bytes().await?;
    log::debug!("CoeiroInk からの音声合成データの取得に成功しました。");

    if let Some(path) = path {
     // path に {T} が含まれていたら ISO8601 日時文字列から : と - を除去して置換
     let path = match path.contains("{T}") {
      true => {
       let t = chrono::Utc::now().to_rfc3339().replace(":", "").replace("-", "");
       let path = std::path::Path::new(&path.replace("{T}", &t)).to_path_buf();
       path
      },
      false => PathBuf::from(path),
     };

     log::debug!("CoeiroInk からの音声合成データを {} に保存します。", path.display());
     tokio::fs::write(path, audio_data.clone()).await?;
     log::trace!("CoeiroInk からの音声合成データを保存しました。");
    }

    // ペイロードを WAV として再生
    let cursor = Cursor::new(audio_data);
    let source = Decoder::new(cursor)?;

    locked_audio_sink.0.append(source);

    log::debug!(
     "CoeiroInk からの音声合成データを再生シンクへ送出しました。 {} / {}",
     num + 1,
     sources_len
    );
   }

   Ok(())
  });

  log::trace!("CoeiroInk::process() は非同期処理を開始しました。");

  Ok(CompletedAnd::Next)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let pc = fix_conf(pc).await?;

  let mut p = CoeiroInk {
   conf: pc.as_shared(),
   channel_data: state.read().await.channel_data.clone(),
   synthesis_or_predict_request_url: pc.api_url.clone().unwrap(),
   synthesis_or_predict_request_template: SynthesisOrPredictRequest::default(),
   audio_file_store_path: pc.audio_file_store_path.clone(),
   split_regex: pc.split_regex_pattern.as_ref().map(|s| Regex::new(s).unwrap()),
   audio_sink: state.read().await.audio_sink.clone(),
  };

  p.update_template().await;

  if !p.is_established().await {
   bail!("CoeiroInk が正常に設定されていません: {:?}", pc);
  }

  Ok(ProcessorKind::CoeiroInk(p))
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

  log::info!(
   "CoeiroInk は正常に設定されています: channel: {:?} speed: {:?} tone: {:?} volume: {:?} voice: {:?}",
   conf.channel_from,
   conf.speed,
   conf.tone,
   conf.volume,
   conf.voice,
  );
  true
 }
}

async fn fix_conf(original: &ProcessorConf) -> Result<ProcessorConf> {
 let mut fixed = original.clone();

 if fixed.api_url.is_none() {
  log::warn!("api_url が設定されていないため、 http://localhost:50032/v1/predict にデフォルトします。");
  fixed.api_url = Some("http://localhost:50032/v1/predict".to_string());
 }

 if fixed.speaker_uuid.is_none() {
  log::warn!("speaker_uuid が設定されていません。 API でデフォルトロードを試みます。");
  match CoeiroInk::get_speakers().await {
   Ok(speakers) if !speakers.is_empty() => {
    log::warn!(
     "speaker_uuid を {} ( {} ) にデフォルトします。",
     speakers[0].speakerUuid,
     speakers[0].speakerName
    );
    fixed.speaker_uuid = Some(speakers[0].speakerUuid.clone());
   },
   _ => bail!("CoeiroInk と API 通信できなかったためデフォルトロードに失敗しました。CoeiroInk の動作状態を確認してください。"),
  }
 }

 if fixed.style_id.is_none() {
  log::warn!("style_id が設定されていません。 API でデフォルトロードを試みます。");
  match CoeiroInk::get_speakers().await {
   Ok(speakers) if !speakers.is_empty() => {
    let speaker = speakers
     .into_iter()
     .find(|s| s.speakerUuid.eq(fixed.speaker_uuid.as_ref().unwrap()));
    if speaker.is_none() {
     bail!("speaker_uuid の Speaker が CoeiroInk の応答に含まれませんでした。");
    }
    let style_id = speaker.unwrap().styles.iter().next().cloned();
    if style_id.is_none() {
     bail!("speaker_uuid の Speaker に Style 情報が存在しませんでした。");
    }
    let style = style_id.unwrap();
    log::warn!("style_id を {}: {} にデフォルトします。", style.styleId, style.styleName);
    fixed.style_id = Some(style.styleId);
   },
   _ => bail!("CoeiroInk と API 通信できなかったためデフォルトロードに失敗しました。CoeiroInk の動作状態を確認してください。"),
  }
 }
 if fixed.speed_scale.is_none() {
  log::warn!("speed_scale が設定されていないため 1.00 にデフォルトします。");
  fixed.speed_scale = Some(1.00);
 }
 if fixed.processing_algorithm.is_none() {
  log::warn!("processing_algorithm が設定されていないため COEIROINK にデフォルトします。");
  fixed.processing_algorithm = Some("COEIROINK".to_string());
 }

 // api_url が synthesis で終わっているか、 synthesis 向けの何れかが設定されている場合は predict ではなく synthesis と推定
 if fixed.api_url.as_ref().unwrap().ends_with("synthesis")
  || fixed.volume_scale.is_some()
  || fixed.pitch_scale.is_some()
  || fixed.intonation_scale.is_some()
  || fixed.pre_phoneme_length.is_some()
  || fixed.post_phoneme_length.is_some()
  || fixed.output_sampling_rate.is_some()
 {
  // api_url が predict で終わっていれば警告
  if fixed.api_url.as_ref().unwrap().ends_with("predict") {
   log::warn!("api_url として predict が設定されていますが、volume_scale, pitch_scale, intonation_scale, pre_phoneme_length, post_phoneme_length, output_sampling_rate は synthesis でのみ有効です。 volume_scale: {:?}, pitch_scale: {:?}, intonation_scale: {:?}, pre_phoneme_length: {:?}, post_phoneme_length: {:?}, output_sampling_rate: {:?}", fixed.volume_scale, fixed.pitch_scale, fixed.intonation_scale, fixed.pre_phoneme_length, fixed.post_phoneme_length, fixed.output_sampling_rate);
  }
  if fixed.volume_scale.is_none() {
   log::warn!("synthesis API モードが設定されましたが volume_scale が設定されていないため 1.00 にデフォルトします。");
   fixed.volume_scale = Some(DEFAULT_VOLUME_SCALE);
  }
  if fixed.pitch_scale.is_none() {
   log::warn!("synthesis API モードが設定されましたが pitch_scale が設定されていないため 0.00 にデフォルトします。");
   fixed.pitch_scale = Some(DEFAULT_PITCH_SCALE);
  }
  if fixed.intonation_scale.is_none() {
   log::warn!("synthesis API モードが設定されましたが intonation_scale が設定されていないため 1.00 にデフォルトします。");
   fixed.intonation_scale = Some(DEFAULT_INTONATION_SCALE);
  }
  if fixed.pre_phoneme_length.is_none() {
   log::warn!("synthesis API モードが設定されましたが pre_phoneme_length が設定されていないため 0.10 にデフォルトします。");
   fixed.pre_phoneme_length = Some(DEFAULT_PRE_PHONEME_LENGTH);
  }
  if fixed.post_phoneme_length.is_none() {
   log::warn!("synthesis API モードが設定されましたが post_phoneme_length が設定されていないため 0.1 にデフォルトします。");
   fixed.post_phoneme_length = Some(DEFAULT_POST_PHONEME_LENGTH);
  }
  if fixed.output_sampling_rate.is_none() {
   log::warn!("synthesis API モードが設定されましたが output_sampling_rate が設定されていないため 48000 にデフォルトします。");
   fixed.output_sampling_rate = Some(DEFAULT_OUTPUT_SAMPLING_RATE);
  }
 }

 Ok(fixed)
}

#[allow(dead_code)]
#[allow(non_snake_case)]
#[derive(Debug, Clone, Deserialize)]
pub struct Speaker {
 pub speakerName: String,
 pub speakerUuid: String,
 pub styles: Vec<SpeakerStyle>,
 pub version: Option<String>,
 pub base64Portrait: Option<String>,
}

#[allow(dead_code)]
#[allow(non_snake_case)]
#[derive(Debug, Clone, Deserialize)]
pub struct SpeakerStyle {
 pub styleName: String,
 pub styleId: i64,
 pub base64Icon: Option<String>,
 pub base64Portrait: Option<String>,
}

pub type Speakers = Vec<Speaker>;

impl CoeiroInk {
 pub async fn get_speakers() -> Result<Speakers> {
  let url = "http://127.0.0.1:50032/v1/speakers";
  let res = reqwest::get(url).await?;
  Ok(res.json::<Speakers>().await?)
 }
}

// JSON API の仕様にあわせるため、フィールド名はcamelCaseで定義する
// このため警告を無効化するために #[allow(non_snake_case)] を付与する
#[allow(non_snake_case)]
#[derive(Debug, Deserialize, Serialize, Clone, Default)]
pub struct SynthesisOrPredictRequest {
 speakerUuid: String,
 styleId: i64,
 text: String,
 speedScale: f64,
 volumeScale: f64,
 pitchScale: f64,
 intonationScale: f64,
 prePhonemeLength: f64,
 postPhonemeLength: f64,
 outputSamplingRate: u32,
 processingAlgorithm: String,
}

impl SynthesisOrPredictRequest {
 fn build_with_text(&self, text: String) -> Self {
  let mut cloned = self.clone();
  cloned.text = text;
  cloned
 }
}

impl CoeiroInk {
 async fn update_template(&mut self) {
  let conf = self.conf.read().await;
  // for predict & synthesis
  self.synthesis_or_predict_request_template.speakerUuid = conf.speaker_uuid.clone().unwrap();
  self.synthesis_or_predict_request_template.styleId = conf.style_id.unwrap();
  self.synthesis_or_predict_request_template.speedScale = conf.speed_scale.unwrap();
  // for synthesis
  self.synthesis_or_predict_request_template.volumeScale = conf.volume_scale.unwrap_or(DEFAULT_VOLUME_SCALE);
  self.synthesis_or_predict_request_template.pitchScale = conf.pitch_scale.unwrap_or(DEFAULT_PITCH_SCALE);
  self.synthesis_or_predict_request_template.intonationScale = conf.intonation_scale.unwrap_or(DEFAULT_INTONATION_SCALE);
  self.synthesis_or_predict_request_template.prePhonemeLength = conf.pre_phoneme_length.unwrap_or(DEFAULT_PRE_PHONEME_LENGTH);
  self.synthesis_or_predict_request_template.postPhonemeLength = conf.post_phoneme_length.unwrap_or(DEFAULT_POST_PHONEME_LENGTH);
  self.synthesis_or_predict_request_template.outputSamplingRate = conf.output_sampling_rate.unwrap_or(DEFAULT_OUTPUT_SAMPLING_RATE);
 }
}
