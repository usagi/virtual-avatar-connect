//! Phase pi-1 smoke test for the jiff crate.
//!
//! Verifies that the core jiff APIs we plan to use in pi-2 (chrono migration)
//! and pi-4/pi-5 (Flowgraph DateTime socket + nodes) compile and behave as expected.
//! Once pi-2 is in progress these tests become redundant, but they act as a
//! cheap gate to catch accidental regressions while the migration is in flight.

use jiff::{
	SignedDuration, Timestamp, Zoned,
	fmt::temporal::DateTimeParser,
	tz::{Offset, TimeZone},
};

#[test]
fn timestamp_now_is_monotonic() {
	let a = Timestamp::now();
	let b = Timestamp::now();
	assert!(b >= a, "Timestamp::now() must be monotonic non-decreasing on a single thread");
}

#[test]
fn timestamp_from_rfc3339_utc_z() {
	let ts: Timestamp = "2026-04-24T12:34:56Z".parse().expect("parse RFC3339 UTC");
	let s = ts.to_string();
	assert!(s.starts_with("2026-04-24T12:34:56"), "got {s}");
	assert!(s.ends_with('Z'), "Timestamp::to_string uses Z suffix for UTC, got {s}");
}

#[test]
fn timestamp_from_rfc3339_with_offset() {
	let ts: Timestamp = "2026-04-24T21:34:56+09:00".parse().expect("parse RFC3339 +09:00");
	let as_utc = ts.to_string();
	assert!(as_utc.starts_with("2026-04-24T12:34:56"), "JST 21:34 must normalize to UTC 12:34, got {as_utc}");
}

#[test]
fn timestamp_with_subseconds_roundtrip() {
	let ts: Timestamp = "2026-04-24T12:34:56.123456789Z".parse().expect("parse nanosecond precision");
	let s = ts.to_string();
	assert!(s.contains(".123456789"), "nanosecond subseconds must round-trip verbatim, got {s}");
}

#[test]
fn offset_constructors() {
	let jst = Offset::constant(9);
	assert_eq!(jst.seconds(), 9 * 3600);

	let utc = Offset::UTC;
	assert_eq!(utc.seconds(), 0);

	let minus_5_30 = Offset::from_seconds(-(5 * 3600 + 30 * 60)).expect("valid offset seconds");
	assert_eq!(minus_5_30.seconds(), -(5 * 3600 + 30 * 60));
}

/// Phase pi-4 will implement `FlowgraphInstanceConfig.default_timezone` parsing on top of
/// `DateTimeParser::parse_time_zone`. This test pins the semantics: `+09:00` / `-05:30`
/// round-trip into a fixed-offset `TimeZone`, while bare `Z` and IANA names behave as
/// documented (jiff rejects bare `Z` via this API so we will special-case it in config
/// parsing; IANA is accepted here but we will reject it at the config layer to stay
/// within pi's FixedOffset-only scope).
#[test]
fn datetime_parser_parse_time_zone() {
	static PARSER: DateTimeParser = DateTimeParser::new();

	let jst = PARSER.parse_time_zone("+09:00").expect("parse +09:00");
	assert_eq!(jst.to_fixed_offset().expect("fixed").seconds(), 9 * 3600);

	let minus = PARSER.parse_time_zone("-05:30").expect("parse -05:30");
	assert_eq!(minus.to_fixed_offset().expect("fixed").seconds(), -(5 * 3600 + 30 * 60));

	assert!(
		PARSER.parse_time_zone("Z").is_err(),
		"jiff's parse_time_zone rejects bare Z (see upstream docstring). Config parser must special-case Z/UTC."
	);

	assert!(
		PARSER.parse_time_zone("Does/Not/Exist").is_err(),
		"nonexistent IANA zones are rejected by jiff"
	);
}

#[test]
fn zoned_from_fixed_offset_roundtrip() {
	let ts: Timestamp = "2026-04-24T12:34:56Z".parse().unwrap();
	let jst = Offset::constant(9);
	let zoned = ts.to_zoned(TimeZone::fixed(jst));
	let wall = zoned.strftime("%Y-%m-%d %H:%M:%S").to_string();
	assert_eq!(wall, "2026-04-24 21:34:56", "UTC 12:34 + 9h = JST 21:34");
}

#[test]
fn signed_duration_from_secs_f64_and_back() {
	let d = SignedDuration::try_from_secs_f64(1.5).expect("1.5 s must be representable");
	assert_eq!(d.as_secs(), 1);
	assert_eq!(d.subsec_nanos(), 500_000_000);

	let neg = SignedDuration::try_from_secs_f64(-0.25).expect("negative fractional seconds");
	assert_eq!(neg.as_secs(), 0);
	assert_eq!(neg.subsec_nanos(), -250_000_000);
}

#[test]
fn timestamp_add_signed_duration() {
	let ts: Timestamp = "2026-04-24T12:34:56Z".parse().unwrap();
	let later = ts + SignedDuration::from_secs(3600);
	assert_eq!(later.to_string(), "2026-04-24T13:34:56Z");

	let earlier = ts - SignedDuration::from_secs(60);
	assert_eq!(earlier.to_string(), "2026-04-24T12:33:56Z");
}

#[test]
fn timestamp_duration_since_gives_ns_precision() {
	let a: Timestamp = "2026-04-24T12:34:56.000000000Z".parse().unwrap();
	let b: Timestamp = "2026-04-24T12:34:57.500000000Z".parse().unwrap();
	let d = b.duration_since(a);
	assert_eq!(d.as_secs(), 1);
	assert_eq!(d.subsec_nanos(), 500_000_000);
}

#[test]
fn timestamp_as_millis_micros_nanos() {
	let ts: Timestamp = "2026-04-24T12:34:56.123456789Z".parse().unwrap();
	let ms = ts.as_millisecond();
	let us = ts.as_microsecond();
	let ns = ts.as_nanosecond();

	assert!(ms > 0 && us > 0 && ns > 0);
	assert_eq!(us / 1000, ms, "us/1000 == ms invariant");
	assert_eq!((ns / 1_000) as i64, us, "ns/1000 == us invariant");
}

#[test]
fn timestamp_from_millis_roundtrip_ms_precision() {
	let ts: Timestamp = "2026-04-24T12:34:56.789Z".parse().unwrap();
	let ms = ts.as_millisecond();
	let round = Timestamp::from_millisecond(ms).expect("in range");
	assert_eq!(round.as_millisecond(), ms);
}

#[test]
fn zoned_now_respects_system_tz_feature() {
	// With the default `tz-system` feature, `Zoned::now()` succeeds and yields a
	// non-empty tz description (either a fixed offset or an IANA name, depending on the host).
	let z = Zoned::now();
	let tz_str = z
		.time_zone()
		.iana_name()
		.map(|s| s.to_string())
		.unwrap_or_else(|| z.offset().to_string());
	assert!(!tz_str.is_empty(), "Zoned::now() must yield a describable timezone");
}

#[test]
fn serde_timestamp_roundtrip_via_serde_json() {
	let ts: Timestamp = "2026-04-24T12:34:56Z".parse().unwrap();
	let json = serde_json::to_string(&ts).expect("serde serialize");
	assert_eq!(json, "\"2026-04-24T12:34:56Z\"", "serde default must be RFC3339 quoted string");

	let back: Timestamp = serde_json::from_str(&json).expect("serde deserialize");
	assert_eq!(back, ts);
}
