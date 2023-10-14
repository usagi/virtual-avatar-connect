use crate::SharedAudioSink;
use anyhow::Result;
use clap::Parser;

const DEFAULT_CONF_PATH: &str = "conf.toml";

#[derive(Parser, Debug)]
#[command(version, about, long_about = None)]
pub struct Args {
 #[arg(name = "CONF", default_value = DEFAULT_CONF_PATH)]
 pub conf: String,
 #[arg(short = 'D', long)]
 pub debug: bool,
 #[arg(long)]
 pub coeiroink_speakers: bool,
 #[arg(long)]
 pub test_os_tts: bool,
 #[arg(long)]
 pub experimental: bool,
}

impl Args {
 pub async fn init(audio_sink: SharedAudioSink) -> Result<Args> {
  log::info!("コマンドライン引数をパースします。");
  let args = Args::parse();
  // args.debug が true ならログレベルを Trace に設定
  match args.debug {
   true => {
    log::set_max_level(log::LevelFilter::Trace);
    log::warn!("コマンドライン引数で -D/--debug が指定されたためログレベルが Trace に設定されます。");
    log::trace!("コマンドライン引数のパース結果: {:?}", args);
   },
   false => log::set_max_level(log::LevelFilter::Info),
  }

  if args.coeiroink_speakers {
   log::info!("CoeiroInk の Speakers を表示します。CoeiroInkのAPIが動作していない場合はエラーが発生します。");
   let speakers = crate::processor::CoeiroInk::get_speakers().await?;
   println!("--- CoeiroInk Speakers ---");
   for speaker in speakers {
    println!(" * speakerName/Uuid: {} / {}", speaker.speakerName, speaker.speakerUuid);
    for style in speaker.styles {
     println!("  - styleName/Id: {} / {}", style.styleName, style.styleId);
    }
   }

   std::process::exit(0);
  }

  if args.test_os_tts {
   crate::processor::OsTts::test(audio_sink).await?;
   std::process::exit(0);
  }

  if args.experimental {
   log::warn!("実験的な機能が有効になっています。");
   {
    // println!("img1.png --> {}", win_ocr::ocr_with_lang("img1.png", "ja").unwrap());
    // println!("img2.png --> {}", win_ocr::ocr_with_lang("img2.png", "ja").unwrap());
    // println!("img3.png --> {}", win_ocr::ocr_with_lang("img3.png", "ja").unwrap());
    // println!("img4.png --> {}", win_ocr::ocr_with_lang("img4.png", "ja").unwrap());
   }
   std::process::exit(0);
  }

  Ok(args)
 }
}
