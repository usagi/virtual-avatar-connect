//! Phase M3: VMC passthrough の状態 API。

use actix_web::web::{self, Data, Json, Path};
use actix_web::{get, post, HttpResponse, Responder};
use serde::Deserialize;

use crate::SharedState;

#[derive(Debug, Deserialize)]
pub struct VmcForwardRequest {
	pub dest: String,
}

#[get("/vmc/status")]
pub async fn get_vmc_status(state: Data<SharedState>) -> impl Responder {
	let status = {
		let s = state.read().await;
		s.vmc_passthrough_status.clone()
	};
	HttpResponse::Ok().json(status.snapshot())
}

#[post("/vmc/{id}/forward/add")]
pub async fn post_vmc_forward_add(state: Data<SharedState>, id: Path<String>, body: Json<VmcForwardRequest>) -> impl Responder {
	edit_forward(state, id.into_inner(), body.into_inner(), true).await
}

#[post("/vmc/{id}/forward/remove")]
pub async fn post_vmc_forward_remove(state: Data<SharedState>, id: Path<String>, body: Json<VmcForwardRequest>) -> impl Responder {
	edit_forward(state, id.into_inner(), body.into_inner(), false).await
}

async fn edit_forward(state: Data<SharedState>, id: String, body: VmcForwardRequest, add: bool) -> HttpResponse {
	let registry = {
		let s = state.read().await;
		s.vmc_passthrough_status.clone()
	};
	let Some(entry) = registry.find(&id) else {
		return HttpResponse::NotFound().json(serde_json::json!({
			"error": "vmc_route_not_found",
			"id": id,
		}));
	};
	if !entry.is_running() {
		return HttpResponse::Conflict().json(serde_json::json!({
			"error": "vmc_route_not_running",
			"id": id,
			"reason": "runtime forward 変更は running route のみ対象です",
		}));
	}
	let result = if add {
		entry.add_forward(&body.dest)
	} else {
		entry.remove_forward(&body.dest)
	};
	match result {
		Ok(view) => HttpResponse::Ok().json(view),
		Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
			"error": "invalid_forward_destination",
			"id": id,
			"detail": e,
		})),
	}
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(get_vmc_status)
		.service(post_vmc_forward_add)
		.service(post_vmc_forward_remove);
}
