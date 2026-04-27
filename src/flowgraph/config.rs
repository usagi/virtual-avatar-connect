//! Flowgraph ランタイム instance スコープの config (Phase π-4c)。
//!
//! 現段階では `default_timezone` のみ保持するが、将来的に:
//! - ロケール依存の数値フォーマット既定
//! - log レベル別 TZ 指定
//! - 単位系統の好み (SI prefix 自動選択など)
//!
//! をここに集約していく boundary 型。
//!
//! ## 設計メモ
//!
//! - v0 は **FixedOffset のみ**。IANA timezone (`"Asia/Tokyo"`) は v0 では reject
//!   (Phase π+ で検討)。理由は DST を含むと計算規則が複雑化して「型砦」の一貫性が
//!   崩れるため。
//! - `jiff::tz::Offset` は `FromStr` 未実装 (π-1 smoke test で確認済み)、
//!   `DateTimeParser::parse_time_zone` は bare `"Z"` を reject するかわりに IANA
//!   zone を受理してしまうため、独自パーサ [`parse_offset_str`] で両方の意味論
//!   をカバーする。

use jiff::fmt::temporal::DateTimeParser;
use jiff::tz::{Offset, TimeZone};
use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Flowgraph ランタイム instance スコープの config。
///
/// 設定ファイル (`conf.toml` 等) や programmatic instantiation から構築する。
/// 主に naive datetime 文字列を DateTime に解釈する際の既定 TZ を保持する。
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct FlowgraphInstanceConfig {
	/// 数値または naive datetime 文字列を [`crate::datetime::DateTime`] に
	/// coerce / parse する際の既定 TZ。未設定 / 空文字 / `"Z"` / `"UTC"` は UTC 扱い。
	/// RFC3339 相当の `"+HH:MM"` / `"-HH:MM"` も受理する。
	///
	/// IANA timezone (`"Asia/Tokyo"`) は v0 では reject。書式不正は
	/// [`FlowgraphInstanceConfig::resolve_default_timezone`] で [`ConfigError`] を返す。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub default_timezone: Option<String>,
	/// RM-5: 実効 Runtime Mode が変わった直後（`noop` でない遷移の後）に、
	/// 指定した Flowgraph ノードへ `TriggerEvent` を 1 発投入する。値はロード済みグラフの **ノード ID**（fq）。
	/// `flowgraph.ingress.web_input` 等、`__trigger__` exec 入力を持つ ingress を想定。
	/// `__content__` に JSON、`__source_kind__` に `runtime_mode_changed`、`__meta__` に同じ JSON を載せる。
	#[serde(default, skip_serializing_if = "Option::is_none")]
	pub runtime_mode_changed_trigger_node_id: Option<String>,
}

impl FlowgraphInstanceConfig {
	/// `default_timezone` を [`Offset`] に解決する。
	///
	/// 未設定 / 空文字 / `"Z"` / `"UTC"` は [`Offset::UTC`]。書式不正や IANA zone は
	/// [`ConfigError`] として返す。起動時の非厳密解釈には
	/// [`FlowgraphInstanceConfig::resolve_default_timezone_or_warn`] を使う。
	pub fn resolve_default_timezone(&self) -> Result<Offset, ConfigError> {
		match self.default_timezone.as_deref() {
			None => Ok(Offset::UTC),
			Some(s) => parse_offset_str(s),
		}
	}

	/// `resolve_default_timezone` の non-fatal 版。parse 失敗時は warn ログ + UTC fallback。
	///
	/// 起動時の「best-effort 解釈」(config が壊れていても app を落とさない) を想定。
	/// 厳密な検証が必要な呼出元 (CLI validator など) は [`Self::resolve_default_timezone`] を使うこと。
	pub fn resolve_default_timezone_or_warn(&self) -> Offset {
		match self.resolve_default_timezone() {
			Ok(off) => off,
			Err(e) => {
				log::warn!("《FlowgraphInstanceConfig》 default_timezone parse 失敗: {e}。UTC に fallback します。");
				Offset::UTC
			}
		}
	}
}

/// `"+09:00"` / `"-05:30"` / `"Z"` / `"UTC"` / `""` を [`Offset`] にパース。
///
/// Phase π-1 の smoke test で pin 止めした jiff 挙動に基づく独自パーサ:
/// - `jiff::tz::Offset` は `FromStr` 未実装なので直接 parse できない
/// - [`DateTimeParser::parse_time_zone`] は bare `"Z"` を reject するが IANA zone (`"Asia/Tokyo"`) は
///   受理してしまう → そのまま採用すると DST 含みの semantics になる
///
/// 本関数は以下の層で両方の意味論を統合する:
/// 1. 空文字 / `"Z"` / `"UTC"` (大文字小文字無関係) → `Offset::UTC`
/// 2. それ以外は [`DateTimeParser::parse_time_zone`] で [`TimeZone`] を得る
/// 3. [`TimeZone::to_fixed_offset`] で fixed offset を取り出す (IANA zone は `Err`)
pub fn parse_offset_str(s: &str) -> Result<Offset, ConfigError> {
	let trimmed = s.trim();
	if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("z") || trimmed.eq_ignore_ascii_case("utc") {
		return Ok(Offset::UTC);
	}
	static PARSER: DateTimeParser = DateTimeParser::new();
	let tz: TimeZone = PARSER.parse_time_zone(trimmed).map_err(|e| ConfigError::InvalidOffset {
		input: trimmed.to_string(),
		reason: e.to_string(),
	})?;
	tz.to_fixed_offset().map_err(|_| ConfigError::IanaNotSupported {
		input: trimmed.to_string(),
	})
}

/// [`FlowgraphInstanceConfig::resolve_default_timezone`] と
/// [`parse_offset_str`] 共通のエラー型。
#[derive(Debug, Clone, Error)]
pub enum ConfigError {
	#[error("default_timezone の書式不正: '{input}' ({reason})")]
	InvalidOffset { input: String, reason: String },
	#[error("default_timezone に IANA zone は受理しません (v0 は fixed offset のみ): '{input}'")]
	IanaNotSupported { input: String },
}

// ---------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	// ----- parse_offset_str -----

	#[test]
	fn parse_offset_str_empty_is_utc() {
		assert_eq!(parse_offset_str("").unwrap(), Offset::UTC);
		assert_eq!(parse_offset_str("   ").unwrap(), Offset::UTC);
	}

	#[test]
	fn parse_offset_str_z_is_utc() {
		assert_eq!(parse_offset_str("Z").unwrap(), Offset::UTC);
		assert_eq!(parse_offset_str("z").unwrap(), Offset::UTC);
	}

	#[test]
	fn parse_offset_str_utc_keyword_is_utc() {
		assert_eq!(parse_offset_str("UTC").unwrap(), Offset::UTC);
		assert_eq!(parse_offset_str("utc").unwrap(), Offset::UTC);
		assert_eq!(parse_offset_str("Utc").unwrap(), Offset::UTC);
	}

	#[test]
	fn parse_offset_str_plus_09_00() {
		let off = parse_offset_str("+09:00").unwrap();
		assert_eq!(off.seconds(), 9 * 3600);
	}

	#[test]
	fn parse_offset_str_minus_05_30() {
		let off = parse_offset_str("-05:30").unwrap();
		assert_eq!(off.seconds(), -(5 * 3600 + 30 * 60));
	}

	#[test]
	fn parse_offset_str_plus_zero() {
		let off = parse_offset_str("+00:00").unwrap();
		assert_eq!(off, Offset::UTC);
	}

	#[test]
	fn parse_offset_str_rejects_iana_zone() {
		let err = parse_offset_str("Asia/Tokyo").unwrap_err();
		match err {
			ConfigError::IanaNotSupported { input } => assert_eq!(input, "Asia/Tokyo"),
			other => panic!("expected IanaNotSupported, got {other:?}"),
		}
	}

	#[test]
	fn parse_offset_str_rejects_nonexistent_zone() {
		let err = parse_offset_str("Does/Not/Exist").unwrap_err();
		assert!(matches!(err, ConfigError::InvalidOffset { .. }));
	}

	#[test]
	fn parse_offset_str_rejects_nonsense() {
		assert!(matches!(parse_offset_str("nonsense"), Err(ConfigError::InvalidOffset { .. })));
		assert!(matches!(parse_offset_str("+9:00"), Err(ConfigError::InvalidOffset { .. })));
	}

	// ----- FlowgraphInstanceConfig -----

	#[test]
	fn default_config_resolves_to_utc() {
		let cfg = FlowgraphInstanceConfig::default();
		assert_eq!(cfg.resolve_default_timezone().unwrap(), Offset::UTC);
		assert_eq!(cfg.resolve_default_timezone_or_warn(), Offset::UTC);
	}

	#[test]
	fn config_with_plus_09_00_resolves() {
		let cfg = FlowgraphInstanceConfig {
			default_timezone: Some("+09:00".into()),
			..Default::default()
		};
		assert_eq!(cfg.resolve_default_timezone().unwrap().seconds(), 9 * 3600);
	}

	#[test]
	fn config_with_invalid_tz_errs_strict() {
		let cfg = FlowgraphInstanceConfig {
			default_timezone: Some("Asia/Tokyo".into()),
			..Default::default()
		};
		assert!(matches!(cfg.resolve_default_timezone(), Err(ConfigError::IanaNotSupported { .. })));
	}

	#[test]
	fn config_with_invalid_tz_falls_back_to_utc_with_warn() {
		let cfg = FlowgraphInstanceConfig {
			default_timezone: Some("Asia/Tokyo".into()),
			..Default::default()
		};
		// warn ログは goes-to void だが UTC に fallback することを確認
		assert_eq!(cfg.resolve_default_timezone_or_warn(), Offset::UTC);
	}

	#[test]
	fn config_serde_roundtrip_with_offset() {
		let cfg = FlowgraphInstanceConfig {
			default_timezone: Some("+09:00".into()),
			..Default::default()
		};
		let toml = toml::to_string(&cfg).unwrap();
		assert!(toml.contains("default_timezone"), "toml: {toml}");
		let back: FlowgraphInstanceConfig = toml::from_str(&toml).unwrap();
		assert_eq!(back, cfg);
	}

	#[test]
	fn config_serde_omits_none_default_timezone() {
		// `skip_serializing_if = "Option::is_none"` で None は書き出さない。
		let cfg = FlowgraphInstanceConfig::default();
		let toml = toml::to_string(&cfg).unwrap();
		assert!(!toml.contains("default_timezone"), "toml should be empty: {toml}");
	}

	#[test]
	fn config_serde_deserialize_from_toml_table() {
		let src = r#"default_timezone = "-05:30""#;
		let cfg: FlowgraphInstanceConfig = toml::from_str(src).unwrap();
		assert_eq!(cfg.default_timezone.as_deref(), Some("-05:30"));
		assert_eq!(cfg.resolve_default_timezone().unwrap().seconds(), -(5 * 3600 + 30 * 60));
	}
}
