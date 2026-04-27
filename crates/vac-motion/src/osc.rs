//! Phase ρ: Flowgraph から共有する **OSC over UDP** ヘルパ（送出・エンコード）。
//!
//! Flowgraph の OSC send node や VMC pose 送出などが利用する。
//! [`parse_vmc_payload`](crate::parse_vmc_payload) の **受信デコード**とは役割が異なる。

use rosc::{encoder, OscArray, OscMessage, OscPacket, OscType};
use serde_json::Value as JsonValue;

/// `flowgraph.osc.send` の `args` 配列要素と同じ規則で JSON → [`OscType`]。
pub fn json_value_to_osc_type(v: &JsonValue) -> Result<OscType, String> {
	match v {
		JsonValue::Null => Ok(OscType::Nil),
		JsonValue::Bool(b) => Ok(OscType::Bool(*b)),
		JsonValue::Number(n) => {
			if let Some(i) = n.as_i64() {
				if i >= i64::from(i32::MIN) && i <= i64::from(i32::MAX) {
					return Ok(OscType::Int(i as i32));
				}
				return Ok(OscType::Long(i));
			}
			let f = n.as_f64().ok_or_else(|| "number".to_string())?;
			Ok(OscType::Double(f))
		}
		JsonValue::String(s) => Ok(OscType::String(s.clone())),
		JsonValue::Array(a) => {
			let inner: Result<Vec<OscType>, String> = a.iter().map(json_value_to_osc_type).collect();
			Ok(OscType::Array(OscArray { content: inner? }))
		}
		other => Err(format!("OSC 引数として未対応の JSON: {other}")),
	}
}

/// 単一 OSC メッセージを UDP ペイロードにエンコードする。
pub fn encode_osc_message_packet(address: &str, args: &[OscType]) -> Result<Vec<u8>, String> {
	if !address.starts_with('/') {
		return Err("OSC address は '/' で始まる必要があります".into());
	}
	let packet = OscPacket::Message(OscMessage {
		addr: address.to_string(),
		args: args.to_vec(),
	});
	encoder::encode(&packet).map_err(|e| format!("OSC encode: {e}"))
}

/// JSON 配列の引数を OSC に変換してからエンコードする（ノードの検証と同順序）。
pub fn encode_osc_message_from_json_args(address: &str, args: &[JsonValue]) -> Result<Vec<u8>, String> {
	let mut osc_args = Vec::with_capacity(args.len());
	for v in args {
		osc_args.push(json_value_to_osc_type(v)?);
	}
	encode_osc_message_packet(address, &osc_args)
}

/// 既にエンコード済みの UDP ペイロードを `host` / `port` へ送信する（VMC 生バンドル等でも利用可）。
pub async fn send_udp_bytes(host: &str, port: i64, payload: &[u8]) -> Result<usize, String> {
	if port <= 0 || port > u16::MAX as i64 {
		return Err(format!("port が不正です: {port}"));
	}
	let sock = tokio::net::UdpSocket::bind("0.0.0.0:0")
		.await
		.map_err(|e| format!("UDP bind: {e}"))?;
	let port_u16 = port as u16;
	let mut targets = tokio::net::lookup_host((host.trim(), port_u16))
		.await
		.map_err(|e| format!("DNS lookup: {e}"))?;
	let addr = targets.next().ok_or_else(|| format!("ホスト解決結果が空です: {host}"))?;
	sock.send_to(payload, addr).await.map_err(|e| format!("UDP send_to: {e}"))
}

/// `host` / `port` へ単発 OSC を UDP 送信し、送信バイト数を返す。
pub async fn send_osc_udp_json_args(host: &str, port: i64, address: &str, args: &[JsonValue]) -> Result<usize, String> {
	let bytes = encode_osc_message_from_json_args(address, args)?;
	send_udp_bytes(host, port, &bytes).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use rosc::{decoder, OscPacket};
	use serde_json::json;

	#[test]
	fn json_to_osc_int_and_double() {
		assert!(matches!(json_value_to_osc_type(&json!(42)).unwrap(), OscType::Int(42)));
		assert!(matches!(json_value_to_osc_type(&json!(1.5)).unwrap(), OscType::Double(_)));
	}

	#[test]
	fn json_to_osc_nested_array() {
		let t = json_value_to_osc_type(&json!([1, [2, 3]])).unwrap();
		let OscType::Array(a) = t else {
			panic!("expected array");
		};
		assert_eq!(a.content.len(), 2);
	}

	#[test]
	fn encode_rejects_non_slash_address() {
		let err = encode_osc_message_packet("no_slash", &[]).unwrap_err();
		assert!(err.contains('/'));
	}

	#[test]
	fn encode_roundtrip_decode() {
		let bytes = encode_osc_message_from_json_args("/x", &[json!(1.0), json!("hi")]).unwrap();
		let (_, pkt) = decoder::decode_udp(&bytes).unwrap();
		let OscPacket::Message(m) = pkt else {
			panic!("expected message");
		};
		assert_eq!(m.addr, "/x");
		assert_eq!(m.args.len(), 2);
	}

	#[tokio::test]
	async fn send_to_discard_smoke() {
		let n = send_osc_udp_json_args("127.0.0.1", 9, "/vac/test", &[json!(null)])
			.await
			.expect("send");
		assert!(n > 0);
	}
}
