//! Phase ρ: **VRChat** が受け取る OSC メッセージの組み立て（送出）。
//!
//! 正本: <https://docs.vrchat.com/docs/osc-as-input-controller>（Chatbox / Typing）、
//! <https://docs.vrchat.com/docs/osc-avatar-parameters>（Avatar Parameters）。  
//! 受信ポート・有効化は VRChat 側設定。本モジュールは **UDP でバイト列を送るだけ**（`host` / `port` はノード入力）。

use crate::flowgraph::osc;
use rosc::OscType;

/// VRChat Chatbox: `s` 文字列、`b` 即送信、`n` 通知 SE。
pub const VRCHAT_CHATBOX_INPUT: &str = "/chatbox/input";

/// VRChat: 入力中インジケータ。
pub const VRCHAT_CHATBOX_TYPING: &str = "/chatbox/typing";

/// `parameter_name` に `/avatar/parameters/` を前置したアドレス（先頭 `/` は禁止）。
pub fn avatar_parameter_address(parameter_name: &str) -> Result<String, String> {
	let n = parameter_name.trim();
	if n.is_empty() {
		return Err("parameter_name が空です".into());
	}
	if n.starts_with('/') || n.contains("..") {
		return Err("parameter_name に先頭 '/' や '..' を含めないでください".into());
	}
	Ok(format!("/avatar/parameters/{n}"))
}

/// Chatbox 用に **最大 144 文字**へ切り詰め（公式制限）。
pub fn clamp_chatbox_text(s: &str) -> String {
	s.chars().take(144).collect()
}

/// `/avatar/parameters/...` に **Float** 1 つ。
pub fn encode_avatar_parameter_float(path: &str, value: f32) -> Result<Vec<u8>, String> {
	if !value.is_finite() {
		return Err("value は有限の float である必要があります".into());
	}
	osc::encode_osc_message_packet(path, &[OscType::Float(value)])
}

/// `/avatar/parameters/...` に **Int**（`i32` に収まる場合は OSC Int、それ以外は Long）。
pub fn encode_avatar_parameter_int(path: &str, value: i64) -> Result<Vec<u8>, String> {
	let t = if value >= i64::from(i32::MIN) && value <= i64::from(i32::MAX) {
		OscType::Int(value as i32)
	} else {
		OscType::Long(value)
	};
	osc::encode_osc_message_packet(path, &[t])
}

/// `/avatar/parameters/...` に **Bool**。
pub fn encode_avatar_parameter_bool(path: &str, value: bool) -> Result<Vec<u8>, String> {
	osc::encode_osc_message_packet(path, &[OscType::Bool(value)])
}

/// `/chatbox/input` — `(string, bool, bool)`。
pub fn encode_chatbox_input(text: &str, send_immediately: bool, play_notification_sfx: bool) -> Result<Vec<u8>, String> {
	let s = clamp_chatbox_text(text);
	osc::encode_osc_message_packet(
		VRCHAT_CHATBOX_INPUT,
		&[
			OscType::String(s),
			OscType::Bool(send_immediately),
			OscType::Bool(play_notification_sfx),
		],
	)
}

/// `/chatbox/typing` — bool。
pub fn encode_chatbox_typing(on: bool) -> Result<Vec<u8>, String> {
	osc::encode_osc_message_packet(VRCHAT_CHATBOX_TYPING, &[OscType::Bool(on)])
}

/// エンコード済みペイロードを VRChat 宛て UDP で送る（`osc::send_udp_bytes` 経由）。
pub async fn send_vrchat_osc(host: &str, port: i64, payload: &[u8]) -> Result<usize, String> {
	osc::send_udp_bytes(host, port, payload).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use rosc::{decoder, OscPacket};

	#[test]
	fn avatar_parameter_address_builds() {
		assert_eq!(
			avatar_parameter_address("VelocityZ").unwrap(),
			"/avatar/parameters/VelocityZ"
		);
		assert!(avatar_parameter_address("").is_err());
		assert!(avatar_parameter_address("/bad").is_err());
	}

	#[test]
	fn encode_float_roundtrip() {
		let path = avatar_parameter_address("TestF").unwrap();
		let bytes = encode_avatar_parameter_float(&path, 0.25f32).unwrap();
		let (_, pkt) = decoder::decode_udp(&bytes).unwrap();
		let OscPacket::Message(m) = pkt else {
			panic!("expected message");
		};
		assert_eq!(m.addr, path);
		assert!(matches!(m.args[0], OscType::Float(x) if (x - 0.25f32).abs() < 1e-5));
	}

	#[test]
	fn encode_chatbox_input_three_args() {
		let bytes = encode_chatbox_input("hi", true, false).unwrap();
		let (_, pkt) = decoder::decode_udp(&bytes).unwrap();
		let OscPacket::Message(m) = pkt else {
			panic!("expected message");
		};
		assert_eq!(m.addr, VRCHAT_CHATBOX_INPUT);
		assert_eq!(m.args.len(), 3);
	}
}
