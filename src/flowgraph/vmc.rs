//! Phase ρ: **VMC Protocol** 準拠の OSC メッセージ組み立て（姿勢の一部）。
//!
//! 正本: <https://protocol.vmc.info/english.html>  
//! 現状は [`VMC_EXT_BONE_POS`] / [`VMC_EXT_ROOT_POS`] の **単一メッセージ**エンコードのみ（全骨ストリームは呼び出し側のループで行う）。

use crate::flowgraph::osc;
use rosc::{OscMessage, OscPacket, OscType};
use serde_json::Value as JsonValue;

/// `/VMC/Ext/Bone/Pos` — `(string){HumanBodyBones 名}` + position + quaternion。
pub const VMC_EXT_BONE_POS: &str = "/VMC/Ext/Bone/Pos";

/// `/VMC/Ext/Root/Pos` — 先頭文字列は通常 `"root"`。
pub const VMC_EXT_ROOT_POS: &str = "/VMC/Ext/Root/Pos";

/// JSON 配列 `[x,y,z]` を位置としてパースする。
pub fn parse_json_vec3(label: &str, v: &JsonValue) -> Result<[f64; 3], String> {
	let a = v
		.as_array()
		.ok_or_else(|| format!("{label} は [x, y, z] の JSON 配列である必要があります"))?;
	if a.len() != 3 {
		return Err(format!("{label} は長さ 3 の配列である必要があります"));
	}
	let mut out = [0.0f64; 3];
	for (i, e) in a.iter().enumerate() {
		let f = e
			.as_f64()
			.ok_or_else(|| format!("{label}[{i}] が数値ではありません"))?;
		if !f.is_finite() {
			return Err(format!("{label}[{i}] は有限の数である必要があります"));
		}
		out[i] = f;
	}
	Ok(out)
}

/// JSON 配列 `[qx, qy, qz, qw]` を回転（Quaternion）としてパースする。
pub fn parse_json_quat(label: &str, v: &JsonValue) -> Result<[f64; 4], String> {
	let a = v
		.as_array()
		.ok_or_else(|| format!("{label} は [qx, qy, qz, qw] の JSON 配列である必要があります"))?;
	if a.len() != 4 {
		return Err(format!("{label} は長さ 4 の配列である必要があります"));
	}
	let mut out = [0.0f64; 4];
	for (i, e) in a.iter().enumerate() {
		let f = e
			.as_f64()
			.ok_or_else(|| format!("{label}[{i}] が数値ではありません"))?;
		if !f.is_finite() {
			return Err(format!("{label}[{i}] は有限の数である必要があります"));
		}
		out[i] = f;
	}
	Ok(out)
}

fn vmc_pos_quat_args(bone_name: &str, pos: [f64; 3], rot: [f64; 4]) -> Vec<OscType> {
	let name = bone_name.trim();
	let mut args = Vec::with_capacity(8);
	args.push(OscType::String(name.to_string()));
	for c in pos {
		args.push(OscType::Float(c as f32));
	}
	for c in rot {
		args.push(OscType::Float(c as f32));
	}
	args
}

/// VMC の Bone/Root Pos メッセージ 1 本分を OSC UDP ペイロードにエンコードする。
pub fn encode_vmc_transform_pos(address: &str, bone_name: &str, pos: [f64; 3], rot: [f64; 4]) -> Result<Vec<u8>, String> {
	if !address.starts_with('/') {
		return Err("OSC address は '/' で始まる必要があります".into());
	}
	let name = bone_name.trim();
	if name.is_empty() {
		return Err("bone 名が空です".into());
	}
	let args = vmc_pos_quat_args(name, pos, rot);
	let packet = OscPacket::Message(OscMessage {
		addr: address.to_string(),
		args,
	});
	rosc::encoder::encode(&packet).map_err(|e| format!("OSC encode: {e}"))
}

/// [`encode_vmc_transform_pos`] の結果を UDP で送る。
pub async fn send_vmc_transform_pos_udp(
	host: &str,
	port: i64,
	address: &str,
	bone_name: &str,
	pos: [f64; 3],
	rot: [f64; 4],
) -> Result<usize, String> {
	let bytes = encode_vmc_transform_pos(address, bone_name, pos, rot)?;
	osc::send_udp_bytes(host, port, &bytes).await
}

#[cfg(test)]
mod tests {
	use super::*;
	use rosc::{decoder, OscPacket};
	use serde_json::json;

	#[test]
	fn encode_bone_pos_roundtrip_decode() {
		let bytes = encode_vmc_transform_pos(VMC_EXT_BONE_POS, "Head", [0.1, -0.2, 0.3], [0.0, 0.0, 0.0, 1.0]).unwrap();
		let (_, pkt) = decoder::decode_udp(&bytes).unwrap();
		let OscPacket::Message(m) = pkt else {
			panic!("expected message");
		};
		assert_eq!(m.addr, VMC_EXT_BONE_POS);
		assert_eq!(m.args.len(), 8);
	}

	#[test]
	fn parse_vec3_quat_errors_on_bad_len() {
		assert!(parse_json_vec3("p", &json!([1, 2])).is_err());
		assert!(parse_json_quat("q", &json!([0, 0, 1])).is_err());
	}

	#[tokio::test]
	async fn send_root_smoke() {
		let n = send_vmc_transform_pos_udp("127.0.0.1", 9, VMC_EXT_ROOT_POS, "root", [0.0; 3], [0.0, 0.0, 0.0, 1.0])
			.await
			.expect("send");
		assert!(n > 0);
	}
}
