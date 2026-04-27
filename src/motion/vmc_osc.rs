//! VMC が載せる **OSC over UDP** バイト列のデコード（`rosc`）。
//!
//! パースのみ。転送は [`super::router`] / `bridges::vmc_ingress` の責務。

use crate::motion::frame::{MotionFrameV0, OscMessageWire};
use base64::{engine::general_purpose::STANDARD as B64, Engine as _};
use rosc::{decoder, OscPacket, OscType};
use serde_json::{json, Value as JsonValue};

fn osc_type_to_json(t: &OscType) -> JsonValue {
	match t {
		OscType::Int(i) => json!(i),
		OscType::Float(f) => json!(f),
		OscType::String(s) => JsonValue::String(s.clone()),
		OscType::Double(d) => json!(d),
		OscType::Long(l) => json!(l),
		OscType::Bool(b) => json!(b),
		OscType::Nil => JsonValue::Null,
		OscType::Inf => json!("inf"),
		OscType::Char(c) => json!(c.to_string()),
		OscType::Time(tm) => json!([tm.seconds, tm.fractional]),
		OscType::Color(c) => json!([c.red, c.green, c.blue, c.alpha]),
		OscType::Blob(b) => json!(B64.encode(b)),
		OscType::Midi(m) => json!({
			"port": m.port,
			"status": m.status,
			"data1": m.data1,
			"data2": m.data2,
		}),
		OscType::Array(a) => JsonValue::Array(a.content.iter().map(osc_type_to_json).collect()),
	}
}

fn collect_messages(packet: &OscPacket, out: &mut Vec<OscMessageWire>) {
	match packet {
		OscPacket::Message(m) => {
			out.push(OscMessageWire {
				address: m.addr.clone(),
				args: m.args.iter().map(osc_type_to_json).collect(),
			});
		}
		OscPacket::Bundle(b) => {
			for inner in &b.content {
				collect_messages(inner, out);
			}
		}
	}
}

/// UDP 上の生バイト列を OSC として解釈し、M4 用 JSON オブジェクトにまとめる。
pub fn parse_vmc_payload(bytes: &[u8]) -> Result<MotionFrameV0, String> {
	let (_, packet) = decoder::decode_udp(bytes).map_err(|e| format!("OSC decode: {e}"))?;
	let mut osc_messages = Vec::new();
	collect_messages(&packet, &mut osc_messages);
	Ok(MotionFrameV0 {
		byte_len: bytes.len(),
		osc_messages,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use rosc::encoder;
	use rosc::{OscMessage, OscPacket};

	#[test]
	fn round_trip_single_message() {
		let pkt = OscPacket::Message(OscMessage {
			addr: "/V/Test/Blend/Fun".into(),
			args: vec![OscType::Float(0.5), OscType::Int(1)],
		});
		let bytes = encoder::encode(&pkt).expect("encode");
		let frame = parse_vmc_payload(&bytes).expect("parse");
		assert_eq!(frame.byte_len, bytes.len());
		assert_eq!(frame.osc_messages.len(), 1);
		assert_eq!(frame.osc_messages[0].address, "/V/Test/Blend/Fun");
		assert_eq!(frame.osc_messages[0].args.len(), 2);
	}
}
