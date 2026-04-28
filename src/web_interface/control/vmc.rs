//! Phase M3: VMC passthrough の状態 API。

use actix_web::web::{self, Data};
use actix_web::{get, HttpResponse, Responder};

use crate::SharedState;

#[get("/vmc/status")]
pub async fn get_vmc_status(state: Data<SharedState>) -> impl Responder {
	let status = {
		let s = state.read().await;
		s.vmc_passthrough_status.clone()
	};
	HttpResponse::Ok().json(status.snapshot())
}

pub fn configure(cfg: &mut web::ServiceConfig) {
	cfg.service(get_vmc_status);
}
