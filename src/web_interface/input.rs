use crate::Result;
use actix_web::{web, HttpResponse, Responder};
use std::path::Path;

/// HTTP POST は [`super::web_input`]（《WebInput》 ingress）で登録する。

#[actix_web::get("/input")]
pub async fn get_index() -> Result<impl Responder> {
	// ./input/index.html を読み込む
	let path = Path::new("./input/index.html");
	match tokio::fs::read_to_string(path).await {
		Ok(content) => Ok(HttpResponse::Ok().content_type("text/html").body(content)),
		_ => Ok(HttpResponse::NotFound().finish()),
	}
}

#[actix_web::get("/input/{subindex}")]
pub async fn get_subfile(subindex: web::Path<String>) -> Result<impl Responder> {
	// ./input/{subindex}.html を読み込む
	let path = Path::new("./input/").join(subindex.as_str()).with_extension("html");
	match tokio::fs::read_to_string(path).await {
		Ok(content) => Ok(HttpResponse::Ok().content_type("text/html").body(content)),
		_ => Ok(HttpResponse::NotFound().finish()),
	}
}
