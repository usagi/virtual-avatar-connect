use crate::{resource::CONTENT_TYPE_APPLICATION_JSON, Result, SharedState};
use actix_web::{post, web, HttpResponse, Responder};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::path::Path;

#[derive(Deserialize, Debug)]
struct OutputRequestPayload {
 /// 受信したいチャンネルのリスト
 channels: Vec<ChannelRequest>,
}

#[derive(Deserialize, Debug)]
struct ChannelRequest {
 /// 受信したいチャンネル名
 name: String,
 /// 既に取得済みの ID を与えると、その ID 以降の内容を取得します。
 retrieved_id: Option<u64>,
 /// 既に取得済みの iso8601 日時を与えると、その日時以降の内容を取得します。
 retrieved_timestamp: Option<String>,
 /// 取得する最大件数を指定します。
 count: Option<usize>,
}

#[derive(Serialize, Debug)]
struct OutputResponsePayload {
 /// チャンネル名 -> チャンネルデータ
 channel_data: HashMap<String, Vec<ChannelDatum>>,
}

#[derive(Serialize, Debug)]
struct ChannelDatum {
 /// Datum ID
 id: Option<u64>,
 /// ISO8601 日時
 datetime: Option<String>,
 /// チャンネル名
 channel: Option<String>,
 /// 内容
 content: Option<String>,
 /// フラグ
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

#[actix_web::get("/output")]
pub async fn get_index() -> Result<impl Responder> {
 // ./output/index.html を読み込む
 let path = Path::new("./output/index.html");
 match tokio::fs::read_to_string(path).await {
  Ok(content) => Ok(HttpResponse::Ok().content_type("text/html").body(content)),
  _ => Ok(HttpResponse::NotFound().finish()),
 }
}

#[actix_web::get("/output/{subindex}")]
pub async fn get_subfile(subindex: web::Path<String>) -> Result<impl Responder> {
 // ./output/{subindex}.html を読み込む
 let path = Path::new("./output/").join(subindex.as_str()).with_extension("html");
 match tokio::fs::read_to_string(path).await {
  Ok(content) => Ok(HttpResponse::Ok().content_type("text/html").body(content)),
  _ => Ok(HttpResponse::NotFound().finish()),
 }
}
