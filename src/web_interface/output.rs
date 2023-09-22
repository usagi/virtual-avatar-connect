use crate::{resource::CONTENT_TYPE_APPLICATION_JSON, Result, SharedState};
use actix_web::{post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

#[derive(Deserialize, Debug)]
struct OutputRequestPayload {
 channels: Vec<ChannelRequest>,
}

#[derive(Deserialize, Debug)]
struct ChannelRequest {
 name: String,
 retrieved_id: Option<u64>,
 retrieved_timestamp: Option<String>,
 count: Option<usize>,
}

#[derive(Serialize, Debug)]
struct OutputResponsePayload {
 /// name -> channel_data
 channel_data: HashMap<String, Vec<ChannelDatum>>,
}

#[derive(Serialize, Debug)]
struct ChannelDatum {
 id: Option<u64>,
 datetime: Option<String>,
 channel: Option<String>,
 content: Option<String>,
 #[serde(default)]
 flags: HashSet<String>,
}

#[post("/output")]
async fn post(state: web::Data<SharedState>, request_paylaod: web::Json<OutputRequestPayload>) -> Result<impl Responder> {
 // 出力の構造を作成
 let mut output_response = OutputResponsePayload {
  channel_data: HashMap::new(),
 };

 // state から channel_data を取得する
 let state = state.get_ref().read().await;
 let channel_data = state.channel_data.read().await;

 // request_payload から channel_request を1つずつ取り出して、それぞれに対応する channel_data を取得する
 for channel_request in request_paylaod.channels.iter() {
  let mut channel_data_for_response = Vec::new();

  // channel_data から channel_request に対応する channel_data を取得する
  let channel_data_for_request = channel_data
   .iter()
   .filter(|cd| cd.channel == channel_request.name)
   .collect::<Vec<_>>();

  // count が指定されている場合は、後ろから count 個の channel_datum まで channel_data_for_request を進める
  let channel_data_for_request = match channel_request.count {
   Some(count) => channel_data_for_request.into_iter().rev().take(count).rev().collect::<Vec<_>>(),
   None => channel_data_for_request,
  };

  // retrieved_id が指定されている場合は、それより大きなIDの channel_datum まで channel_data_for_request を進める
  let channel_data_for_request = match channel_request.retrieved_id {
   Some(retrieved_id) => channel_data_for_request
    .into_iter()
    .filter(|cd| cd.get_id() > retrieved_id)
    .collect::<Vec<_>>(),
   None => channel_data_for_request,
  };

  // retrieved_timestamp が指定されている場合は、それより新しい timestamp の channel_datum まで channel_data_for_request を進める
  let channel_data_for_request = match &channel_request.retrieved_timestamp {
   Some(retrieved_timestamp) => channel_data_for_request
    .into_iter()
    .filter(|cd| cd.get_datetime() > retrieved_timestamp.parse::<chrono::DateTime<chrono::Utc>>().unwrap())
    .collect::<Vec<_>>(),
   None => channel_data_for_request,
  };

  // channel_data_for_request を channel_data_for_response にコピーする
  for cd in channel_data_for_request {
   channel_data_for_response.push(ChannelDatum {
    id: Some(cd.get_id()),
    datetime: Some(cd.get_datetime().to_rfc3339()),
    channel: Some(cd.channel.clone()),
    content: Some(cd.content.clone()),
    flags: cd.flags.clone(),
   });
  }

  // 出力の構造に channel_data_for_response を追加する
  output_response
   .channel_data
   .insert(channel_request.name.clone(), channel_data_for_response);
 }

 Ok(HttpResponse::Ok().content_type(CONTENT_TYPE_APPLICATION_JSON).json(output_response))
}
