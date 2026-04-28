//! Phase ρ: **VMC Protocol** 準拠の OSC メッセージ組み立て・**受信ワイヤからの取り出し**（姿勢の一部）。
//!
//! 正本: <https://protocol.vmc.info/english.html>  
//! - **送出**: [`VMC_EXT_BONE_POS`] / [`VMC_EXT_ROOT_POS`] の単一メッセージエンコード（全骨ストリームは呼び出し側ループ）。
//! - **受信**: [`MotionFrame`](crate::MotionFrame) 内の姿勢・表情アドレスを構造化（ingress は `vmc_udp` + `vmc_parse`）。

use crate::osc;
use crate::{MotionFrame, OscMessageWire};
use rosc::{OscMessage, OscPacket, OscType};
use serde::Serialize;
use serde_json::Value as JsonValue;

/// `/VMC/Ext/Bone/Pos` — `(string){HumanBodyBones 名}` + position + quaternion。
pub const VMC_EXT_BONE_POS: &str = "/VMC/Ext/Bone/Pos";

/// `/VMC/Ext/Root/Pos` — 先頭文字列は通常 `"root"`。
pub const VMC_EXT_ROOT_POS: &str = "/VMC/Ext/Root/Pos";

/// `/VMC/Ext/Blend/Val` — `(string){BlendShape 名}` + `(float)value`。
pub const VMC_EXT_BLEND_VAL: &str = "/VMC/Ext/Blend/Val";

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
		let f = e.as_f64().ok_or_else(|| format!("{label}[{i}] が数値ではありません"))?;
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
		let f = e.as_f64().ok_or_else(|| format!("{label}[{i}] が数値ではありません"))?;
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

// ---------------------------------------------------------------------
// 受信: MotionFrame 内の /VMC/Ext/*/Pos
// ---------------------------------------------------------------------

/// VMC Bone/Root Pos 1 メッセージ分の構造化サンプル（`pose` JSON 出力用）。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VmcPosQuatSample {
	pub bone: String,
	pub position: [f64; 3],
	pub rotation: [f64; 4],
}

/// VMC BlendShape 値 1 メッセージ分の構造化サンプル。
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VmcBlendShapeSample {
	pub name: String,
	pub value: f64,
}

impl VmcBlendShapeSample {
	pub fn to_json_value(&self) -> JsonValue {
		serde_json::to_value(self).unwrap_or(JsonValue::Null)
	}
}

impl VmcPosQuatSample {
	pub fn to_json_value(&self) -> JsonValue {
		serde_json::to_value(self).unwrap_or(JsonValue::Null)
	}
}

fn json_arg_as_f64(v: &JsonValue) -> Option<f64> {
	v.as_f64()
		.or_else(|| v.as_i64().map(|i| i as f64))
		.or_else(|| v.as_u64().map(|u| u as f64))
}

/// `expected_address` に一致し、かつ 8 引数（string + 7 数値）が解釈できるときだけ [`VmcPosQuatSample`] を返す。
pub fn try_parse_ext_pos_message(msg: &OscMessageWire, expected_address: &str) -> Option<VmcPosQuatSample> {
	if msg.address != expected_address {
		return None;
	}
	if msg.args.len() < 8 {
		return None;
	}
	let bone = msg.args[0].as_str()?.to_string();
	let mut position = [0.0f64; 3];
	for i in 0..3 {
		let f = json_arg_as_f64(&msg.args[1 + i])?;
		if !f.is_finite() {
			return None;
		}
		position[i] = f;
	}
	let mut rotation = [0.0f64; 4];
	for i in 0..4 {
		let f = json_arg_as_f64(&msg.args[4 + i])?;
		if !f.is_finite() {
			return None;
		}
		rotation[i] = f;
	}
	Some(VmcPosQuatSample { bone, position, rotation })
}

/// `/VMC/Ext/Blend/Val` を `(name, value)` として解釈する。
pub fn try_parse_blend_val_message(msg: &OscMessageWire) -> Option<VmcBlendShapeSample> {
	if msg.address != VMC_EXT_BLEND_VAL {
		return None;
	}
	if msg.args.len() < 2 {
		return None;
	}
	let name = msg.args[0].as_str()?.to_string();
	let value = json_arg_as_f64(&msg.args[1])?;
	if !value.is_finite() {
		return None;
	}
	Some(VmcBlendShapeSample { name, value })
}

/// 最初に見つかった `/VMC/Ext/Bone/Pos`。`bone_name_equals` が空でなければ骨名が **完全一致**のもののみ。
pub fn extract_first_bone_pos(frame: &MotionFrame, bone_name_equals: &str) -> Option<VmcPosQuatSample> {
	let want = bone_name_equals.trim();
	for m in &frame.osc_messages {
		if let Some(s) = try_parse_ext_pos_message(m, VMC_EXT_BONE_POS) {
			if want.is_empty() || s.bone == want {
				return Some(s);
			}
		}
	}
	None
}

/// 最初に見つかった `/VMC/Ext/Root/Pos`。
pub fn extract_first_root_pos(frame: &MotionFrame) -> Option<VmcPosQuatSample> {
	for m in &frame.osc_messages {
		if let Some(s) = try_parse_ext_pos_message(m, VMC_EXT_ROOT_POS) {
			return Some(s);
		}
	}
	None
}

/// 最初に見つかった `/VMC/Ext/Blend/Val`。`name_equals` が空でなければ blendshape 名が **完全一致**のもののみ。
pub fn extract_first_blendshape(frame: &MotionFrame, name_equals: &str) -> Option<VmcBlendShapeSample> {
	let want = name_equals.trim();
	for m in &frame.osc_messages {
		if let Some(s) = try_parse_blend_val_message(m) {
			if want.is_empty() || s.name == want {
				return Some(s);
			}
		}
	}
	None
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
	use crate::OscMessageWire;
	use rosc::{decoder, OscPacket};
	use serde_json::json;

	#[test]
	fn parse_ext_pos_from_wire_args() {
		let msg = OscMessageWire {
			address: VMC_EXT_BONE_POS.into(),
			args: vec![
				JsonValue::String("Head".into()),
				json!(0.1),
				json!(0.2),
				json!(0.3),
				json!(0.0),
				json!(0.0),
				json!(0.0),
				json!(1.0),
			],
		};
		let s = try_parse_ext_pos_message(&msg, VMC_EXT_BONE_POS).expect("parse");
		assert_eq!(s.bone, "Head");
		assert_eq!(s.position, [0.1, 0.2, 0.3]);
		assert_eq!(s.rotation, [0.0, 0.0, 0.0, 1.0]);
	}

	#[test]
	fn extract_bone_respects_name_filter() {
		let frame = MotionFrame {
			byte_len: 0,
			osc_messages: vec![
				OscMessageWire {
					address: "/other".into(),
					args: vec![],
				},
				OscMessageWire {
					address: VMC_EXT_BONE_POS.into(),
					args: vec![
						JsonValue::String("Hips".into()),
						json!(1.0),
						json!(0.0),
						json!(0.0),
						json!(0.0),
						json!(0.0),
						json!(0.0),
						json!(1.0),
					],
				},
				OscMessageWire {
					address: VMC_EXT_BONE_POS.into(),
					args: vec![
						JsonValue::String("Head".into()),
						json!(2.0),
						json!(0.0),
						json!(0.0),
						json!(0.0),
						json!(0.0),
						json!(0.0),
						json!(1.0),
					],
				},
			],
		};
		let first = extract_first_bone_pos(&frame, "").expect("first bone");
		assert_eq!(first.bone, "Hips");
		let head = extract_first_bone_pos(&frame, "Head").expect("head");
		assert_eq!(head.bone, "Head");
		assert_eq!(head.position[0], 2.0);
		assert!(extract_first_bone_pos(&frame, "Spine").is_none());
	}

	#[test]
	fn extract_root_first() {
		let frame = MotionFrame {
			byte_len: 0,
			osc_messages: vec![OscMessageWire {
				address: VMC_EXT_ROOT_POS.into(),
				args: vec![
					JsonValue::String("root".into()),
					json!(0.0),
					json!(1.0),
					json!(2.0),
					json!(0.0),
					json!(0.0),
					json!(0.0),
					json!(1.0),
				],
			}],
		};
		let s = extract_first_root_pos(&frame).expect("root");
		assert_eq!(s.bone, "root");
		assert_eq!(s.position, [0.0, 1.0, 2.0]);
	}

	#[test]
	fn extract_blendshape_respects_name_filter() {
		let frame = MotionFrame {
			byte_len: 0,
			osc_messages: vec![
				OscMessageWire {
					address: VMC_EXT_BLEND_VAL.into(),
					args: vec![JsonValue::String("Joy".into()), json!(0.25)],
				},
				OscMessageWire {
					address: VMC_EXT_BLEND_VAL.into(),
					args: vec![JsonValue::String("Blink_L".into()), json!(0.75)],
				},
			],
		};
		let first = extract_first_blendshape(&frame, "").expect("first blendshape");
		assert_eq!(first.name, "Joy");
		assert_eq!(first.value, 0.25);
		let blink = extract_first_blendshape(&frame, "Blink_L").expect("blink");
		assert_eq!(blink.value, 0.75);
		assert!(extract_first_blendshape(&frame, "Angry").is_none());
	}

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
