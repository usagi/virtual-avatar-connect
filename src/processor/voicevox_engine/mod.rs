//! VOICEVOX 互換 HTTP API（`GET /speakers`、`POST /audio_query`、`POST /synthesis`）の共通実装。
//! 《AivisSpeech Engine》《VOICEVOX ENGINE》など。CoeiroInk は別系統。

use crate::ProcessorConf;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};

/// `GET /speakers` の1要素（VOICEVOX 互換 JSON）
#[derive(Debug, Clone, Deserialize)]
pub struct VoicevoxSpeaker {
	pub name: String,
	pub speaker_uuid: String,
	pub styles: Vec<VoicevoxStyle>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct VoicevoxStyle {
	pub name: String,
	pub id: i64,
	#[serde(rename = "type")]
	pub style_type: Option<String>,
}

/// `speaker_uuid` がある場合は `style_id` をローカル番号とみなし `/speakers` からグローバル ID を取得する。
/// ない場合は `style_id` をそのまま VOICEVOX 互換のグローバル ID として使う。
pub async fn resolve_speaker_style_id(engine_base: &str, pc: &ProcessorConf, ctx: &str) -> Result<i64> {
	let style_id = pc.style_id.context(format!(
  "{}: style_id が必要です。speaker_uuid 指定時はローカルスタイル番号(0〜)、未指定時は GET /speakers の styles[].id を指定してください。",
  ctx
 ))?;

	if let Some(ref uuid) = pc.speaker_uuid {
		if style_id < 0 {
			bail!(
				"{}: speaker_uuid 指定時は style_id を 0 以上のローカルスタイル番号にしてください（現在: {}）",
				ctx,
				style_id
			);
		}
		let idx = style_id as usize;
		let global = fetch_global_style_id(engine_base, uuid, idx, ctx).await?;
		log::debug!(
			"{}: speaker_uuid={} ローカル style_id={} → グローバル speaker={}",
			ctx,
			uuid,
			style_id,
			global
		);
		return Ok(global);
	}

	Ok(style_id)
}

pub async fn fetch_global_style_id(engine_base: &str, speaker_uuid: &str, local_index: usize, ctx: &str) -> Result<i64> {
	let list = fetch_speakers_list(engine_base, ctx).await?;
	let want_uuid = speaker_uuid.trim().to_lowercase();
	let url = format!("{}/speakers", engine_base.trim_end_matches('/'));
	for sp in list {
		if sp.speaker_uuid.trim().to_lowercase() != want_uuid {
			continue;
		}
		let st = sp.styles.get(local_index).with_context(|| {
			format!(
				"{}: 話者 {} のローカル style 番号 {} が存在しません（styles は {} 件）。GET {} を確認してください。",
				ctx,
				speaker_uuid,
				local_index,
				sp.styles.len(),
				url
			)
		})?;
		return Ok(st.id);
	}

	bail!(
  "{}: speaker_uuid={} が GET {} の一覧に見つかりませんでした（大文字小文字は無視して照合済み。エンジンを起動して GET /speakers を確認してください）。",
  ctx,
  speaker_uuid,
  url
 )
}

/// `engine_base` の `/speakers` を取得してパースする。
pub async fn fetch_speakers_list(engine_base: &str, ctx: &str) -> Result<Vec<VoicevoxSpeaker>> {
	let base = engine_base.trim_end_matches('/');
	let url = format!("{}/speakers", base);
	let client = reqwest::Client::builder().timeout(std::time::Duration::from_secs(30)).build()?;
	let res = client.get(&url).send().await.with_context(|| {
		format!(
			"{}: {} に接続できませんでした。エンジンを先に起動し、api_url が正しいか確認してください。",
			ctx, url
		)
	})?;
	if !res.status().is_success() {
		bail!(
			"{}: GET /speakers が失敗しました: HTTP {}。エンジンが起動しているか確認してください。",
			ctx,
			res.status()
		);
	}
	let v: Value = res.json().await?;
	let arr = speakers_json_as_array(&v).context(format!("{}: /speakers の JSON を解釈できませんでした", ctx))?;
	let mut out = Vec::with_capacity(arr.len());
	for (i, item) in arr.iter().enumerate() {
		let s: VoicevoxSpeaker = serde_json::from_value(item.clone())
			.with_context(|| format!("{}: speakers[{}] を VOICEVOX 互換形式としてパースできませんでした", ctx, i))?;
		out.push(s);
	}
	Ok(out)
}

/// VOICEVOX 系はルートが配列。ラップ形式の場合も拾う。
pub fn speakers_json_as_array(v: &Value) -> Option<&Vec<Value>> {
	if let Some(a) = v.as_array() {
		return Some(a);
	}
	v.get("speakers").and_then(|x| x.as_array())
}

pub async fn synthesize_wav(
	engine_base: &str,
	speaker: i64,
	text: String,
	speed_scale: f64,
	volume_scale: f64,
	pitch_scale: f64,
	intonation_scale: f64,
	pre_phoneme_length: f64,
	post_phoneme_length: f64,
	output_sampling_rate: u32,
) -> Result<Vec<u8>, String> {
	let base = engine_base.trim_end_matches('/');
	let client = reqwest::Client::builder()
		.timeout(std::time::Duration::from_secs(300))
		.build()
		.map_err(|e| e.to_string())?;

	let audio_query_url = format!("{}/audio_query?text={}&speaker={}", base, urlencoding::encode(&text), speaker);
	let res = client
		.post(&audio_query_url)
		.send()
		.await
		.map_err(|e| format!("audio_query: {e}"))?;

	if !res.status().is_success() {
		let status = res.status();
		let t = res.text().await.unwrap_or_default();
		return Err(format!("audio_query HTTP {}: {}", status, t));
	}

	let mut query: Value = res.json().await.map_err(|e| e.to_string())?;

	query["speedScale"] = json!(speed_scale);
	query["volumeScale"] = json!(volume_scale);
	query["pitchScale"] = json!(pitch_scale);
	query["intonationScale"] = json!(intonation_scale);
	query["prePhonemeLength"] = json!(pre_phoneme_length);
	query["postPhonemeLength"] = json!(post_phoneme_length);
	query["outputSamplingRate"] = json!(output_sampling_rate);

	let synthesis_url = format!("{}/synthesis?speaker={}", base, speaker);
	let res = client
		.post(&synthesis_url)
		.json(&query)
		.send()
		.await
		.map_err(|e| format!("synthesis: {e}"))?;

	if !res.status().is_success() {
		let status = res.status();
		let t = res.text().await.unwrap_or_default();
		return Err(format!("synthesis HTTP {}: {}", status, t));
	}

	let b = res.bytes().await.map_err(|e| e.to_string())?;
	Ok(b.to_vec())
}
