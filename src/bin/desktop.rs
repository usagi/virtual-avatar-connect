#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

#[actix_web::main]
async fn main() -> std::io::Result<()> {
	virtual_avatar_connect::run_desktop_headless().await.unwrap();
	Ok(())
}
