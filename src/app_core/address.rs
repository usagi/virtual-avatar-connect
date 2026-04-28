use std::net::SocketAddr;

pub(super) fn normalize_loopback_address(address: &str) -> String {
	if let Ok(socket) = address.parse::<SocketAddr>() {
		let port = socket.port();
		let ip = socket.ip();
		if ip.is_unspecified() {
			return format!("127.0.0.1:{port}");
		}
		if ip.is_ipv6() {
			return format!("[{ip}]:{port}");
		}
		return socket.to_string();
	}

	address
		.strip_prefix("0.0.0.0:")
		.map(|port| format!("127.0.0.1:{port}"))
		.unwrap_or_else(|| address.to_string())
}
