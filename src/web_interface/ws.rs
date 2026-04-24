use crate::{Arc, ChannelData, ChannelDatum, RwLock, SharedChannelData, SharedState};

use actix::{Actor, ActorContext, AsyncContext, Handler, Message, StreamHandler};
use actix_web::{web, HttpRequest, HttpResponse};
use actix_web_actors::ws;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use tokio::time::Duration;

const DEFAULT_PUSH_INTERVAL: Duration = Duration::from_millis(100);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsServerPayloadChannelDatum {
 pub channel: String,
 pub content: String,
 #[serde(default)]
 pub flags: HashSet<String>,
 pub id: Option<u64>,
 pub datetime: Option<Timestamp>,
}

impl WsServerPayloadChannelDatum {
 /// Server --> Client プッシュ配信用
 pub fn from_channel_datum(cd: &ChannelDatum) -> Self {
  Self {
   channel: cd.channel.clone(),
   content: cd.content.clone(),
   flags: cd.flags.clone(),
   id: Some(cd.get_id()),
   datetime: Some(cd.get_datetime()),
  }
 }
 /// Client --> Server 入力受信用
 pub fn to_channel_datum(self) -> ChannelDatum {
  let mut cd = ChannelDatum::new(self.channel, self.content);
  for f in self.flags.into_iter() {
   cd = cd.with_flag(&f);
  }
  cd
 }
}

pub type WsServerPayloadChannelData = VecDeque<WsServerPayloadChannelDatum>;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WsServerPayload {
 pub channel_data: Option<WsServerPayloadChannelData>,
 pub channel_datum: Option<WsServerPayloadChannelDatum>,
}

#[derive(Clone, Debug)]
pub struct WebSocketServer {
 pub state: SharedState,
 pub channel_data: SharedChannelData,
 pub last_retrieved_id: Arc<RwLock<u64>>,
 /// 同一 `ChannelDatum` id の `content` / `flags` 更新を配信する（Vosk 部分認識・OpenAI ストリーム等）
 pub last_sent_fingerprint: Arc<RwLock<HashMap<u64, (String, HashSet<String>)>>>,
 pub client: Option<actix::Addr<WebSocketServer>>,
}

#[derive(Message)]
#[rtype(result = "()")]
pub struct StringMessage(pub String);

impl Handler<StringMessage> for WebSocketServer {
 type Result = ();

 fn handle(&mut self, msg: StringMessage, ctx: &mut Self::Context) {
  ctx.text(msg.0);
 }
}

impl WebSocketServer {
 pub async fn new(state: &SharedState) -> Self {
  Self {
   state: state.clone(),
   channel_data: state.read().await.channel_data.clone(),
   last_retrieved_id: Arc::new(RwLock::new(ChannelDatum::get_last_id())),
   last_sent_fingerprint: Arc::new(RwLock::new(HashMap::new())),
   client: None,
  }
 }
}

impl Actor for WebSocketServer {
 type Context = ws::WebsocketContext<Self>;

 fn started(&mut self, ctx: &mut Self::Context) {
  log::debug!("WebSocket が開始されました。");
  let channel_data = self.channel_data.clone();
  let last_retrieved_id = self.last_retrieved_id.clone();
  let last_sent_fingerprint = self.last_sent_fingerprint.clone();
  let addr = ctx.address();
  ctx.spawn(actix::fut::wrap_future(async move {
   // Server --> Client プッシュ配信（新規 id に加え、同一 id の内容更新も送る）
   loop {
    let last_retrieved_id_read_only = *last_retrieved_id.read().await;
    let mut max_new_id = last_retrieved_id_read_only;
    let mut data: VecDeque<WsServerPayloadChannelDatum> = VecDeque::new();

    {
     let channel_data = channel_data.read().await;
     let mut fp = last_sent_fingerprint.write().await;
     for cd in channel_data.iter() {
      let id = cd.get_id();
      let fingerprint = (cd.content.clone(), cd.flags.clone());
      let is_new_id = id > last_retrieved_id_read_only;
      let changed = fp.get(&id).map(|old| old != &fingerprint).unwrap_or(false);
      if is_new_id {
       data.push_back(WsServerPayloadChannelDatum::from_channel_datum(cd));
       max_new_id = max_new_id.max(id);
       fp.insert(id, fingerprint);
      } else if changed {
       data.push_back(WsServerPayloadChannelDatum::from_channel_datum(cd));
       fp.insert(id, fingerprint);
      }
     }
     let valid_ids: HashSet<u64> = channel_data.iter().map(|c| c.get_id()).collect();
     fp.retain(|k, _| valid_ids.contains(k));
    }

    if !data.is_empty() {
     {
      let mut lw = last_retrieved_id.write().await;
      *lw = (*lw).max(max_new_id);
     }
     let json = serde_json::to_string(&WsServerPayload {
      channel_data: Some(data),
      channel_datum: None,
     })
     .unwrap();
     addr.send(StringMessage(json)).await.unwrap();
    }
    tokio::time::sleep(DEFAULT_PUSH_INTERVAL).await;
   }
  }));
 }

 fn stopped(&mut self, _ctx: &mut Self::Context) {
  log::debug!("WebSocket が停止されました。");
 }
}

impl StreamHandler<actix_web::Result<ws::Message, ws::ProtocolError>> for WebSocketServer {
 fn handle(&mut self, msg: actix_web::Result<ws::Message, ws::ProtocolError>, ctx: &mut Self::Context) {
  match msg {
   Ok(ws::Message::Text(text)) => {
    log::trace!("WebSocket がテキストを受信しました。 {:?}", text);

    if let Ok(ws_server_payload) = serde_json::from_str::<WsServerPayload>(&text) {
     if let Some(ws_channel_datum) = ws_server_payload.channel_datum {
      let channel_datum = ws_channel_datum.to_channel_datum();
      log::trace!("channel_datum = {:?}", channel_datum);
      let state = self.state.clone();
      tokio::spawn(async move {
       state.read().await.push_channel_datum(channel_datum).await;
      });
     } else if let Some(ws_channel_data) = ws_server_payload.channel_data {
      let channel_data = ws_channel_data.into_iter().map(|cd| cd.to_channel_datum()).collect::<ChannelData>();
      log::trace!("channel_data = {:?}", channel_data);
      let state = self.state.clone();
      tokio::spawn(async move {
       state.read().await.push_channel_data(channel_data).await;
      });
     }
    }
   },
   Ok(ws::Message::Binary(bin)) => {
    log::trace!("WebSocket がバイナリを受信しました。 {:?}", bin);
   },
   Ok(ws::Message::Close(close_data)) => {
    log::debug!("WebSocket がクローズを受信しました。 {:?}", close_data);
    ctx.close(close_data);
    ctx.stop();
   },
   Err(err) => {
    log::error!("WebSocket がエラーを受信しました。 {:?}", err);
    ctx.stop();
   },
   _ => log::warn!("WebSocket がその他のメッセージを受信しました。"),
  }
 }
}

#[actix_web::get("/")]
pub async fn websocket(
 r: HttpRequest,
 stream: web::Payload,
 state: web::Data<SharedState>,
) -> actix_web::Result<HttpResponse, actix_web::Error> {
 log::debug!("WebSocket が接続されました。");
 let s = WebSocketServer::new(&state).await;
 ws::start(s, &r, stream)
}
