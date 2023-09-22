use crate::{resource::CONTENT_TYPE_APPLICATION_JSON, ChannelDatum, Result, SharedState};
use actix_web::{post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
struct InputPayload {
 channel: String,
 content: String,
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
