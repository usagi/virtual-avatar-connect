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
 pub async fn init() -> Result<Args> {
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
   for voice in voices.iter() {
    println!(" * Name/ID: {} / {}", voice.name(), voice.id(),);
   }

   for voice in voices.into_iter() {
    let text = match voice.language().to_lowercase().as_str() {
     // ChatGPTに使用人口の多そうな順に30種類作ってもらった。これ以上は要望があってから考える。
     "zh-cn" => format!("我是{}，如果您想要这样的语音，请选择我。", voice.name()),
     "zh-tw" => format!("我是{}，如果您想要這樣的語音，請選擇我。", voice.name()),
     "es-es" => format!("Soy {} y si deseas esta voz, elige la mía.", voice.name()),
     en if en.starts_with("en") => format!("I am {}, if you want this voice, choose me.", voice.name()),
     "hi-in" => format!("मैं हूं {} और अगर आपको यह आवाज़ चाहिए तो मेरा चयन करें।", voice.name()),
     "ar-sa" => format!("أنا {}، إذا كنت ترغب في هذا الصوت، اخترني.", voice.name()),
     "bn-bd" => format!("আমি {} এবং আপনি যদি এই ভয়েসটি চান তবে আমাকে চয়ন করুন।", voice.name()),
     "ru-ru" => format!("Я {} и если вам нужен такой голос, выберите меня.", voice.name()),
     "pt-br" => format!("Eu sou {} e se você quiser esta voz, escolha-me.", voice.name()),
     "ja-jp" => format!("私は{}、このような音声を使いたい場合は選んでくれ。", voice.name()),
     "pa-in" => format!("ਮੈਂ {} ਹਾਂ ਅਤੇ ਜੇਕਰ ਤੁਸੀਂ ਇਸ ਆਵਾਜ਼ ਦੀ ਇਚਛਾ ਕਰਦੇ ਹੋ ਤਾਂ ਮੈਨੂੰ ਚੁਣੋ।", voice.name()),
     "de-de" => format!("Ich bin {} und wenn du diese Stimme möchtest, wähle mich.", voice.name()),
     "fr-fr" => format!("Je suis {} et si vous voulez cette voix, choisissez-moi.", voice.name()),
     "ko-kr" => format!("나는 {} 이며, 이 음성을 원하면 나를 선택하십시오.", voice.name()),
     "tr-tr" => format!("Ben {}'yim ve bu sesi istersen beni seç.", voice.name()),
     "it-it" => format!("Sono {} e se vuoi questa voce, scegli me.", voice.name()),
     "uk-ua" => format!("Я {} і якщо вам потрібен саме такий голос, оберіть мене.", voice.name()),
     "vi-vn" => format!("Tôi là {} và nếu bạn muốn giọng nói này, hãy chọn tôi.", voice.name()),
     "fa-ir" => format!("من {} هستم و اگر می‌خواهید این صدا را داشته باشید، من را انتخاب کنید.", voice.name()),
     "th-th" => format!("ฉันคือ {} และหากคุณต้องการเสียงนี้ โปรดเลือกฉัน.", voice.name()),
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
    tts.speak(text, false)?;
   }

   while tts.is_speaking()? {
    log::warn!("OS-TTS がまだ話しています。");
    tokio::time::sleep(std::time::Duration::from_millis(100)).await;
   }
   log::error!("OS-TTS のテストが完了しました。");

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
