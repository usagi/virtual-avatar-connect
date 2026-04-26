//! `POST /ingress` ハンドラ。

use actix_web::web::{Data, Json};
use actix_web::{post, HttpResponse, Responder};

use crate::state::DataSource;
use crate::{ChannelDatum, SharedState};

use super::IngressRequest;
use super::IngressResponse;

#[post("/ingress")]
pub async fn post_ingress(state: Data<SharedState>, body: Json<IngressRequest>) -> impl Responder {
	let req = body.into_inner();

	let channel = req.channel.trim();
	if channel.is_empty() {
		return HttpResponse::BadRequest().json(serde_json::json!({
		 "error": "bad_request",
		 "reason": "channel is required and must not be empty",
		}));
	}

	let mut cd = ChannelDatum::new(channel.to_string(), req.content);
	let is_final = req.is_final.unwrap_or(true);
	cd = cd.with_flag_if(ChannelDatum::FLAG_IS_FINAL, is_final);

	if let Some(flags) = req.flags {
		for f in flags {
			let f = f.trim();
			if !f.is_empty() {
				cd.flags.insert(f.to_string());
			}
		}
	}

	cd.source = req.source.or_else(|| Some(DataSource::new("control.ingress").with_actor("gui")));

	if let Some(meta) = req.meta {
		cd.meta = meta;
	}

	let id = cd.get_id();
	let channel_out = cd.channel.clone();
	log::info!(
		"《ControlAPI/ingress》 投入: id={} channel={} content_len={} flags={:?} meta_keys={:?}",
		id,
		channel_out,
		cd.content.len(),
		cd.flags,
		cd.meta.keys().collect::<Vec<_>>(),
	);
	state.read().await.push_channel_datum(cd).await;

	HttpResponse::Ok().json(IngressResponse {
		id,
		channel: channel_out,
		accepted: true,
	})
}
