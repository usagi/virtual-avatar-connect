#[actix_web::main]
async fn main() -> virtual_avatar_connect::Result<()> {
	virtual_avatar_connect::run_cli().await
}
