//! Flowgraph / engine 全体で使う絶対時刻の境界型。
//!
//! Phase π-4a で導入した `jiff::Timestamp` の newtype。内部は UTC absolute
//! (nanosecond 精度)。表示や入力受け付けは RFC3339 を第一級とし、任意の
//! `±HH:MM` オフセットを受容して UTC 絶対時刻に正規化する。naive (tz なし)
//! 文字列への対応は π-4c で `FlowgraphInstanceConfig.default_timezone`
//! と組み合わせて実装する予定 (現段階では naive 入力は常に `Err`)。
//!
//! この層では timezone-aware 表現 (`jiff::Zoned`) は持たない。Flowgraph 内の
//! 計算は全て UTC absolute の instant で行い、表示層 / IO 境界で必要に
//! 応じて tz を適用する方針 (phase doc §2 参照)。

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::str::FromStr;

/// Flowgraph / engine 全体で扱う絶対時刻。
///
/// `jiff::Timestamp` (UTC absolute, nanosecond 精度) の透過ラッパ。
/// `Serialize` / `Deserialize` は inner の RFC3339 表現を `#[serde(transparent)]`
/// で継承する (`"2026-04-24T12:34:56.123456789Z"`)。Deserialize 側は任意の
/// offset (`Z` / `+00:00` / `+09:00` / `-05:30` 等) を受容し、UTC 絶対時刻
/// に正規化する。
///
/// inner フィールドは `pub` にしてあるので、拡張パターンマッチや直接の
/// `Timestamp` 取り出しが可能。
#[derive(
	Clone,
	Copy,
	Debug,
	PartialEq,
	Eq,
	Hash,
	PartialOrd,
	Ord,
	Default,
	Serialize,
	Deserialize,
)]
#[serde(transparent)]
pub struct DateTime(pub Timestamp);

impl DateTime {
	/// 現在時刻 (UTC absolute, 壁時計依存)。
	#[inline]
	pub fn now() -> Self {
		Self(Timestamp::now())
	}

	/// UNIX epoch (`1970-01-01T00:00:00Z`)。`Default::default()` と等価。
	#[inline]
	pub const fn unix_epoch() -> Self {
		Self(Timestamp::UNIX_EPOCH)
	}

	/// Z suffix を持つ RFC3339 文字列表現 (`jiff::Timestamp::Display` と同一)。
	/// nanosecond 桁は非ゼロのときのみ出力される。
	#[inline]
	pub fn to_rfc3339(&self) -> String {
		self.0.to_string()
	}

	/// 秒精度 (subsecond 切り捨て) の RFC3339 文字列表現 (`"...Z"` suffix)。
	/// ログ / ファイル名など、ミリ秒以下の桁ゆらぎが邪魔になる場面向け。
	#[inline]
	pub fn to_rfc3339_seconds(&self) -> String {
		self.0.strftime("%Y-%m-%dT%H:%M:%SZ").to_string()
	}

	/// 任意の RFC3339 文字列 (`Z` / `+HH:MM` オフセット込み) から構築。
	/// 内部で UTC absolute に正規化される。naive (tz なし) 文字列は `Err`。
	#[inline]
	pub fn from_rfc3339(s: &str) -> Result<Self, jiff::Error> {
		s.parse::<Timestamp>().map(Self)
	}

	/// inner の `jiff::Timestamp` を取り出す。
	#[inline]
	pub fn timestamp(&self) -> Timestamp {
		self.0
	}
}

impl fmt::Display for DateTime {
	#[inline]
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		fmt::Display::fmt(&self.0, f)
	}
}

impl FromStr for DateTime {
	type Err = jiff::Error;

	#[inline]
	fn from_str(s: &str) -> Result<Self, Self::Err> {
		Self::from_rfc3339(s)
	}
}

impl From<Timestamp> for DateTime {
	#[inline]
	fn from(ts: Timestamp) -> Self {
		Self(ts)
	}
}

impl From<DateTime> for Timestamp {
	#[inline]
	fn from(dt: DateTime) -> Self {
		dt.0
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	// ----- construction -----

	#[test]
	fn default_is_unix_epoch() {
		assert_eq!(DateTime::default(), DateTime::unix_epoch());
		assert_eq!(DateTime::default().to_rfc3339(), "1970-01-01T00:00:00Z");
	}

	#[test]
	fn now_is_after_unix_epoch() {
		assert!(DateTime::now() > DateTime::unix_epoch());
	}

	#[test]
	fn unix_epoch_is_const_fn_safe() {
		// const context で使えることを型で保証 (コンパイルが通るだけで十分)。
		const EPOCH: DateTime = DateTime::unix_epoch();
		assert_eq!(EPOCH, DateTime::default());
	}

	// ----- from_rfc3339 / FromStr -----

	#[test]
	fn from_rfc3339_z_suffix_roundtrip() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:56Z");
	}

	#[test]
	fn fromstr_delegates_to_from_rfc3339() {
		let dt: DateTime = "2026-04-24T12:34:56Z".parse().unwrap();
		assert_eq!(dt, DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap());
	}

	#[test]
	fn from_rfc3339_plus_offset_normalizes_to_utc() {
		let dt = DateTime::from_rfc3339("2026-04-24T21:34:56+09:00").unwrap();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:56Z");
	}

	#[test]
	fn from_rfc3339_minus_offset_normalizes_to_utc() {
		let dt = DateTime::from_rfc3339("2026-04-24T07:04:56-05:30").unwrap();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:56Z");
	}

	#[test]
	fn from_rfc3339_plus_zero_normalizes_to_z() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56+00:00").unwrap();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:56Z");
	}

	#[test]
	fn from_rfc3339_preserves_nanoseconds() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56.123456789Z").unwrap();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:56.123456789Z");
	}

	#[test]
	fn from_rfc3339_rejects_naive_string() {
		// naive (tz なし) は π-4a 層では常にエラー。
		// π-4c で config の default_timezone と組み合わせて明示的に受容する。
		assert!(DateTime::from_rfc3339("2026-04-24T12:34:56").is_err());
		assert!(DateTime::from_rfc3339("2026-04-24T12:34:56.123").is_err());
	}

	#[test]
	fn from_rfc3339_rejects_nonsense_inputs() {
		assert!(DateTime::from_rfc3339("not-a-datetime").is_err());
		assert!(DateTime::from_rfc3339("").is_err());
		assert!(DateTime::from_rfc3339("2026-13-01T00:00:00Z").is_err());
	}

	// ----- formatting -----

	#[test]
	fn to_rfc3339_seconds_omits_subsecond() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56.999999999Z").unwrap();
		assert_eq!(dt.to_rfc3339_seconds(), "2026-04-24T12:34:56Z");
	}

	#[test]
	fn display_matches_to_rfc3339() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56.123Z").unwrap();
		assert_eq!(format!("{dt}"), dt.to_rfc3339());
	}

	// ----- ordering / equality -----

	#[test]
	fn ordering_is_by_instant() {
		let earlier = DateTime::from_rfc3339("2026-04-24T12:00:00Z").unwrap();
		let later = DateTime::from_rfc3339("2026-04-24T13:00:00Z").unwrap();
		assert!(earlier < later);
		assert!(later > earlier);
	}

	#[test]
	fn equality_across_different_offset_spellings() {
		// 同じ瞬間を別 offset で表記したら等しい (UTC absolute 正規化)。
		let z = DateTime::from_rfc3339("2026-04-24T12:00:00Z").unwrap();
		let jst = DateTime::from_rfc3339("2026-04-24T21:00:00+09:00").unwrap();
		let est = DateTime::from_rfc3339("2026-04-24T07:00:00-05:00").unwrap();
		assert_eq!(z, jst);
		assert_eq!(z, est);
		assert_eq!(jst, est);
	}

	// ----- serde -----

	#[test]
	fn serde_json_roundtrip_transparent() {
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56.123456789Z").unwrap();
		let json = serde_json::to_string(&dt).unwrap();
		assert_eq!(json, "\"2026-04-24T12:34:56.123456789Z\"");
		let back: DateTime = serde_json::from_str(&json).unwrap();
		assert_eq!(back, dt);
	}

	#[test]
	fn serde_json_accepts_plus_offset_input_and_normalizes() {
		let raw = "\"2026-04-24T21:34:56+09:00\"";
		let dt: DateTime = serde_json::from_str(raw).unwrap();
		let expected = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		assert_eq!(dt, expected);
	}

	#[test]
	fn serde_json_rejects_naive_string() {
		let raw = "\"2026-04-24T12:34:56\"";
		let result: Result<DateTime, _> = serde_json::from_str(raw);
		assert!(result.is_err());
	}

	// ----- From / Into -----

	#[test]
	fn from_and_into_timestamp_roundtrip() {
		let ts: Timestamp = "2026-04-24T12:34:56Z".parse().unwrap();
		let dt: DateTime = ts.into();
		assert_eq!(dt.timestamp(), ts);
		let back: Timestamp = dt.into();
		assert_eq!(back, ts);
	}

	#[test]
	fn inner_field_is_accessible() {
		// `pub` inner field により、パターンマッチと直接アクセスが可能。
		let dt = DateTime::from_rfc3339("2026-04-24T12:34:56Z").unwrap();
		let DateTime(inner) = dt;
		assert_eq!(inner, dt.0);
	}
}
