use super::Args;
use crate::SharedAudioSink;
use anyhow::Result;

impl Args {
 /// conf 不要の特殊モード処理群の実行
 pub async fn execute_special_modes_without_conf(&self, audio_sink: SharedAudioSink) -> Result<()> {
  // debug が true ならログレベルを Trace に設定
  match self.debug {
   true => {
    log::set_max_level(log::LevelFilter::Trace);
    log::warn!("コマンドライン引数で -D/--debug が指定されたためログレベルが Trace に設定されます。");
    log::trace!("コマンドライン引数のパース結果: {:?}", self);
   },
   false => log::set_max_level(log::LevelFilter::Info),
  }

  if self.coeiroink_speakers {
   log::info!("CoeiroInk の Speakers を表示します。CoeiroInkのAPIが動作していない場合はエラーが発生します。");
   let speakers = match crate::processor::CoeiroInk::get_speakers().await {
    Ok(speakers) => speakers,
    Err(e) => {
     log::error!("CoeiroInk の Speakers の取得に失敗しました: {}", e);
     std::process::exit(1);
    },
   };
   println!("--- CoeiroInk Speakers ---");
   for speaker in speakers {
    println!(" * speakerName/Uuid: {} / {}", speaker.speakerName, speaker.speakerUuid);
    for style in speaker.styles {
     println!("  - styleName/Id: {} / {}", style.styleName, style.styleId);
    }
   }

   std::process::exit(0);
  }

  if self.test_os_tts {
   if let Err(e) = crate::processor::OsTts::test(audio_sink).await {
    log::error!("OS-TTS のテストに失敗しました: {}", e);
    std::process::exit(1);
   }
   std::process::exit(0);
  }

  Ok(())
 }
}
