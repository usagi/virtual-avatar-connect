//! `flowgraph.util.format` PureNode (Phase \u{3be}-4).
//!
//! Quantity -> String formatter with explicit control over unit inclusion and
//! numeric precision. Use when the default `Quantity -> String` engine coerce
//! (`"{value} {unit}"` via `Quantity`'s Display impl) is not enough.
//!
//! Ports:
//! - input `value`      : Quantity (required)
//! - output `result`    : String
//!
//! Properties:
//! - `include_unit`     : Bool (default true) - append " {unit}" when non-dimensionless
//! - `precision`        : Int  (default -1)   - decimal places, -1 = default Display
//! - `unit_override`    : String (default "") - if non-empty, convert to this unit before formatting
//!                                              (same-dimension-only, errors if mismatched)
//!
//! On dimensionless Quantity, `include_unit` is ignored (no unit to append).

use crate::flowgraph::node::{
	get_required_quantity, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PropertySpec, PureNode,
};
use crate::flowgraph::quantity::parse_unit;
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct UtilFormatNode;

impl NodeDescriptor for UtilFormatNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.util.format".into(),
			title: "Format Quantity".into(),
			category: "util".into(),
			description: Some(
				"Quantity \u{2192} String with explicit include_unit / precision / unit_override control. \
				 Default output is \"{value} {unit}\" matching the engine-level Quantity \u{2192} String coerce."
					.into(),
			),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Quantity)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::String)],
			properties: vec![
				PropertySpec::new("include_unit", "Include Unit", SocketType::Bool, SocketValue::Bool(true))
					.description("Append \" {unit}\" suffix when the value is non-dimensionless."),
				PropertySpec::new("precision", "Precision", SocketType::Int, SocketValue::Int(-1))
					.description("Decimal places for the numeric part. -1 means use the default Display formatter (no forced precision)."),
				PropertySpec::new(
					"unit_override",
					"Unit Override",
					SocketType::String,
					SocketValue::String(String::new()),
				)
				.description(
					"If non-empty, convert the Quantity to this unit before formatting (same-dimension only, \
					 errors otherwise). Useful for rendering in a different unit than upstream.",
				),
			],
		}
	}
}

#[async_trait]
impl PureNode for UtilFormatNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "value")?;
		let include_unit = properties.get("include_unit").and_then(|v| v.as_bool().ok()).unwrap_or(true);
		let precision: i64 = properties.get("precision").and_then(|v| v.as_i64().ok()).unwrap_or(-1);
		let unit_override = properties
			.get("unit_override")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.trim()
			.to_string();

		let q_owned;
		let q_ref = if unit_override.is_empty() {
			q
		} else {
			let target = parse_unit(&unit_override)
				.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("unit_override parse error on '{unit_override}': {e}")))?;
			q_owned = q
				.convert_to(&target)
				.map_err(|e| NodeExecError::Generic(anyhow::anyhow!("unit_override convert failed: {e}")))?;
			&q_owned
		};

		let value_str = if precision < 0 {
			format!("{}", q_ref.value)
		} else {
			format!("{:.*}", precision as usize, q_ref.value)
		};

		let out = if include_unit && !q_ref.is_dimensionless() {
			format!("{} {}", value_str, q_ref.unit.canonical())
		} else {
			value_str
		};

		Ok(NodeOutput::new().set_data("result", SocketValue::String(out)))
	}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::quantity::{parse_unit, Quantity};

	fn props(include_unit: bool, precision: i64, unit_override: &str) -> InputMap {
		[
			("include_unit".into(), SocketValue::Bool(include_unit)),
			("precision".into(), SocketValue::Int(precision)),
			("unit_override".into(), SocketValue::String(unit_override.into())),
		]
		.into_iter()
		.collect()
	}

	fn input_quantity(q: Quantity) -> InputMap {
		[("value".into(), SocketValue::Quantity(q))].into_iter().collect()
	}

	#[tokio::test]
	async fn default_format_with_unit() {
		let q = Quantity::of(42.5, parse_unit("m/s^2").unwrap());
		let out = UtilFormatNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&props(true, -1, ""),
				&input_quantity(q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let s = out.data.get("result").and_then(|v| v.as_str().ok()).unwrap().to_string();
		assert!(s.starts_with("42.5"), "got: {s}");
		assert!(s.contains("m") && s.contains("s"));
	}

	#[tokio::test]
	async fn include_unit_false_strips_unit() {
		let q = Quantity::of(3.14, parse_unit("m").unwrap());
		let out = UtilFormatNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&props(false, -1, ""),
				&input_quantity(q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let s = out.data.get("result").and_then(|v| v.as_str().ok()).unwrap().to_string();
		assert_eq!(s, "3.14");
	}

	#[tokio::test]
	async fn precision_applies() {
		let q = Quantity::of(1.0_f64 / 3.0, parse_unit("m").unwrap());
		let out = UtilFormatNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&props(true, 3, ""),
				&input_quantity(q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let s = out.data.get("result").and_then(|v| v.as_str().ok()).unwrap().to_string();
		assert_eq!(s, "0.333 m");
	}

	#[tokio::test]
	async fn unit_override_converts_before_format() {
		// 180 deg -> pi rad
		let q = Quantity::of(180.0, parse_unit("deg").unwrap());
		let out = UtilFormatNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&props(true, 4, "rad"),
				&input_quantity(q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let s = out.data.get("result").and_then(|v| v.as_str().ok()).unwrap().to_string();
		assert_eq!(s, "3.1416 rad");
	}

	#[tokio::test]
	async fn unit_override_dim_mismatch_errors() {
		let q = Quantity::of(1.0, parse_unit("m").unwrap());
		let e = UtilFormatNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&props(true, -1, "s"),
				&input_quantity(q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)), "got {e:?}");
	}

	#[tokio::test]
	async fn dimensionless_never_appends_unit() {
		let q = Quantity::dimensionless(7.0);
		let out = UtilFormatNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&props(true, 2, ""),
				&input_quantity(q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let s = out.data.get("result").and_then(|v| v.as_str().ok()).unwrap().to_string();
		assert_eq!(s, "7.00");
	}
}
