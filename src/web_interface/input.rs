use crate::{resource::CONTENT_TYPE_APPLICATION_JSON, ChannelDatum, Result, SharedState};
use actix_web::{post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct InputPayload {
 /// チャンネル名
 channel: String,
 /// 内容
 content: String,
 /// 入力途中なら false, 確定済みで内容が変化しないなら true
 is_final: bool,
}

#[post("/input")]
async fn post(state: web::Data<SharedState>, payload: web::Json<InputPayload>) -> Result<impl Responder> {
 log::trace!("入力インターフェースからの入力を受け取りました: {:?}", payload);

 let state = state.get_ref();

 // コマンド処理
 process_command(&state, &payload).await?;

 let cd = ChannelDatum::new(payload.channel.clone(), payload.content.clone()).with_flag_if(ChannelDatum::FLAG_IS_FINAL, payload.is_final);
 log::trace!("ChannelDatum を生成しました: {:?}", cd);
 state.read().await.push_channel_datum(cd).await;
 log::trace!("state.push_channel_datum(cd).await が完了しました。");

 Ok(HttpResponse::Ok().content_type(CONTENT_TYPE_APPLICATION_JSON).json(payload))
}

async fn process_command(state: &SharedState, payload: &InputPayload) -> Result<()> {
 if payload.content.starts_with("/quit") {
  log::info!("終了コマンド /quit を受け取りました。");
  std::process::exit(0);
 } else if payload.content.starts_with("/save") {
  state.read().await.save().await?;
 } else if payload.content.starts_with("/load") {
  state.write().await.load().await?;
 }
 Ok(())
}

#[actix_web::get("/input")]
pub async fn get_index() -> Result<impl Responder> {
 // ./input/index.html を読み込む
 let path = Path::new("./input/index.html");
 match tokio::fs::read_to_string(path).await {
  Ok(content) => Ok(HttpResponse::Ok().content_type("text/html").body(content)),
  _ => Ok(HttpResponse::NotFound().finish()),
 }
}

#[actix_web::get("/input/{subindex}")]
pub async fn get_subfile(subindex: web::Path<String>) -> Result<impl Responder> {
 // ./input/{subindex}.html を読み込む
 let path = Path::new("./input/").join(subindex.as_str()).with_extension("html");
 match tokio::fs::read_to_string(path).await {
  Ok(content) => Ok(HttpResponse::Ok().content_type("text/html").body(content)),
  _ => Ok(HttpResponse::NotFound().finish()),
 }
}
