mod args;
mod conf;
mod error;
mod logger;
mod processor;
mod resource;
mod state;

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
use actix_web::web;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

pub async fn run() -> Result<()> {
 // ロガーの実装を初期化
 logger::init();

 // コマンドライン引数をパースし、ログレベルを更新
 let args = Args::init().await?;
 // 設定を読み込みし、ログレベルを更新
 let conf = Conf::new(&args)?;
 // 共有ステートを作成
 let state = State::new(&conf).await?;

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
   .service(web_interface::input::post)
   .service(web::redirect("/input/", "/input"))
   .service(Files::new("/input", "input").index_file("index.html"))
   .service(web_interface::output::post)
   .service(web::redirect("/output/", "/output"))
   .service(Files::new("/output", "output").index_file("index.html"))
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
