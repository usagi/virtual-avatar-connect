//! Phase M3: VMC passthrough の状態 API。

use actix_web::web::{self, Data, Json, Path};
use actix_web::{get, post, HttpResponse, Responder};
use serde::Deserialize;

use crate::conf::VmcPassthroughSpec;
use crate::SharedState;

#[derive(Debug, Deserialize)]
pub struct VmcForwardRequest {
	pub dest: String,
}

#[derive(Debug, Deserialize)]
pub struct VmcBindRequest {
	pub bind: String,
	#[serde(default)]
	pub forward_to: Vec<String>,
	#[serde(default)]
	pub label: Option<String>,
}

#[get("/vmc/status")]
pub async fn get_vmc_status(state: Data<SharedState>) -> impl Responder {
	let status = {
		let s = state.read().await;
		s.vmc_passthrough_status.clone()
	};
	HttpResponse::Ok().json(status.snapshot())
}

#[post("/vmc/bind")]
pub async fn post_vmc_bind(state: Data<SharedState>, body: Json<VmcBindRequest>) -> impl Responder {
	let req = body.into_inner();
	if let Err(e) = validate_bind_request(&req) {
		return HttpResponse::BadRequest().json(serde_json::json!({
			"error": "invalid_vmc_bind_request",
			"detail": e,
		}));
	}
	let (registry, shutdown) = {
		let s = state.read().await;
		(s.vmc_passthrough_status.clone(), s.shutdown.clone())
	};
	let spec = VmcPassthroughSpec {
		enabled: true,
		bind: req.bind,
		forward_to: req.forward_to,
		label: req.label,
	};
	match crate::motion::spawn_vmc_passthrough_route(spec, shutdown, registry).await {
		Ok(view) => HttpResponse::Accepted().json(view),
		Err(e) => HttpResponse::Conflict().json(serde_json::json!({
			"error": "vmc_bind_failed",
			"detail": e,
		})),
	}
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
		.service(post_vmc_bind)
		.service(post_vmc_forward_add)
		.service(post_vmc_forward_remove);
}

fn validate_bind_request(req: &VmcBindRequest) -> Result<(), String> {
	req.bind
		.trim()
		.parse::<std::net::SocketAddr>()
		.map_err(|e| format!("bind {:?}: {}", req.bind, e))?;
	if req.forward_to.is_empty() {
		return Err("forward_to は 1 件以上必要です".to_string());
	}
	for (index, dest) in req.forward_to.iter().enumerate() {
		dest.trim()
			.parse::<std::net::SocketAddr>()
			.map_err(|e| format!("forward_to[{index}] {:?}: {e}", dest))?;
	}
	Ok(())
}
