use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedAudioSink, SharedChannelData, SharedProcessorConf, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;

#[derive(Clone)]
pub struct OsTts {
 conf: SharedProcessorConf,
 channel_data: SharedChannelData,
 tts: tts::Tts,
 audio_sink: SharedAudioSink,
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

   log::debug!("OsTts に音声合成をリクエストします。");
   #[cfg(not(target_os = "windows"))]
   if let Some(e) = tts.speak(source, false).err() {
    log::error!("音声合成に失敗しました: {:?}", e);
    return Ok(());
   }
   #[cfg(target_os = "windows")]
   match tts.synthesize(source) {
    Ok(wav) => {
     let cursor = std::io::Cursor::new(wav);
     let source = rodio::Decoder::new(cursor).unwrap();
     audio_sink.lock().await.0.append(source)
    },
    Err(e) => {
     log::error!("音声合成に失敗しました: {:?}", e);
     return Ok(());
    },
   }

   // tts の処理が終わるまで待機
   #[cfg(not(target_os = "windows"))]
   while tts.is_speaking().unwrap() {
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
   }

   Ok(())
  });

  log::trace!("OsTts::process() は非同期処理を開始しました。");

  Ok(CompletedAnd::Next)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  let mut p = OsTts {
   conf: pc.as_shared(),
   channel_data: state.read().await.channel_data.clone(),
   tts: tts::Tts::default()?,
   audio_sink: state.read().await.audio_sink.clone(),
  };

  if !p.is_established().await {
   bail!("OsTts が正常に設定されていません: {:?}", pc);
  }

  Ok(ProcessorKind::OsTts(p))
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
  let voices = if let Ok(voice) = self.tts.voices() {
   voice
  } else {
   log::error!("音声合成エンジンが見つかりませんでした。");
   return false;
  };

  let mut is_voice_set = false;

  if let Some(ref id) = conf.voice_id {
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
   if let Some(name) = &conf.voice_name {
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

  if let Some(mut rate) = conf.tts_rate {
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

  if let Some(mut pitch) = conf.tts_pitch {
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

  if let Some(mut volume) = conf.tts_volume {
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
   conf.channel_from,
   self.tts.voice().unwrap().unwrap(),
   self.tts.get_rate(),
   self.tts.get_pitch(),
   self.tts.get_volume(),
  );
  true
 }
}

impl OsTts {
 /// OS-TTS のテストを実行します。コマンドライン引数からの実行を想定しています。
 pub async fn test(audio_sink: SharedAudioSink) -> Result<()> {
  let mut tts = tts::Tts::default()?;
  let voices = tts.voices()?;

  println!("--- OS-TTS Voices ---");
  println!(" - rate  : min={} std={} max={}", tts.min_rate(), tts.normal_rate(), tts.max_rate());
  println!(
   " - pitch : min={} std={} max={}",
   tts.min_pitch(),
   tts.normal_pitch(),
   tts.max_pitch()
  );
  println!(
   " - volume: min={} std={} max={}",
   tts.min_volume(),
   tts.normal_volume(),
   tts.max_volume()
  );

  if voices.is_empty() {
   log::warn!("お使いの環境では OS が提供可能な音声合成エンジンは見つかりませんでした。 OS の設定画面等から使いたい音声を有効化する必要があるかもしれません。");
   std::process::exit(1);
  }

  for voice in voices.iter() {
   println!(" * Lang: {}  Name: {}  ID: {}", voice.language(), voice.name(), voice.id(),);
  }

  for voice in voices.into_iter() {
   let text = match voice.language().to_string().to_lowercase().as_str() {
    // ChatGPTに使用人口の多そうな順に30種類作ってもらった。これ以上は要望があってから考える。
    "zh-cn" => format!("我是{}，如果您想要这样的语音，请选择我。你想吃点心吗？", voice.name()),
    "zh-tw" => format!("我是{}，如果您想要這樣的語音，請選擇我。你想吃鳳梨酥嗎？", voice.name()),
    es if es.starts_with("es") => format!("Soy {} y si deseas esta voz, elige la mía. ¿Quieres comer tortillas?", voice.name()),
    en if en.starts_with("en") => format!(
     "I am {}, if you want this voice, choose me. Do you want to eat meat pie?",
     voice.name()
    ),
    "hi-in" => format!(
     "मैं हूं {} और अगर आपको यह आवाज़ चाहिए तो मेरा चयन करें। क्या आप मसाला चाय पीते हैं?",
     voice.name()
    ),
    "ar-sa" => format!("أنا {}، إذا كنت ترغب في هذا الصوت، اخترني. هل تريد أن تأكل الحمص؟", voice.name()),
    "bn-bd" => format!("আমি {} এবং আপনি যদি এই ভয়েসটি চান তবে আমাকে চয়ন করুন।", voice.name()),
    "ru-ru" => format!("Я {} и если вам нужен такой голос, выберите меня. Вы пьете водку?", voice.name()),
    "pt-br" => format!("Eu sou {} e se você quiser esta voz, escolha-me.", voice.name()),
    "ja-jp" => format!("私は{}、このような音声を使いたい場合は選んでくれ。お寿司食べる？", voice.name()),
    "pa-in" => format!("ਮੈਂ {} ਹਾਂ ਅਤੇ ਜੇਕਰ ਤੁਸੀਂ ਇਸ ਆਵਾਜ਼ ਦੀ ਇਚਛਾ ਕਰਦੇ ਹੋ ਤਾਂ ਮੈਨੂੰ ਚੁਣੋ।", voice.name()),
    "de-de" => format!(
     "Ich bin {} und wenn du diese Stimme möchtest, wähle mich. Sollte Ihr Bier Schwartz sein?",
     voice.name()
    ),
    "fr-fr" => format!(
     "Je suis {} et si vous voulez cette voix, choisissez-moi. Tu veux manger de la galette ?",
     voice.name()
    ),
    "ko-kr" => format!("나는 {} 이며, 이 음성을 원하면 나를 선택하십시오. 순두부 먹어?", voice.name()),
    "tr-tr" => format!("Ben {}'yim ve bu sesi istersen beni seç.", voice.name()),
    "it-it" => format!(
     "Sono {} e se vuoi questa voce, scegli me. Che tipo di pasta utilizzare?",
     voice.name()
    ),
    "uk-ua" => format!(
     "Я {} і якщо вам потрібен саме такий голос, оберіть мене. Хочеш їсти борщ?",
     voice.name()
    ),
    "vi-vn" => format!("Tôi là {} và nếu bạn muốn giọng nói này, hãy chọn tôi.", voice.name()),
    "fa-ir" => format!("من {} هستم و اگر می‌خواهید این صدا را داشته باشید، من را انتخاب کنید.", voice.name()),
    "th-th" => format!("ฉันคือ {} และหากคุณต้องการเสียงนี้ โปรดเลือกฉัน. อยากกินต้มยำกุ้งมั้ย?", voice.name()),
    "pl-pl" => format!("Jestem {} i jeśli chcesz ten głos, wybierz mnie.", voice.name()),
    "ro-ro" => format!("Sunt {} și dacă vrei această voce, alege-mă.", voice.name()),
    "nl-nl" => format!("Ik ben {} en als je deze stem wilt, kies dan voor mij.", voice.name()),
    "el-gr" => format!("Είμαι {} και αν θέλετε αυτήν τη φωνή, επιλέξτε με.", voice.name()),
    "hu-hu" => format!("Én vagyok {} és ha ezt a hangot szeretnéd, válassz engem.", voice.name()),
    "cs-cz" => format!("Jsem {} a pokud chcete tento hlas, vyberte si mě.", voice.name()),
    "sv-se" => format!("Jag är {} och om du vill ha den här rösten, välj mig.", voice.name()),
    "pt-pt" => format!("Sou {} e se quiseres esta voz, escolhe-me.", voice.name()),
    "id-id" => format!("Saya {} dan jika Anda ingin suara ini, pilih saya.", voice.name()),
    "ms-my" => format!("Saya {} dan jika anda mahu suara ini, pilih saya.", voice.name()),
    lang => {
     log::warn!("未知の言語コードです: {}", lang);
     "".to_string()
    },
   };
   tts.set_voice(&voice)?;

   #[cfg(not(target_os = "windows"))]
   {
    tts.speak(text, false)?;
   }
   #[cfg(target_os = "windows")]
   {
    let wav = tts.synthesize(text)?;
    let cursor = std::io::Cursor::new(wav);
    let source = rodio::Decoder::new(cursor)?;
    audio_sink.lock().await.0.append(source);
   }
  }

  #[cfg(not(target_os = "windows"))]
  while tts.is_speaking()? {
   log::warn!("OS-TTS がまだ話しています。");
   tokio::time::sleep(std::time::Duration::from_millis(100)).await;
  }
  #[cfg(target_os = "windows")]
  audio_sink.lock().await.0.sleep_until_end();
  log::info!("OS-TTS のテストが完了しました。");

  #[cfg(target_os = "windows")]
  {
   log::info!("Windows 11 をお使いの場合は「時刻と言語」>「音声認識」から音声を追加または削除できます。");
  }

  Ok(())
 }
}
