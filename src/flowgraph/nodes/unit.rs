//! Unit-aware PureNode group (Phase \u{3be}-2).
//!
//! `flowgraph.unit.*` namespace. Operates on the `SocketValue::Quantity` variant
//! introduced alongside this commit. See `docs/roadmap/phase-ksi-dimensional-quantity-system.md`
//! \u{a7}4.1 for the full spec.
//!
//! Nodes:
//! - `flowgraph.unit.assign`          Float (dimensionless) + property `unit` -> Quantity
//! - `flowgraph.unit.try_parse`       String -> result<Quantity>
//! - `flowgraph.unit.convert`         Quantity + property `target_unit` -> Quantity (same-dim only)
//! - `flowgraph.unit.strip`           Quantity -> Float (raw value, dimensionless escape hatch)
//! - `flowgraph.unit.get_unit_string` Quantity -> String (canonical unit name)
//! - `flowgraph.unit.get_dim_string`  Quantity -> String (dimension canonical, e.g. `L\u{b7}T^-2`)
//! - `flowgraph.unit.same_dimension`  (Quantity, Quantity) -> Bool
//! - `flowgraph.unit.to_json`         Quantity -> Json `{value, unit, dimension}`
//!
//! All pure; no exec ports (D4: dimension errors halt the graph via `NodeExecError::Generic`).

use crate::flowgraph::node::{
	get_required_float, get_required_quantity, get_required_string, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput,
	NodeSpec, PortSpec, PropertySpec, PureNode,
};
use crate::flowgraph::quantity::{parse_unit, Quantity};
use crate::flowgraph::socket::{parse_quantity_string, FlowResult, SocketType, SocketValue};
use async_trait::async_trait;

// ---------------------------------------------------------------------------
// assign: dimensionless float + unit string -> Quantity
// ---------------------------------------------------------------------------

pub struct UnitAssignNode;

impl NodeDescriptor for UnitAssignNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.assign".into(),
			title: "Unit Assign".into(),
			category: "unit".into(),
			description: Some("Attach a unit to a dimensionless Float and produce a Quantity.".into()),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Float)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![
				PropertySpec::new("unit", "Unit", SocketType::String, SocketValue::String(String::new()))
					.description("SI-compatible unit string, e.g. \"m/s^2\", \"Hz\", \"kg\". Empty = dimensionless."),
			],
		}
	}
}

#[async_trait]
impl PureNode for UnitAssignNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let value = get_required_float(inputs, "value")?;
		let unit_str = unit_property(properties, "unit");
		let unit = parse_assign_unit(&unit_str).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(Quantity::of(value, unit))))
	}
}

pub struct UnitTryParseNode;

impl NodeDescriptor for UnitTryParseNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.try_parse".into(),
			title: "Unit Try Parse".into(),
			category: "unit".into(),
			description: Some("Parse a quantity string and return failure as result<quantity> instead of halting.".into()),
			inputs: vec![PortSpec::input("text", "Text", SocketType::String)],
			outputs: vec![
				PortSpec::output("ok", "OK", SocketType::Bool),
				PortSpec::output("quantity", "Quantity", SocketType::Quantity),
				PortSpec::output("error", "Error", SocketType::String),
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Quantity))),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for UnitTryParseNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let text = get_required_string(inputs, "text")?;
		match parse_quantity_string(&text) {
			Ok(quantity) => Ok(NodeOutput::new()
				.set_data("ok", SocketValue::Bool(true))
				.set_data("quantity", SocketValue::Quantity(quantity.clone()))
				.set_data("error", SocketValue::String(String::new()))
				.set_data("result", SocketValue::Result(FlowResult::ok(SocketValue::Quantity(quantity))))),
			Err(error) => {
				let error = format!("quantity parse error: {error}");
				Ok(NodeOutput::new()
					.set_data("ok", SocketValue::Bool(false))
					.set_data("quantity", SocketType::Quantity.default_value().unwrap())
					.set_data("error", SocketValue::String(error.clone()))
					.set_data("result", SocketValue::Result(FlowResult::err(error).with_code("unit.parse"))))
			}
		}
	}
}

fn unit_property(properties: &InputMap, name: &str) -> String {
	properties.get(name).and_then(|v| v.as_str().ok()).unwrap_or("").trim().to_string()
}

fn parse_assign_unit(unit_str: &str) -> Result<crate::flowgraph::quantity::Unit, String> {
	if unit_str.is_empty() {
		Ok(crate::flowgraph::quantity::Unit::dimensionless())
	} else {
		parse_unit(unit_str).map_err(|e| format!("unit parse error on '{unit_str}': {e}"))
	}
}

// ---------------------------------------------------------------------------
// convert: Quantity + target unit string -> Quantity (same dim only)
// ---------------------------------------------------------------------------

pub struct UnitConvertNode;

impl NodeDescriptor for UnitConvertNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.convert".into(),
			title: "Unit Convert".into(),
			category: "unit".into(),
			description: Some(
				"Convert a Quantity to the target unit. Errors if dimensions differ or K/\u{394}K semantics mismatch.".into(),
			),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Quantity)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![
				PropertySpec::new("target_unit", "Target Unit", SocketType::String, SocketValue::String(String::new()))
					.required()
					.description("Target unit string. Must match the input dimension."),
			],
		}
	}
}

#[async_trait]
impl PureNode for UnitConvertNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "value")?.clone();
		let target_str = properties
			.get("target_unit")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("")
			.trim()
			.to_string();
		if target_str.is_empty() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"flowgraph.unit.convert: property 'target_unit' is required (non-empty)"
			)));
		}
		let target =
			parse_unit(&target_str).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("unit parse error on '{target_str}': {e}")))?;
		let converted = q.convert_to(&target).map_err(|e| NodeExecError::Generic(anyhow::anyhow!("{e}")))?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(converted)))
	}
}

// ---------------------------------------------------------------------------
// strip: Quantity -> Float (raw value, dimensionless escape hatch)
// ---------------------------------------------------------------------------

pub struct UnitStripNode;

impl NodeDescriptor for UnitStripNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.strip".into(),
			title: "Unit Strip".into(),
			category: "unit".into(),
			description: Some("Explicit escape hatch: discard the unit and emit the raw numeric value as Float.".into()),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Quantity)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Float)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for UnitStripNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "value")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Float(q.value)))
	}
}

// ---------------------------------------------------------------------------
// get_unit_string: Quantity -> String (canonical unit form)
// ---------------------------------------------------------------------------

pub struct UnitGetUnitStringNode;

impl NodeDescriptor for UnitGetUnitStringNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.get_unit_string".into(),
			title: "Unit -> String".into(),
			category: "unit".into(),
			description: Some("Return the canonical unit string (e.g. \"m/s^2\", \"Hz\").".into()),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Quantity)],
			outputs: vec![PortSpec::output("name", "Name", SocketType::String)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for UnitGetUnitStringNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "value")?;
		Ok(NodeOutput::new().set_data("name", SocketValue::String(q.unit.canonical())))
	}
}

// ---------------------------------------------------------------------------
// get_dim_string: Quantity -> String (dimension canonical)
// ---------------------------------------------------------------------------

pub struct UnitGetDimensionStringNode;

impl NodeDescriptor for UnitGetDimensionStringNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.get_dim_string".into(),
			title: "Dimension -> String".into(),
			category: "unit".into(),
			description: Some("Return the canonical dimension string (e.g. \"L\u{b7}T^-2\").".into()),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Quantity)],
			outputs: vec![PortSpec::output("dim", "Dimension", SocketType::String)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for UnitGetDimensionStringNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "value")?;
		Ok(NodeOutput::new().set_data("dim", SocketValue::String(q.dimension().canonical())))
	}
}

// ---------------------------------------------------------------------------
// same_dimension: (Quantity, Quantity) -> Bool
// ---------------------------------------------------------------------------

pub struct UnitSameDimensionNode;

impl NodeDescriptor for UnitSameDimensionNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.same_dimension".into(),
			title: "Same Dimension?".into(),
			category: "unit".into(),
			description: Some("True iff both inputs carry the same physical dimension.".into()),
			inputs: vec![
				PortSpec::input("a", "A", SocketType::Quantity),
				PortSpec::input("b", "B", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Bool)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for UnitSameDimensionNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let a = get_required_quantity(inputs, "a")?;
		let b = get_required_quantity(inputs, "b")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Bool(a.dimension() == b.dimension())))
	}
}

// ---------------------------------------------------------------------------
// to_json: Quantity -> Json `{value, unit, dimension}` (internal form)
// ---------------------------------------------------------------------------

pub struct UnitToJsonNode;

impl NodeDescriptor for UnitToJsonNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.unit.to_json".into(),
			title: "Unit -> JSON".into(),
			category: "unit".into(),
			description: Some("Serialize Quantity to JSON with `value`, `unit`, `dimension` fields (internal form).".into()),
			inputs: vec![PortSpec::input("value", "Value", SocketType::Quantity)],
			outputs: vec![PortSpec::output("json", "JSON", SocketType::Json)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for UnitToJsonNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "value")?;
		let j = serde_json::json!({
			"value": q.value,
			"unit": q.unit.canonical(),
			"dimension": q.dimension().canonical(),
		});
		Ok(NodeOutput::new().set_data("json", SocketValue::Json(j)))
	}
}

// ---------------------------------------------------------------------------
// helper (tests only)
// ---------------------------------------------------------------------------

#[cfg(test)]
fn prop_str(k: &str, v: &str) -> InputMap {
	[(k.to_string(), SocketValue::String(v.into()))].into_iter().collect()
}

#[cfg(test)]
fn in_float(k: &str, v: f64) -> InputMap {
	[(k.to_string(), SocketValue::Float(v))].into_iter().collect()
}

#[cfg(test)]
fn in_quantity(k: &str, q: Quantity) -> InputMap {
	[(k.to_string(), SocketValue::Quantity(q))].into_iter().collect()
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::quantity::{SIPrefix, Unit};

	// ---- assign ----------------------------------------------------------

	#[tokio::test]
	async fn assign_attaches_unit_to_float() {
		let out = UnitAssignNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&prop_str("unit", "m/s"),
				&in_float("value", 9.5),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let q = match out.data.get("result").unwrap() {
			SocketValue::Quantity(q) => q.clone(),
			_ => panic!("expected Quantity"),
		};
		assert_eq!(q.value, 9.5);
		assert_eq!(q.unit.canonical(), "m\u{b7}s^-1");
	}

	#[tokio::test]
	async fn assign_empty_unit_produces_dimensionless() {
		let out = UnitAssignNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&in_float("value", 3.14),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		match out.data.get("result").unwrap() {
			SocketValue::Quantity(q) => {
				assert_eq!(q.value, 3.14);
				assert!(q.is_dimensionless());
			}
			_ => panic!("expected Quantity"),
		}
	}

	#[tokio::test]
	async fn assign_invalid_unit_errors() {
		let err = UnitAssignNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&prop_str("unit", "not_a_unit"),
				&in_float("value", 1.0),
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(err, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn try_parse_returns_result_for_success_and_error() {
		let inputs: InputMap = [("text".into(), SocketValue::String("2 km".into()))].into_iter().collect();
		let out = UnitTryParseNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("ok"), Some(&SocketValue::Bool(true)));
		match out.data.get("quantity").unwrap() {
			SocketValue::Quantity(q) => {
				assert_eq!(q.value, 2.0);
				assert_eq!(q.unit.canonical(), "1000\u{b7}m");
			}
			_ => panic!("expected Quantity"),
		}
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => assert!(result.ok),
			_ => panic!("expected Result"),
		}

		let inputs: InputMap = [("text".into(), SocketValue::String("not_a_quantity".into()))]
			.into_iter()
			.collect();
		let out = UnitTryParseNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("ok"), Some(&SocketValue::Bool(false)));
		assert!(out.data.get("error").unwrap().as_str().unwrap().contains("quantity parse error"));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("unit.parse"));
			}
			_ => panic!("expected Result"),
		}
	}

	// ---- convert ---------------------------------------------------------

	#[tokio::test]
	async fn convert_km_to_m() {
		let km = Quantity::of(1.0, Unit::metre().with_prefix(SIPrefix::Kilo));
		let out = UnitConvertNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&prop_str("target_unit", "m"),
				&in_quantity("value", km),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		match out.data.get("result").unwrap() {
			SocketValue::Quantity(q) => {
				assert!((q.value - 1000.0).abs() < 1e-9);
				assert_eq!(q.unit, Unit::metre());
			}
			_ => panic!("expected Quantity"),
		}
	}

	#[tokio::test]
	async fn convert_deg_to_rad() {
		let deg = Quantity::of(180.0, Unit::degree());
		let out = UnitConvertNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&prop_str("target_unit", "rad"),
				&in_quantity("value", deg),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		match out.data.get("result").unwrap() {
			SocketValue::Quantity(q) => {
				// 180 deg = \u{3c0} rad (regardless of Unit internals: convert_to normalizes via SI)
				assert!((q.value - std::f64::consts::PI).abs() < 1e-9);
			}
			_ => panic!("expected Quantity"),
		}
	}

	#[tokio::test]
	async fn convert_dimension_mismatch_errors() {
		let metres = Quantity::of(1.0, Unit::metre());
		let err = UnitConvertNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&prop_str("target_unit", "s"),
				&in_quantity("value", metres),
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(err, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn convert_requires_target_unit_property() {
		let q = Quantity::of(1.0, Unit::metre());
		let err = UnitConvertNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&in_quantity("value", q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(err, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn convert_k_to_delta_k_errors() {
		// K and \u{394}K share dimension but have different semantics; convert must reject
		let k = Quantity::of(300.0, Unit::kelvin());
		let err = UnitConvertNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&prop_str("target_unit", "\u{394}K"),
				&in_quantity("value", k),
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(err, NodeExecError::Generic(_)));
	}

	// ---- strip -----------------------------------------------------------

	#[tokio::test]
	async fn strip_discards_unit() {
		let q = Quantity::of(9.8, Unit::metre().div(&Unit::second().pow_i8(2)));
		let out = UnitStripNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&in_quantity("value", q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Float(9.8)));
	}

	// ---- get_unit_string / get_dim_string --------------------------------

	#[tokio::test]
	async fn get_unit_string_returns_canonical_form() {
		let q = Quantity::of(1.0, Unit::metre().with_prefix(SIPrefix::Kilo));
		let out = UnitGetUnitStringNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&in_quantity("value", q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("name"), Some(&SocketValue::String("km".into())));
	}

	#[tokio::test]
	async fn get_dim_string_returns_canonical_dimension() {
		let q = Quantity::of(9.8, Unit::metre().div(&Unit::second().pow_i8(2)));
		let out = UnitGetDimensionStringNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&in_quantity("value", q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("dim"), Some(&SocketValue::String("L\u{b7}T^-2".into())));
	}

	// ---- same_dimension --------------------------------------------------

	#[tokio::test]
	async fn same_dimension_true_for_km_and_m() {
		let mut inputs = InputMap::new();
		inputs.insert(
			"a".into(),
			SocketValue::Quantity(Quantity::of(1.0, Unit::metre().with_prefix(SIPrefix::Kilo))),
		);
		inputs.insert("b".into(), SocketValue::Quantity(Quantity::of(500.0, Unit::metre())));
		let out = UnitSameDimensionNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(true)));
	}

	#[tokio::test]
	async fn same_dimension_false_for_metre_and_second() {
		let mut inputs = InputMap::new();
		inputs.insert("a".into(), SocketValue::Quantity(Quantity::of(1.0, Unit::metre())));
		inputs.insert("b".into(), SocketValue::Quantity(Quantity::of(1.0, Unit::second())));
		let out = UnitSameDimensionNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Bool(false)));
	}

	// ---- to_json ---------------------------------------------------------

	#[tokio::test]
	async fn to_json_emits_internal_form() {
		let q = Quantity::of(60.0, Unit::hertz());
		let out = UnitToJsonNode
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&in_quantity("value", q),
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		match out.data.get("json").unwrap() {
			SocketValue::Json(v) => {
				assert_eq!(v["value"].as_f64().unwrap(), 60.0);
				assert_eq!(v["unit"].as_str().unwrap(), "s^-1");
				assert_eq!(v["dimension"].as_str().unwrap(), "T^-1");
			}
			_ => panic!("expected Json"),
		}
	}
}
