#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> std::io::Result<()> {
	virtual_avatar_connect::run_desktop().unwrap();
	Ok(())
}
