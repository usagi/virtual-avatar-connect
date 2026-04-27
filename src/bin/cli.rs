#[actix_web::main]
async fn main() -> std::io::Result<()> {
	virtual_avatar_connect::run_cli().await.unwrap();
	Ok(())
}
