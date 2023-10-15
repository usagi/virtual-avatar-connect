mod args;
mod conf;
mod error;
mod logger;
mod processor;
mod resource;
mod state;

pub mod utility;
pub mod web_interface;

pub use crate::{
 args::Args,
 conf::Conf,
 conf::*,
 error::{Error, Result},
 processor::*,
 state::{ChannelData, ChannelDatum, SharedChannelData, SharedState, State},
};

use actix_files::Files;
use rodio::{OutputStream, Sink};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub struct AudioSink(pub Sink);
pub type SharedAudioSink = Arc<Mutex<AudioSink>>;
impl std::fmt::Debug for AudioSink {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  write!(f, "AudioSink")
 }
}

pub async fn run() -> Result<()> {
 // ロガーの実装を初期化
 logger::init();

 // 音声再生用の handle を生成
 // Sink は Shared 化できるが、 steam_handle は Shared 化できないため、
 // ここで actix_web のサービスが .await を抜けるまで保持してしまう。
 let (_stream, stream_handle) = OutputStream::try_default()?;
 let audio_sink = Sink::try_new(&stream_handle)?;
 let audio_sink = Arc::new(Mutex::new(AudioSink(audio_sink)));

 // コマンドライン引数をパースし、ログレベルを更新
 let args = Args::new();
 // conf を必要としない特殊な動作モードを実行
 args.execute_special_modes_without_conf(audio_sink.clone()).await?;
 // 設定を読み込みし、ログレベルを更新
 let conf = Conf::new(&args)?;
 // conf を必要とする特殊な動作モードを実行
 args.execute_special_modes_with_conf(&conf).await?;

 // run_with 機能の実行
 conf.execute_run_with()?;

 // 共有ステートを作成
 let state = State::new(&conf, audio_sink).await?;

 // 通常の動作モードで実行
 run_services(conf, state).await?;

 Ok(())
}

async fn run_services(conf: Conf, state: SharedState) -> Result<()> {
 let workers = conf.get_workers();
 let web_ui_address = conf.get_web_ui_address().to_string();
 actix_web::HttpServer::new(move || {
  let state = state.clone();
  let conf = conf.clone();
  let app = actix_web::App::new()
   .app_data(actix_web::web::Data::new(state))
   .service(web_interface::websocket)
   .service(web_interface::input::post)
   .service(web_interface::input::get_index)
   .service(web_interface::input::get_subfile)
   .service(web_interface::output::post)
   .service(web_interface::output::get_index)
   .service(web_interface::output::get_subfile)
   .service(web_interface::status::get)
   .service(web_interface::favicon);
  if let Some(web_ui_resources_path) = conf.web_ui_resources_path {
   app.service(Files::new("/resources", web_ui_resources_path))
  } else {
   app
  }
 })
 .workers(workers)
 .bind(web_ui_address)?
 .run()
 .await?;

 Ok(())
}
