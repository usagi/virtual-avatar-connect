//! DateTime nodes (Phase \u{03C0}-5).
//!
//! 8 PureNode \u{3067} DateTime \u{6f14}\u{7b97}\u{3092}\u{63d0}\u{4f9b}:
//!
//! - `flowgraph.datetime.now`          () -> DateTime  (\u{975e}\u{6c7a}\u{5b9a}\u{6027})
//! - `flowgraph.datetime.parse`        String -> DateTime  (aware / naive + default_tz property)
//! - `flowgraph.datetime.format`       DateTime -> String  (rfc3339 / iso8601_compact / unix_* / custom)
//! - `flowgraph.datetime.add_duration` (DateTime, Quantity<time>) -> DateTime
//! - `flowgraph.datetime.sub_duration` (DateTime, Quantity<time>) -> DateTime
//! - `flowgraph.datetime.diff`         (DateTime, DateTime) -> Quantity<s>
//! - `flowgraph.datetime.epoch_ms`     DateTime -> Quantity<ms>
//! - `flowgraph.datetime.from_epoch_ms` Quantity (time | dimensionless) -> DateTime
//!
//! \u{5185}\u{90e8}\u{8868}\u{73fe}\u{306f} [`crate::datetime::DateTime`] (= `jiff::Timestamp` \u{306e} newtype)\u{3002}
//! Duration \u{306f} Phase \u{03BE} \u{3067}\u{5c0e}\u{5165}\u{6e08}\u{306e} `Quantity<time>` \u{3092}\u{305d}\u{306e}\u{307e}\u{307e}\u{517c}\u{52d9}\u{3057}\u{3001}
//! \u{65b0}\u{305f}\u{306a} Duration \u{578b}\u{306f}\u{8ffd}\u{52a0}\u{3057}\u{306a}\u{3044} (phase doc \u{00a7}3.2\u{3002})\u{3002}
//!
//! `add_duration` / `sub_duration` / `from_epoch_ms` \u{306f}\u{6b21}\u{5143}\u{4e0d}\u{4e00}\u{81f4}\u{3067} halt \u{3059}\u{308b}
//! (D4: strict default\u{3001}phase-\u{03BE} \u{306e}\u{300c}\u{578b}\u{306e}\u{7802}\u{300d}\u{5400}\u{5b66})\u{3002}
//! dimensionless Quantity \u{306f} `ms` (from_epoch_ms) / `s` (add/sub_duration) \u{3068}\u{3057}\u{3066}
//! \u{89e3}\u{91c8}\u{3055}\u{308c}\u{308b} (\u{5f8c}\u{65b9}\u{4e92}\u{63db}: \u{003c}i-3 \u{66b4}\u{9ed9} Float\u{2192}dimensionless \u{306e}\u{5ef6}\u{9577})\u{3002}

use crate::datetime::DateTime;
use crate::flowgraph::config::parse_offset_str;
use crate::flowgraph::node::{
	get_required_datetime, get_required_quantity, get_required_string, ExecFireSet, InputMap, NodeDescriptor,
	NodeExecError, NodeOutput, NodeSpec, PortSpec, PropertySpec, PureNode,
};
use crate::flowgraph::quantity::{Dimension, Quantity, SIPrefix, Unit};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use jiff::tz::{Offset, TimeZone};
use jiff::{SignedDuration, Timestamp};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// `Quantity<time>` または dimensionless Quantity を SI \u{79d2}\u{306b}\u{6b63}\u{898f}\u{5316}\u{3057}\u{3001}
/// `jiff::SignedDuration` に変換する。
///
/// - \u{6b21}\u{5143} = time:
///   `q.as_si_base()` (= value \u{00d7} prefix \u{00d7} si_factor) \u{3092}\u{300c}\u{79d2}\u{300d}\u{3068}\u{3057}\u{3066}\u{63e1}\u{308b}\u{3002}
/// - \u{7121}\u{6b21}\u{5143}:
///   `default_dim_unit` (\u{4f8b}: s / ms) \u{306e}\u{4efa}\u{5b9a}\u{3092}\u{9069}\u{7528}\u{3002}
///   (\u{8a02}\u{6b63}\u{524d}\u{306f} dimensionless \u{3092} `default_dim_unit` \u{306e}\u{6570}\u{5024}\u{3068}\u{898b}\u{306a}\u{3059})
/// - \u{305d}\u{308c}\u{4ee5}\u{5916}\u{306e}\u{6b21}\u{5143}:
///   dimension mismatch error\u{3002}
fn quantity_to_signed_duration(q: &Quantity, op: &str) -> Result<SignedDuration, NodeExecError> {
	let dim = q.dimension();
	let secs = if dim == Dimension::TIME {
		q.as_si_base()
	} else if dim == Dimension::DIMENSIONLESS {
		// dimensionless \u{306f}\u{300c}\u{79d2}\u{300d}\u{3068}\u{3057}\u{3066}\u{6271}\u{3046}\u{3002}Float \u{2192} dimensionless Quantity \u{306e}
		// coerce (\u{3be}-3) \u{304c}\u{3042}\u{308b}\u{306e}\u{3067} "add_duration(Float 3.5)" \u{306f} 3.5s \u{3068}\u{306a}\u{308b}\u{3002}
		q.value
	} else {
		return Err(NodeExecError::Generic(anyhow::anyhow!(
			"{op}: duration \u{306f} time \u{6b21}\u{5143} \u{307e}\u{305f}\u{306f} dimensionless \u{3067}\u{3042}\u{308b}\u{5fc5}\u{8981}\u{304c}\u{3042}\u{308a}\u{307e}\u{3059} (\u{5165}\u{529b} dim: {:?})",
			dim
		)));
	};
	if !secs.is_finite() {
		return Err(NodeExecError::Generic(anyhow::anyhow!(
			"{op}: duration \u{5024}\u{304c}\u{6709}\u{9650}\u{3067}\u{3042}\u{308a}\u{307e}\u{305b}\u{3093} ({secs})"
		)));
	}
	SignedDuration::try_from_secs_f64(secs).map_err(|e| {
		NodeExecError::Generic(anyhow::anyhow!("{op}: duration {secs}s \u{3092} SignedDuration \u{306b}\u{5909}\u{63db}\u{3067}\u{304d}\u{307e}\u{305b}\u{3093}: {e}"))
	})
}

/// `Quantity<time>` または dimensionless Quantity を \u{30df}\u{30ea}\u{79d2} (i64) に\u{6b63}\u{898f}\u{5316}\u{3002}
/// from_epoch_ms \u{5c02}\u{7528}\u{3002}dimensionless \u{306f} ms \u{3068}\u{3057}\u{3066}\u{63e1}\u{308b}\u{3002}
fn quantity_to_epoch_ms(q: &Quantity, op: &str) -> Result<i64, NodeExecError> {
	let dim = q.dimension();
	let ms_f = if dim == Dimension::TIME {
		// SI \u{79d2} \u{2192} ms
		q.as_si_base() * 1_000.0
	} else if dim == Dimension::DIMENSIONLESS {
		q.value
	} else {
		return Err(NodeExecError::Generic(anyhow::anyhow!(
			"{op}: \u{5165}\u{529b}\u{306f} time \u{6b21}\u{5143} \u{307e}\u{305f}\u{306f} dimensionless \u{3067}\u{3042}\u{308b}\u{5fc5}\u{8981}\u{304c}\u{3042}\u{308a}\u{307e}\u{3059} (\u{5165}\u{529b} dim: {:?})",
			dim
		)));
	};
	if !ms_f.is_finite() {
		return Err(NodeExecError::Generic(anyhow::anyhow!(
			"{op}: \u{5024}\u{304c}\u{6709}\u{9650}\u{3067}\u{3042}\u{308a}\u{307e}\u{305b}\u{3093} ({ms_f})"
		)));
	}
	// i64 range \u{30c1}\u{30a7}\u{30c3}\u{30af}
	if ms_f < i64::MIN as f64 || ms_f > i64::MAX as f64 {
		return Err(NodeExecError::Generic(anyhow::anyhow!(
			"{op}: {ms_f} ms \u{306f} i64 \u{7bc4}\u{56f2}\u{5916}"
		)));
	}
	Ok(ms_f as i64)
}

/// Unit: ms (= second with Milli prefix)
fn millisecond_unit() -> Unit {
	Unit::second().with_prefix(SIPrefix::Milli)
}

// ---------------------------------------------------------------------------
// 1. flowgraph.datetime.now
// ---------------------------------------------------------------------------

pub struct DateTimeNowNode;

impl NodeDescriptor for DateTimeNowNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.now".into(),
			title: "DateTime Now".into(),
			category: "datetime".into(),
			description: Some(
				"Emit the current wall-clock time as a DateTime (UTC absolute, nanosecond precision). \
				 \u{975e}\u{6c7a}\u{5b9a}\u{6027} (\u{547c}\u{3073}\u{51fa}\u{3057}\u{3054}\u{3068}\u{306b}\u{7570}\u{306a}\u{308b}\u{7d50}\u{679c}) \u{306a}\u{306e}\u{3067}\u{3001}\u{3059}\u{306a}\u{3086}\u{304f} sample \u{3059}\u{308b}\u{7528}\u{9014}\u{3067}\u{306f}\
				 \u{72b6}\u{614b}\u{5316}\u{30ce}\u{30fc}\u{30c9} (prev_value \u{7b49}) \u{3068}\u{7d44}\u{307f}\u{5408}\u{308f}\u{305b}\u{308b}\u{3053}\u{3068}\u{3002}"
					.into(),
			),
			inputs: vec![],
			outputs: vec![PortSpec::output("datetime", "DateTime", SocketType::DateTime)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeNowNode {
	async fn compute(
		&self,
		_p: &InputMap,
		_inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		Ok(NodeOutput::new().set_data("datetime", SocketValue::DateTime(DateTime::now())))
	}
}

// ---------------------------------------------------------------------------
// 2. flowgraph.datetime.parse
// ---------------------------------------------------------------------------

pub struct DateTimeParseNode;

impl NodeDescriptor for DateTimeParseNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.parse".into(),
			title: "DateTime Parse".into(),
			category: "datetime".into(),
			description: Some(
				"Parse an RFC3339 / ISO 8601 string into a DateTime. Accepts both aware \
				 (\"...Z\" / \"...+09:00\") and naive (\"2026-04-24T12:34:56\") inputs. \
				 Naive \u{5165}\u{529b}\u{306f} `default_timezone` \u{30d7}\u{30ed}\u{30d1}\u{30c6}\u{30a3} (\u{7a7a}\u{306a}\u{3089} UTC) \u{3067}\u{89e3}\u{91c8}\u{3055}\u{308c}\u{308b}\u{3002}\
				 `require_timezone = true` \u{306e}\u{3068}\u{304d}\u{306f} naive \u{3092}\u{62d2}\u{5426}\u{3059}\u{308b} strict \u{30e2}\u{30fc}\u{30c9}\u{3002}"
					.into(),
			),
			inputs: vec![PortSpec::input("s", "String", SocketType::String)],
			outputs: vec![PortSpec::output("datetime", "DateTime", SocketType::DateTime)],
			properties: vec![
				PropertySpec::new(
					"require_timezone",
					"Require timezone",
					SocketType::Bool,
					SocketValue::Bool(false),
				)
				.description(
					"When true, naive (no-timezone) inputs are rejected. \
					 Default false: naive inputs are interpreted with `default_timezone` (or UTC).",
				),
				PropertySpec::new(
					"default_timezone",
					"Default timezone",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description(
					"Fixed offset to apply when the input has no timezone info. \
					 Accepts `\"\"` / `\"Z\"` / `\"UTC\"` (= UTC), `\"+09:00\"`, `\"-05:30\"`. \
					 IANA zones (`\"Asia/Tokyo\"`) are rejected (v0 is fixed-offset only).",
				),
			],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeParseNode {
	async fn compute(
		&self,
		properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let s = get_required_string(inputs, "s")?;
		let require_tz = properties
			.get("require_timezone")
			.and_then(|v| v.as_bool().ok())
			.unwrap_or(false);
		let default_tz_str = properties
			.get("default_timezone")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.to_string();

		if require_tz {
			let dt = DateTime::from_rfc3339(&s).map_err(|e| {
				NodeExecError::Generic(anyhow::anyhow!(
					"datetime.parse (require_timezone=true): '{s}' \u{306f} RFC3339 aware \u{3067}\u{306f}\u{3042}\u{308a}\u{307e}\u{305b}\u{3093}: {e}"
				))
			})?;
			return Ok(NodeOutput::new().set_data("datetime", SocketValue::DateTime(dt)));
		}

		let default_tz = parse_offset_str(&default_tz_str).map_err(|e| {
			NodeExecError::Generic(anyhow::anyhow!(
				"datetime.parse: default_timezone '{default_tz_str}' \u{306e}\u{66f8}\u{5f0f}\u{4e0d}\u{6b63}: {e}"
			))
		})?;
		let dt = DateTime::parse_with_default_tz(&s, default_tz).map_err(|e| {
			NodeExecError::Generic(anyhow::anyhow!("datetime.parse: {e}"))
		})?;
		Ok(NodeOutput::new().set_data("datetime", SocketValue::DateTime(dt)))
	}
}

// ---------------------------------------------------------------------------
// 3. flowgraph.datetime.format
// ---------------------------------------------------------------------------

const FORMAT_CHOICES: &[&str] = &["rfc3339", "iso8601_compact", "unix_seconds", "unix_millis", "custom"];

pub struct DateTimeFormatNode;

impl NodeDescriptor for DateTimeFormatNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.format".into(),
			title: "DateTime Format".into(),
			category: "datetime".into(),
			description: Some(
				"Format a DateTime as a string. \
				 `rfc3339`: `\"2026-04-24T12:34:56.123Z\"` \u{5f62}\u{5f0f} (UTC \u{307e}\u{305f}\u{306f} `timezone` \u{6307}\u{5b9a}\u{6642}\u{306f} offset \u{8868}\u{793a})\u{3002} \
				 `iso8601_compact`: `\"20260424T123456Z\"` \u{5f62}\u{5f0f} (\u{30d5}\u{30a1}\u{30a4}\u{30eb}\u{540d}\u{5411}\u{3051})\u{3002} \
				 `unix_seconds` / `unix_millis`: \u{6574}\u{6570}\u{6587}\u{5b57}\u{5217}\u{3002} \
				 `custom`: `custom_format` \u{30d7}\u{30ed}\u{30d1}\u{30c6}\u{30a3}\u{306e} strftime \u{30d1}\u{30bf}\u{30fc}\u{30f3}\u{3092}\u{9069}\u{7528} (jiff::Zoned::strftime)\u{3002}"
					.into(),
			),
			inputs: vec![PortSpec::input("datetime", "DateTime", SocketType::DateTime)],
			outputs: vec![PortSpec::output("text", "Text", SocketType::String)],
			properties: vec![
				PropertySpec::new(
					"format",
					"Format",
					SocketType::String,
					SocketValue::String("rfc3339".into()),
				)
				.description(
					"Output shape. `rfc3339` / `iso8601_compact` / `unix_seconds` / `unix_millis` / `custom`.",
				)
				.with_choices(FORMAT_CHOICES.iter().map(|s| (*s).to_string())),
				PropertySpec::new(
					"custom_format",
					"Custom strftime",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description(
					"strftime pattern used when `format = \"custom\"`. See jiff::fmt::strtime. \
					 Example: `\"%Y-%m-%d %H:%M:%S\"`.",
				),
				PropertySpec::new(
					"timezone",
					"Timezone",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description(
					"Fixed offset for display (`\"\"` / `\"Z\"` / `\"UTC\"` = UTC, `\"+09:00\"` etc.). \
					 Applies to rfc3339 / iso8601_compact / custom. unix_* are always UTC-absolute and ignore this.",
				),
			],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeFormatNode {
	async fn compute(
		&self,
		properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let dt = get_required_datetime(inputs, "datetime")?;
		let format = properties
			.get("format")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("rfc3339")
			.to_string();
		let custom_format = properties
			.get("custom_format")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.to_string();
		let tz_str = properties
			.get("timezone")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.to_string();

		let offset = parse_offset_str(&tz_str).map_err(|e| {
			NodeExecError::Generic(anyhow::anyhow!(
				"datetime.format: timezone '{tz_str}' \u{306e}\u{66f8}\u{5f0f}\u{4e0d}\u{6b63}: {e}"
			))
		})?;

		let text = match format.as_str() {
			"rfc3339" => format_rfc3339(dt.timestamp(), offset),
			"iso8601_compact" => format_iso8601_compact(dt.timestamp(), offset),
			"unix_seconds" => dt.timestamp().as_second().to_string(),
			"unix_millis" => dt.timestamp().as_millisecond().to_string(),
			"custom" => {
				if custom_format.is_empty() {
					return Err(NodeExecError::Generic(anyhow::anyhow!(
						"datetime.format (format=custom): custom_format \u{30d7}\u{30ed}\u{30d1}\u{30c6}\u{30a3}\u{304c}\u{7a7a}\u{3067}\u{3059}"
					)));
				}
				format_custom(dt.timestamp(), offset, &custom_format)?
			}
			other => {
				return Err(NodeExecError::Generic(anyhow::anyhow!(
					"datetime.format: unknown format '{other}'. Expected one of: {}",
					FORMAT_CHOICES.join(", ")
				)));
			}
		};

		Ok(NodeOutput::new().set_data("text", SocketValue::String(text)))
	}
}

fn format_rfc3339(ts: Timestamp, offset: Offset) -> String {
	if offset == Offset::UTC {
		// jiff::Timestamp::to_string \u{304c} \"...Z\" suffix \u{3092}\u{4ed8}\u{4e0e}\u{3057} subsec \u{3092}\u{81ea}\u{52d5}\u{51fa}\u{3057}\u{3059}\u{308b}\u{3002}
		ts.to_string()
	} else {
		// offset \u{3042}\u{308a}: Zoned \u{7d4c}\u{7531}\u{3067} %Y-%m-%dT%H:%M:%S%.9f%:z \u{3092} strftime
		// subsec \u{304c} 0 \u{306e}\u{3068}\u{304d}\u{3082} %.9f \u{306f}\u{3064}\u{3051}\u{308b} (\u{7d71}\u{4e00}\u{611f})\u{3002}
		// \u{8a73}\u{7d30}\u{3048}\u{3070} %.fN \u{306e}\u{4e26}\u{3073}\u{306f} jiff strtime.
		let zoned = ts.to_zoned(TimeZone::fixed(offset));
		// subsec \u{304c} 0 \u{306a}\u{3089} \"%Y-%m-%dT%H:%M:%S%:z\"\u{3001}\u{975e}\u{30bc}\u{30ed}\u{306a}\u{3089} \".%9f\" \u{3092}\u{633f}\u{5165}\u{3002}
		let ns = zoned.subsec_nanosecond();
		if ns == 0 {
			zoned.strftime("%Y-%m-%dT%H:%M:%S%:z").to_string()
		} else {
			zoned.strftime("%Y-%m-%dT%H:%M:%S.%9f%:z").to_string()
		}
	}
}

fn format_iso8601_compact(ts: Timestamp, offset: Offset) -> String {
	let zoned = ts.to_zoned(TimeZone::fixed(offset));
	if offset == Offset::UTC {
		zoned.strftime("%Y%m%dT%H%M%SZ").to_string()
	} else {
		// compact \u{5f62}\u{5f0f}\u{306f} offset \u{3082} colon \u{7121}\u{3057} (+0900) \u{304c}\u{6163}\u{4f8b}\u{3002}
		zoned.strftime("%Y%m%dT%H%M%S%z").to_string()
	}
}

fn format_custom(ts: Timestamp, offset: Offset, pattern: &str) -> Result<String, NodeExecError> {
	let zoned = ts.to_zoned(TimeZone::fixed(offset));
	// jiff::Zoned::strftime \u{306f}\u{76f4}\u{63a5} panic \u{3057}\u{306a}\u{3044}\u{304c}\u{3001}Display \u{304c}\u{5931}\u{6557}\u{3059}\u{308b}\u{3053}\u{3068}\u{304c}\u{3042}\u{308b}\u{3002}
	// Display \u{306e}\u{5931}\u{6557} = invalid \u{306a} conversion specifier \u{306a}\u{3069}\u{3002}
	use std::fmt::Write;
	let mut buf = String::new();
	write!(&mut buf, "{}", zoned.strftime(pattern)).map_err(|e| {
		NodeExecError::Generic(anyhow::anyhow!(
			"datetime.format (custom): strftime pattern '{pattern}' \u{306f}\u{4e0d}\u{6b63}\u{3067}\u{3059}: {e}"
		))
	})?;
	Ok(buf)
}

// ---------------------------------------------------------------------------
// 4. flowgraph.datetime.add_duration
// ---------------------------------------------------------------------------

pub struct DateTimeAddDurationNode;

impl NodeDescriptor for DateTimeAddDurationNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.add_duration".into(),
			title: "DateTime + Duration".into(),
			category: "datetime".into(),
			description: Some(
				"Add a duration (Quantity<time>) to a DateTime. \
				 Dimensionless Quantity (Float \u{306e} \u{03BE}-3 coerce \u{7d4c}\u{7531}) \u{306f}\u{300c}\u{79d2}\u{300d}\u{3068}\u{89e3}\u{91c8}\u{3055}\u{308c}\u{308b}\u{3002}\
				 \u{6b21}\u{5143}\u{4e0d}\u{4e00}\u{81f4} (\u{4f8b}: length) \u{306f} error\u{3002}"
					.into(),
			),
			inputs: vec![
				PortSpec::input("datetime", "DateTime", SocketType::DateTime),
				PortSpec::input("duration", "Duration", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::DateTime)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeAddDurationNode {
	async fn compute(
		&self,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let dt = get_required_datetime(inputs, "datetime")?;
		let q = get_required_quantity(inputs, "duration")?;
		let d = quantity_to_signed_duration(q, "datetime.add_duration")?;
		let result = dt.timestamp().checked_add(d).map_err(|e| {
			NodeExecError::Generic(anyhow::anyhow!("datetime.add_duration: overflow: {e}"))
		})?;
		Ok(NodeOutput::new().set_data("result", SocketValue::DateTime(DateTime::from(result))))
	}
}

// ---------------------------------------------------------------------------
// 5. flowgraph.datetime.sub_duration
// ---------------------------------------------------------------------------

pub struct DateTimeSubDurationNode;

impl NodeDescriptor for DateTimeSubDurationNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.sub_duration".into(),
			title: "DateTime - Duration".into(),
			category: "datetime".into(),
			description: Some(
				"Subtract a duration (Quantity<time>) from a DateTime. \
				 Dimensionless Quantity \u{306f}\u{300c}\u{79d2}\u{300d}\u{3068}\u{89e3}\u{91c8}\u{3055}\u{308c}\u{308b} (\u{03BE}-3 coerce)\u{3002}"
					.into(),
			),
			inputs: vec![
				PortSpec::input("datetime", "DateTime", SocketType::DateTime),
				PortSpec::input("duration", "Duration", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::DateTime)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeSubDurationNode {
	async fn compute(
		&self,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let dt = get_required_datetime(inputs, "datetime")?;
		let q = get_required_quantity(inputs, "duration")?;
		let d = quantity_to_signed_duration(q, "datetime.sub_duration")?;
		let result = dt.timestamp().checked_sub(d).map_err(|e| {
			NodeExecError::Generic(anyhow::anyhow!("datetime.sub_duration: overflow: {e}"))
		})?;
		Ok(NodeOutput::new().set_data("result", SocketValue::DateTime(DateTime::from(result))))
	}
}

// ---------------------------------------------------------------------------
// 6. flowgraph.datetime.diff
// ---------------------------------------------------------------------------

pub struct DateTimeDiffNode;

impl NodeDescriptor for DateTimeDiffNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.diff".into(),
			title: "DateTime - DateTime".into(),
			category: "datetime".into(),
			description: Some(
				"Compute `lhs - rhs` as a Quantity<time> (unit: seconds, nanosecond precision). \
				 \u{7d50}\u{679c}\u{306f}\u{6b63}\u{8ca0} OK\u{3002}`flowgraph.unit.convert` \u{3067} ms / us / ns \u{306b}\u{5909}\u{63db}\u{53ef}\u{80fd}\u{3002}"
					.into(),
			),
			inputs: vec![
				PortSpec::input("lhs", "Lhs", SocketType::DateTime),
				PortSpec::input("rhs", "Rhs", SocketType::DateTime),
			],
			outputs: vec![PortSpec::output("duration", "Duration", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeDiffNode {
	async fn compute(
		&self,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let lhs = get_required_datetime(inputs, "lhs")?;
		let rhs = get_required_datetime(inputs, "rhs")?;
		let d: SignedDuration = lhs.timestamp().duration_since(rhs.timestamp());
		// ns \u{7cbe}\u{5ea6}\u{3092}\u{4fdd}\u{6301}\u{3057}\u{3064}\u{3064} f64 \u{79d2}\u{306b}\u{5909}\u{63db}\u{3002}
		// f64 \u{79d2} = secs + ns * 1e-9\u{3002}\u{7d20}\u{76f4}\u{306b} as_secs_f64() \u{3082}\u{53ef}\u{3060}\u{304c}\u{3001}\u{5fb4}\u{5999}\u{306a}\u{4e38}\u{3081}\u{3092}\u{56de}\u{907f}\u{3059}\u{308b}\u{305f}\u{3081}\u{81ea}\u{4f9c}\u{3067}\u{7d44}\u{3080}\u{3002}
		let secs = d.as_secs() as f64 + (d.subsec_nanos() as f64) / 1_000_000_000.0;
		let q = Quantity::of(secs, Unit::second());
		Ok(NodeOutput::new().set_data("duration", SocketValue::Quantity(q)))
	}
}

// ---------------------------------------------------------------------------
// 7. flowgraph.datetime.epoch_ms
// ---------------------------------------------------------------------------

pub struct DateTimeEpochMsNode;

impl NodeDescriptor for DateTimeEpochMsNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.epoch_ms".into(),
			title: "DateTime -> Epoch ms".into(),
			category: "datetime".into(),
			description: Some(
				"Return Unix epoch milliseconds as a Quantity (unit: ms, dim: time). \
				 Negative for pre-1970 timestamps. \u{4ed6}\u{306e}\u{6642}\u{9593}\u{5358}\u{4f4d}\u{3078}\u{306f} `flowgraph.unit.convert` \u{3067}\u{5909}\u{63db}\u{3002}"
					.into(),
			),
			inputs: vec![PortSpec::input("datetime", "DateTime", SocketType::DateTime)],
			outputs: vec![PortSpec::output("millis", "Millis", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeEpochMsNode {
	async fn compute(
		&self,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let dt = get_required_datetime(inputs, "datetime")?;
		let ms = dt.timestamp().as_millisecond();
		let q = Quantity::of(ms as f64, millisecond_unit());
		Ok(NodeOutput::new().set_data("millis", SocketValue::Quantity(q)))
	}
}

// ---------------------------------------------------------------------------
// 8. flowgraph.datetime.from_epoch_ms
// ---------------------------------------------------------------------------

pub struct DateTimeFromEpochMsNode;

impl NodeDescriptor for DateTimeFromEpochMsNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.datetime.from_epoch_ms".into(),
			title: "Epoch ms -> DateTime".into(),
			category: "datetime".into(),
			description: Some(
				"Construct a DateTime from Unix epoch milliseconds. \
				 Quantity<time> (\u{4efb}\u{610f}\u{306e}\u{6642}\u{9593}\u{5358}\u{4f4d}) \u{306f} SI \u{79d2} \u{2192} ms \u{306b}\u{6b63}\u{898f}\u{5316}\u{3055}\u{308c}\u{3066}\u{53d7}\u{7406}\u{3055}\u{308c}\u{308b}\u{3002} \
				 Dimensionless Quantity (Float \u{306e} \u{03BE}-3 coerce \u{7d4c}\u{7531}) \u{306f}\u{300c}ms \u{306e}\u{6570}\u{5024}\u{300d}\u{3068}\u{3057}\u{3066}\u{89e3}\u{91c8}\u{3055}\u{308c}\u{308b}\u{3002}"
					.into(),
			),
			inputs: vec![PortSpec::input("millis", "Millis", SocketType::Quantity)],
			outputs: vec![PortSpec::output("datetime", "DateTime", SocketType::DateTime)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for DateTimeFromEpochMsNode {
	async fn compute(
		&self,
		_p: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "millis")?;
		let ms = quantity_to_epoch_ms(q, "datetime.from_epoch_ms")?;
		let ts = Timestamp::from_millisecond(ms).map_err(|e| {
			NodeExecError::Generic(anyhow::anyhow!(
				"datetime.from_epoch_ms: {ms} ms \u{306f} Timestamp \u{7bc4}\u{56f2}\u{5916}: {e}"
			))
		})?;
		Ok(NodeOutput::new().set_data("datetime", SocketValue::DateTime(DateTime::from(ts))))
	}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	fn dt_input(key: &str, s: &str) -> (String, SocketValue) {
		(key.to_string(), SocketValue::DateTime(DateTime::from_rfc3339(s).unwrap()))
	}

	fn q_input(key: &str, value: f64, unit: Unit) -> (String, SocketValue) {
		(key.to_string(), SocketValue::Quantity(Quantity::of(value, unit)))
	}

	fn s_input(key: &str, v: &str) -> (String, SocketValue) {
		(key.to_string(), SocketValue::String(v.into()))
	}

	fn props(pairs: &[(&str, SocketValue)]) -> InputMap {
		pairs.iter().map(|(k, v)| ((*k).to_string(), v.clone())).collect()
	}

	// ----- 1. now -----

	#[tokio::test]
	async fn now_returns_datetime_close_to_system_clock() {
		let out = DateTimeNowNode
			.compute(&InputMap::new(), &InputMap::new(), &ExecFireSet::new())
			.await
			.unwrap();
		let dt = out.data.get("datetime").unwrap().as_datetime().unwrap().clone();
		let delta = DateTime::now().timestamp().duration_since(dt.timestamp());
		// \u{307b}\u{3068}\u{3093}\u{3069} 0 \u{306b}\u{8fd1}\u{3044}\u{306f}\u{305a} (\u{3054}\u{304f}\u{307e}\u{308c}\u{306b} 0.1s \u{672a}\u{6e80})
		assert!(delta.as_secs().abs() < 2, "now diff too large: {delta:?}");
	}

	// ----- 2. parse -----

	#[tokio::test]
	async fn parse_rfc3339_utc() {
		let out = DateTimeParseNode
			.compute(
				&props(&[
					("require_timezone", SocketValue::Bool(false)),
					("default_timezone", SocketValue::String(String::new())),
				]),
				&[s_input("s", "2026-04-24T12:34:56Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("datetime").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:56Z");
	}

	#[tokio::test]
	async fn parse_rfc3339_offset_normalizes_to_utc() {
		let out = DateTimeParseNode
			.compute(
				&props(&[]),
				&[s_input("s", "2026-04-24T21:34:56+09:00")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("datetime").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:56Z");
	}

	#[tokio::test]
	async fn parse_naive_with_jst_default() {
		let out = DateTimeParseNode
			.compute(
				&props(&[("default_timezone", SocketValue::String("+09:00".into()))]),
				&[s_input("s", "2026-04-24T12:34:56")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("datetime").unwrap().as_datetime().unwrap().clone();
		// JST \u{306e} 12:34 \u{306f} UTC 03:34
		assert_eq!(dt.to_rfc3339(), "2026-04-24T03:34:56Z");
	}

	#[tokio::test]
	async fn parse_require_timezone_rejects_naive() {
		let r = DateTimeParseNode
			.compute(
				&props(&[("require_timezone", SocketValue::Bool(true))]),
				&[s_input("s", "2026-04-24T12:34:56")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await;
		assert!(r.is_err(), "require_timezone=true \u{3067} naive \u{5165}\u{529b}\u{306f} error \u{306b}\u{306a}\u{308b}\u{306f}\u{305a}");
	}

	#[tokio::test]
	async fn parse_iana_timezone_rejected_as_default() {
		let r = DateTimeParseNode
			.compute(
				&props(&[("default_timezone", SocketValue::String("Asia/Tokyo".into()))]),
				&[s_input("s", "2026-04-24T12:34:56")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await;
		assert!(r.is_err(), "IANA zone \u{306f} v0 \u{3067}\u{306f} reject");
	}

	// ----- 3. format -----

	#[tokio::test]
	async fn format_rfc3339_default_utc() {
		let out = DateTimeFormatNode
			.compute(
				&props(&[("format", SocketValue::String("rfc3339".into()))]),
				&[dt_input("datetime", "2026-04-24T12:34:56Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(
			out.data.get("text").unwrap().as_str().unwrap(),
			"2026-04-24T12:34:56Z"
		);
	}

	#[tokio::test]
	async fn format_rfc3339_jst_offset() {
		let out = DateTimeFormatNode
			.compute(
				&props(&[
					("format", SocketValue::String("rfc3339".into())),
					("timezone", SocketValue::String("+09:00".into())),
				]),
				&[dt_input("datetime", "2026-04-24T12:34:56Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(
			out.data.get("text").unwrap().as_str().unwrap(),
			"2026-04-24T21:34:56+09:00"
		);
	}

	#[tokio::test]
	async fn format_iso8601_compact_utc() {
		let out = DateTimeFormatNode
			.compute(
				&props(&[("format", SocketValue::String("iso8601_compact".into()))]),
				&[dt_input("datetime", "2026-04-24T12:34:56Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(
			out.data.get("text").unwrap().as_str().unwrap(),
			"20260424T123456Z"
		);
	}

	#[tokio::test]
	async fn format_unix_seconds() {
		let out = DateTimeFormatNode
			.compute(
				&props(&[("format", SocketValue::String("unix_seconds".into()))]),
				&[dt_input("datetime", "1970-01-01T00:00:10Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("text").unwrap().as_str().unwrap(), "10");
	}

	#[tokio::test]
	async fn format_unix_millis() {
		let out = DateTimeFormatNode
			.compute(
				&props(&[("format", SocketValue::String("unix_millis".into()))]),
				&[dt_input("datetime", "1970-01-01T00:00:01.234Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("text").unwrap().as_str().unwrap(), "1234");
	}

	#[tokio::test]
	async fn format_custom_strftime() {
		let out = DateTimeFormatNode
			.compute(
				&props(&[
					("format", SocketValue::String("custom".into())),
					("custom_format", SocketValue::String("%Y/%m/%d %H:%M:%S".into())),
					("timezone", SocketValue::String("+09:00".into())),
				]),
				&[dt_input("datetime", "2026-04-24T12:34:56Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(
			out.data.get("text").unwrap().as_str().unwrap(),
			"2026/04/24 21:34:56"
		);
	}

	#[tokio::test]
	async fn format_custom_requires_pattern() {
		let r = DateTimeFormatNode
			.compute(
				&props(&[("format", SocketValue::String("custom".into()))]),
				&[dt_input("datetime", "2026-04-24T12:34:56Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await;
		assert!(r.is_err(), "custom \u{30e2}\u{30fc}\u{30c9}\u{3067} custom_format \u{304c}\u{7a7a}\u{306a}\u{3089} error");
	}

	#[tokio::test]
	async fn format_unknown_errs() {
		let r = DateTimeFormatNode
			.compute(
				&props(&[("format", SocketValue::String("nonsense".into()))]),
				&[dt_input("datetime", "2026-04-24T12:34:56Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await;
		assert!(r.is_err());
	}

	// ----- 4. add_duration -----

	#[tokio::test]
	async fn add_duration_seconds() {
		let out = DateTimeAddDurationNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("datetime", "2026-04-24T12:34:56Z"),
					q_input("duration", 60.0, Unit::second()),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("result").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:35:56Z");
	}

	#[tokio::test]
	async fn add_duration_milliseconds_via_unit() {
		let out = DateTimeAddDurationNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("datetime", "2026-04-24T12:34:56Z"),
					q_input("duration", 1500.0, millisecond_unit()),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("result").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:34:57.5Z");
	}

	#[tokio::test]
	async fn add_duration_dimensionless_treated_as_seconds() {
		let out = DateTimeAddDurationNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("datetime", "2026-04-24T12:34:56Z"),
					q_input("duration", 3600.0, Unit::dimensionless()),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("result").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T13:34:56Z");
	}

	#[tokio::test]
	async fn add_duration_rejects_length_dim() {
		let r = DateTimeAddDurationNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("datetime", "2026-04-24T12:34:56Z"),
					q_input("duration", 1.0, Unit::metre()),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await;
		assert!(r.is_err(), "length \u{6b21}\u{5143}\u{306e} duration \u{306f} error");
	}

	// ----- 5. sub_duration -----

	#[tokio::test]
	async fn sub_duration_seconds() {
		let out = DateTimeSubDurationNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("datetime", "2026-04-24T12:34:56Z"),
					q_input("duration", 120.0, Unit::second()),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("result").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "2026-04-24T12:32:56Z");
	}

	#[tokio::test]
	async fn sub_duration_rejects_mass_dim() {
		let r = DateTimeSubDurationNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("datetime", "2026-04-24T12:34:56Z"),
					q_input("duration", 1.0, Unit::kilogram()),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await;
		assert!(r.is_err());
	}

	// ----- 6. diff -----

	#[tokio::test]
	async fn diff_positive_seconds() {
		let out = DateTimeDiffNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("lhs", "2026-04-24T12:35:56Z"),
					dt_input("rhs", "2026-04-24T12:34:56Z"),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let q = out.data.get("duration").unwrap().as_quantity().unwrap().clone();
		assert_eq!(q.dimension(), Dimension::TIME);
		assert!((q.value - 60.0).abs() < 1e-9, "got {}", q.value);
	}

	#[tokio::test]
	async fn diff_negative_when_lhs_before_rhs() {
		let out = DateTimeDiffNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("lhs", "2026-04-24T12:34:00Z"),
					dt_input("rhs", "2026-04-24T12:34:30Z"),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let q = out.data.get("duration").unwrap().as_quantity().unwrap().clone();
		assert!((q.value + 30.0).abs() < 1e-9, "got {}", q.value);
	}

	#[tokio::test]
	async fn diff_preserves_nanosecond_precision() {
		let out = DateTimeDiffNode
			.compute(
				&InputMap::new(),
				&[
					dt_input("lhs", "2026-04-24T12:34:56.500000000Z"),
					dt_input("rhs", "2026-04-24T12:34:56.000000000Z"),
				]
				.into_iter()
				.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let q = out.data.get("duration").unwrap().as_quantity().unwrap().clone();
		assert!((q.value - 0.5).abs() < 1e-12, "got {}", q.value);
	}

	// ----- 7. epoch_ms -----

	#[tokio::test]
	async fn epoch_ms_unix_epoch_is_zero() {
		let out = DateTimeEpochMsNode
			.compute(
				&InputMap::new(),
				&[dt_input("datetime", "1970-01-01T00:00:00Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let q = out.data.get("millis").unwrap().as_quantity().unwrap().clone();
		assert_eq!(q.value, 0.0);
		assert_eq!(q.dimension(), Dimension::TIME);
	}

	#[tokio::test]
	async fn epoch_ms_positive_value() {
		let out = DateTimeEpochMsNode
			.compute(
				&InputMap::new(),
				&[dt_input("datetime", "1970-01-01T00:00:01.234Z")].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let q = out.data.get("millis").unwrap().as_quantity().unwrap().clone();
		assert_eq!(q.value, 1234.0);
	}

	// ----- 8. from_epoch_ms -----

	#[tokio::test]
	async fn from_epoch_ms_with_ms_unit() {
		let out = DateTimeFromEpochMsNode
			.compute(
				&InputMap::new(),
				&[q_input("millis", 1234.0, millisecond_unit())].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("datetime").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "1970-01-01T00:00:01.234Z");
	}

	#[tokio::test]
	async fn from_epoch_ms_with_seconds_unit_normalizes() {
		let out = DateTimeFromEpochMsNode
			.compute(
				&InputMap::new(),
				&[q_input("millis", 10.0, Unit::second())].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("datetime").unwrap().as_datetime().unwrap().clone();
		// 10 s = 10000 ms = 1970-01-01T00:00:10Z
		assert_eq!(dt.to_rfc3339(), "1970-01-01T00:00:10Z");
	}

	#[tokio::test]
	async fn from_epoch_ms_dimensionless_treated_as_ms() {
		let out = DateTimeFromEpochMsNode
			.compute(
				&InputMap::new(),
				&[q_input("millis", 5000.0, Unit::dimensionless())].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out.data.get("datetime").unwrap().as_datetime().unwrap().clone();
		assert_eq!(dt.to_rfc3339(), "1970-01-01T00:00:05Z");
	}

	#[tokio::test]
	async fn from_epoch_ms_rejects_length() {
		let r = DateTimeFromEpochMsNode
			.compute(
				&InputMap::new(),
				&[q_input("millis", 1.0, Unit::metre())].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await;
		assert!(r.is_err());
	}

	#[tokio::test]
	async fn from_epoch_ms_then_epoch_ms_roundtrip() {
		// from_epoch_ms \u{2192} epoch_ms \u{3067} ms \u{7cbe}\u{5ea6}\u{3092}\u{5b88}\u{308b}\u{3002}
		let out1 = DateTimeFromEpochMsNode
			.compute(
				&InputMap::new(),
				&[q_input("millis", 1_714__000_000_123.0, millisecond_unit())]
					.into_iter()
					.collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let dt = out1.data.get("datetime").unwrap().clone();
		let out2 = DateTimeEpochMsNode
			.compute(
				&InputMap::new(),
				&[("datetime".to_string(), dt)].into_iter().collect(),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let q = out2.data.get("millis").unwrap().as_quantity().unwrap().clone();
		assert_eq!(q.value, 1_714__000_000_123.0);
	}
}
