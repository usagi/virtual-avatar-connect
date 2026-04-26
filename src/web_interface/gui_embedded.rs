//! `/gui/*` をメモリから返す（feature `embed-gui`）。バイト列は **`vac_gui_assets`** クレートが `include_dir` する。
//!
//! 既定ビルドでは無効。リリース同梱用に `cargo build --release --features embed-gui`（事前に `cd gui && npm ci && npm run build`）。

use actix_web::{HttpRequest, HttpResponse, Responder};

/// `/gui` および `/gui/*` を内蔵静的で配信する。
pub fn register(cfg: &mut actix_web::web::ServiceConfig) {
	log::info!("《GUI》 gui/dist をバイナリ内蔵（feature embed-gui）で /gui/ に配信します。");
	cfg.service(actix_web::web::resource("/gui{tail:.*}").route(actix_web::web::get().to(serve_embedded)));
}

async fn serve_embedded(req: HttpRequest) -> impl Responder {
	let Some(rel) = super::gui_path::path_under_gui(req.path()) else {
		return HttpResponse::Forbidden().finish();
	};
	let Some(file) = vac_gui_assets::GUI_DIST.get_file(&rel) else {
		return HttpResponse::NotFound().finish();
	};
	let mime = mime_guess::from_path(file.path()).first_or_octet_stream();
	HttpResponse::Ok().content_type(mime).body(file.contents())
}
