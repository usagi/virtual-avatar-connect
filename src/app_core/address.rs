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

#[cfg(test)]
mod tests {
	use super::normalize_loopback_address;

	#[test]
	fn unspecified_ipv4_becomes_loopback() {
		assert_eq!(normalize_loopback_address("0.0.0.0:57000"), "127.0.0.1:57000");
	}

	#[test]
	fn unspecified_ipv6_becomes_loopback() {
		assert_eq!(normalize_loopback_address("[::]:57000"), "127.0.0.1:57000");
	}

	#[test]
	fn concrete_ipv6_is_bracketed() {
		assert_eq!(normalize_loopback_address("[::1]:57000"), "[::1]:57000");
	}

	#[test]
	fn host_name_is_left_as_is() {
		assert_eq!(normalize_loopback_address("localhost:57000"), "localhost:57000");
	}
}
