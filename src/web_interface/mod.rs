pub mod input;
pub mod output;

use actix_web::{get, HttpResponse};

static FAVICON_ICO: &[u8] = include_bytes!("../../favicon.ico");

#[get("/favicon.ico")]
pub async fn favicon() -> HttpResponse {
 HttpResponse::Ok().content_type("image/x-icon").body(FAVICON_ICO)
}
