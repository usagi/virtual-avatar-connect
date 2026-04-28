#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

fn main() -> virtual_avatar_connect::Result<()> {
	virtual_avatar_connect::run_desktop()
}
