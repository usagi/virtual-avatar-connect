//! Math nodes (all PureNode).
//!
//! `int_*` and `float_*` are split by type. No implicit conversion (spec 2.1).
//! Division by zero halts with `NodeExecError::Generic` (delta-0 safety-first policy).
//!
//! Phase \u{03BE}-3: `float_*` binop nodes moved to `SocketType::Quantity` input/output.
//! Existing `Float` edges are wrapped as dimensionless Quantity by the engine-side
//! [`crate::flowgraph::socket::coerce_to_type`] so backward compatibility is preserved.
//!
//! Phase \u{03BF}-1: 42 additional math nodes added (all Quantity-aware from start).
//! Categories:
//! - Int unary/binary/ternary: abs, sign, min, max, clamp.
//! - Float unary pass-through: abs, sign, floor, ceil, round (unit preserved).
//! - Float unary dimensionless-only: sqrt, exp, ln, log2, log10, sinh/cosh/tanh, asinh/acosh/atanh.
//! - Float same-dim binop: min, max.
//! - Float same-dim ternary: clamp.
//! - Trig (angle \u{2192} dimensionless): sin, cos, tan.
//! - Arctrig (dimensionless \u{2192} angle rad): asin, acos, atan.
//! - atan2 (same-dim pair \u{2192} rad).
//! - pow (dimensionless pair \u{2192} dimensionless).
//! - Interp: lerp, inverse_lerp, remap, smoothstep.
//! - Angle conversion: deg_to_rad, rad_to_deg.
//! - Angle normalization: `[0, 360)`, `[-180, 180)`, `[0, 2\u{03C0})`, `[-\u{03C0}, \u{03C0})`.
//!
//! Angle inputs follow a lenient policy: dimensionless values are accepted as already
//! being in the expected angular unit (pre-\u{03BE} backward compatibility for plain-float flows).
//! Non-angle non-dimensionless values are rejected with a dimension-mismatch error.
//! sinh/cosh/tanh/asinh/acosh/atanh/sqrt/pow/exp/ln/log2/log10 are **strictly dimensionless**.

use crate::flowgraph::node::{
	get_required_int, get_required_quantity, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::quantity::{Dimension, Quantity, Unit};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

// ============================================================================
// Int binary ops (existing)
// ============================================================================

macro_rules! int_binop_node {
	($name:ident, $feature:literal, $title:literal, $fn:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: None,
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Int),
						PortSpec::input("b", "B", SocketType::Int),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Int)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let a = get_required_int(inputs, "a")?;
				let b = get_required_int(inputs, "b")?;
				let op: fn(i64, i64) -> Result<i64, anyhow::Error> = $fn;
				let r = op(a, b)?;
				Ok(NodeOutput::new().set_data("result", SocketValue::Int(r)))
			}
		}
	};
}

int_binop_node!(IntAddNode, "flowgraph.math.int_add", "Int +", |a, b| Ok(a.wrapping_add(b)));
int_binop_node!(IntSubNode, "flowgraph.math.int_sub", "Int -", |a, b| Ok(a.wrapping_sub(b)));
int_binop_node!(IntMulNode, "flowgraph.math.int_mul", "Int *", |a, b| Ok(a.wrapping_mul(b)));
int_binop_node!(IntDivNode, "flowgraph.math.int_div", "Int /", |a, b| {
	if b == 0 {
		Err(anyhow::anyhow!("division by zero: a / b where b = 0"))
	} else {
		Ok(a.wrapping_div(b))
	}
});
int_binop_node!(IntModNode, "flowgraph.math.int_mod", "Int %", |a, b| {
	if b == 0 {
		Err(anyhow::anyhow!("modulo by zero: a % b where b = 0"))
	} else {
		Ok(a.wrapping_rem(b))
	}
});
int_binop_node!(IntMinNode, "flowgraph.math.int_min", "Int min", |a, b| Ok(a.min(b)));
int_binop_node!(IntMaxNode, "flowgraph.math.int_max", "Int max", |a, b| Ok(a.max(b)));

// ============================================================================
// Int unary ops (abs, sign)
// ============================================================================

macro_rules! int_unary_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $fn:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![PortSpec::input("x", "X", SocketType::Int)],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Int)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let x = get_required_int(inputs, "x")?;
				let op: fn(i64) -> i64 = $fn;
				Ok(NodeOutput::new().set_data("result", SocketValue::Int(op(x))))
			}
		}
	};
}

int_unary_node!(
	IntAbsNode,
	"flowgraph.math.abs_int",
	"Int abs",
	"Integer absolute value. i64::MIN overflow is handled via wrapping_abs (returns i64::MIN).",
	|x: i64| x.wrapping_abs()
);
int_unary_node!(
	IntSignNode,
	"flowgraph.math.sign_int",
	"Int sign",
	"Integer sign: -1 for x<0, 0 for x==0, +1 for x>0.",
	|x: i64| x.signum()
);

// ============================================================================
// Int clamp (custom: 3-input same-type)
// ============================================================================

pub struct IntClampNode;

impl NodeDescriptor for IntClampNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.clamp_int".into(),
			title: "Int clamp".into(),
			category: "math".into(),
			description: Some("Clamp value to [lo, hi]. If lo > hi, they are swapped before clamping.".into()),
			inputs: vec![
				PortSpec::input("value", "Value", SocketType::Int),
				PortSpec::input("lo", "Lo", SocketType::Int),
				PortSpec::input("hi", "Hi", SocketType::Int),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Int)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for IntClampNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let v = get_required_int(inputs, "value")?;
		let lo = get_required_int(inputs, "lo")?;
		let hi = get_required_int(inputs, "hi")?;
		let (lo, hi) = if lo > hi { (hi, lo) } else { (lo, hi) };
		Ok(NodeOutput::new().set_data("result", SocketValue::Int(v.clamp(lo, hi))))
	}
}

// ============================================================================
// Float binary ops (Phase xi-3: Quantity-based)
// ============================================================================

/// Shared implementation for `flowgraph.math.float_*` binop nodes. Handles
/// dimension-aware arithmetic via `Quantity` methods.
///
/// The `$op` closure receives two `&Quantity` references and returns a
/// `Result<Quantity, QuantityArithError>`. Dimension mismatches, absolute
/// temperature misuse, and division-by-zero are propagated as
/// `NodeExecError::Generic`.
macro_rules! float_binop_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $op:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Quantity),
						PortSpec::input("b", "B", SocketType::Quantity),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let a = get_required_quantity(inputs, "a")?;
				let b = get_required_quantity(inputs, "b")?;
				let op: fn(
					&crate::flowgraph::quantity::Quantity,
					&crate::flowgraph::quantity::Quantity,
				) -> Result<crate::flowgraph::quantity::Quantity, crate::flowgraph::quantity::QuantityArithError> = $op;
				let r = op(a, b).map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?;
				Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
			}
		}
	};
}

float_binop_node!(
	FloatAddNode,
	"flowgraph.math.float_add",
	"Float +",
	"Quantity addition. Dimension mismatch is an error. \u{0394}K + K(abs) is allowed; K + K is rejected.",
	|a, b| a.try_add(b)
);
float_binop_node!(
	FloatSubNode,
	"flowgraph.math.float_sub",
	"Float -",
	"Quantity subtraction. Dimension mismatch is an error. K - K yields \u{0394}K.",
	|a, b| a.try_sub(b)
);
float_binop_node!(
	FloatMulNode,
	"flowgraph.math.float_mul",
	"Float *",
	"Quantity multiplication. Dimensions are composed (m * s = m\u{00B7}s). Absolute-temperature multiplication is rejected.",
	|a, b| a.try_mul(b)
);
float_binop_node!(
	FloatDivNode,
	"flowgraph.math.float_div",
	"Float /",
	"Quantity division. Dimensions are composed (m / s = m\u{00B7}s\u{207B}\u{00B9}). Absolute-temperature division and division by zero are rejected.",
	|a, b| {
		if b.value == 0.0 {
			return Err(crate::flowgraph::quantity::QuantityArithError::DivisionByZero);
		}
		a.try_div(b)
	}
);

// ============================================================================
// Float unary pass-through (unit preserved, value transformed by f64 -> f64)
// ============================================================================

macro_rules! float_unary_pass_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $fn:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let q = get_required_quantity(inputs, "x")?;
				let op: fn(f64) -> f64 = $fn;
				let r = Quantity::of(op(q.value), q.unit.clone());
				Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
			}
		}
	};
}

float_unary_pass_node!(
	FloatAbsNode,
	"flowgraph.math.abs_float",
	"Float abs",
	"Absolute value. Unit is preserved (abs(-5 m) = 5 m). NaN input yields NaN output.",
	|x: f64| x.abs()
);
float_unary_pass_node!(
	FloatSignNode,
	"flowgraph.math.sign_float",
	"Float sign",
	"Sign classifier: -1 / 0 / +1 (value only, unit preserved). NaN yields 0. Note: the SI meaning of a unit-bearing sign is unusual but kept for pass-through consistency.",
	|x: f64| {
		if x.is_nan() {
			0.0
		} else if x > 0.0 {
			1.0
		} else if x < 0.0 {
			-1.0
		} else {
			0.0
		}
	}
);
float_unary_pass_node!(
	FloatFloorNode,
	"flowgraph.math.floor",
	"Float floor",
	"Largest integer \u{2264} x (f64::floor). Unit is preserved.",
	|x: f64| x.floor()
);
float_unary_pass_node!(
	FloatCeilNode,
	"flowgraph.math.ceil",
	"Float ceil",
	"Smallest integer \u{2265} x (f64::ceil). Unit is preserved.",
	|x: f64| x.ceil()
);
float_unary_pass_node!(
	FloatRoundNode,
	"flowgraph.math.round",
	"Float round",
	"Round half away from zero (f64::round std default). Unit is preserved.",
	|x: f64| x.round()
);

// ============================================================================
// Float unary dimensionless-only (strictly dimensionless in/out)
// ============================================================================

fn require_dimensionless(q: &Quantity, node: &str, port: &str) -> Result<f64, NodeExecError> {
	if q.is_dimensionless() {
		Ok(q.value)
	} else {
		Err(NodeExecError::Generic(anyhow::anyhow!(
			"{} node requires dimensionless input at port '{}' but got unit '{}'; use flowgraph.unit.strip or flowgraph.unit.convert",
			node,
			port,
			q.unit.canonical()
		)))
	}
}

macro_rules! float_unary_dimless_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $fn:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let q = get_required_quantity(inputs, "x")?;
				let v = require_dimensionless(q, $feature, "x")?;
				let op: fn(f64) -> f64 = $fn;
				let r = Quantity::dimensionless(op(v));
				Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
			}
		}
	};
}

float_unary_dimless_node!(
	FloatExpNode,
	"flowgraph.math.exp",
	"Float exp",
	"e^x. Input must be dimensionless.",
	|x: f64| x.exp()
);
float_unary_dimless_node!(
	FloatLnNode,
	"flowgraph.math.ln",
	"Float ln",
	"Natural logarithm. Input must be dimensionless. x \u{2264} 0 yields -inf/NaN per std::f64.",
	|x: f64| x.ln()
);
float_unary_dimless_node!(
	FloatLog2Node,
	"flowgraph.math.log2",
	"Float log2",
	"Base-2 logarithm. Input must be dimensionless.",
	|x: f64| x.log2()
);
float_unary_dimless_node!(
	FloatLog10Node,
	"flowgraph.math.log10",
	"Float log10",
	"Base-10 logarithm. Input must be dimensionless.",
	|x: f64| x.log10()
);
float_unary_dimless_node!(
	FloatSinhNode,
	"flowgraph.math.sinh",
	"Float sinh",
	"Hyperbolic sine. Input must be dimensionless (hyperbolic functions take unitless arguments in their SI-compatible form).",
	|x: f64| x.sinh()
);
float_unary_dimless_node!(
	FloatCoshNode,
	"flowgraph.math.cosh",
	"Float cosh",
	"Hyperbolic cosine. Input must be dimensionless.",
	|x: f64| x.cosh()
);
float_unary_dimless_node!(
	FloatTanhNode,
	"flowgraph.math.tanh",
	"Float tanh",
	"Hyperbolic tangent. Input must be dimensionless.",
	|x: f64| x.tanh()
);
float_unary_dimless_node!(
	FloatAsinhNode,
	"flowgraph.math.asinh",
	"Float asinh",
	"Inverse hyperbolic sine. Input must be dimensionless. Defined for all real x.",
	|x: f64| x.asinh()
);
float_unary_dimless_node!(
	FloatAcoshNode,
	"flowgraph.math.acosh",
	"Float acosh",
	"Inverse hyperbolic cosine. Input must be dimensionless. x < 1 yields NaN (std::f64).",
	|x: f64| x.acosh()
);
float_unary_dimless_node!(
	FloatAtanhNode,
	"flowgraph.math.atanh",
	"Float atanh",
	"Inverse hyperbolic tangent. Input must be dimensionless. |x| \u{2265} 1 yields \u{00B1}inf/NaN (std::f64).",
	|x: f64| x.atanh()
);

// ============================================================================
// Float sqrt (Phase o-1.1: dimension-aware via Quantity::try_sqrt)
// ============================================================================

/// `flowgraph.math.sqrt` — delegates to [`Quantity::try_sqrt`] so that
/// dimension-aware square roots (sqrt(m\u{00B2}) = m, sqrt(m\u{00B2}/s\u{00B2}) = m/s)
/// are supported natively. The underlying `Dimension` is an 8-component `i8`
/// vector, so non-integer resulting exponents are rejected as a type error.
///
/// Accepted: dimensionless, and any Quantity whose atom exponents are all
/// even (e.g. m\u{00B2}, m\u{2074}, m\u{00B2}\u{00B7}s\u{207B}\u{00B2}).
///
/// Rejected: odd-exponent inputs (`sqrt(m)` would require `L^(1/2)` which
/// the integer-dimension type system cannot express; users who really want
/// this must explicitly `flowgraph.unit.strip` first). Absolute temperature
/// (K) is also rejected (handled inside `try_sqrt`).
pub struct FloatSqrtNode;

impl NodeDescriptor for FloatSqrtNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.sqrt".into(),
			title: "Float sqrt".into(),
			category: "math".into(),
			description: Some(
				"Square root. Dimension-aware: sqrt(m\u{00B2}) = m, sqrt(m\u{00B2}/s\u{00B2}) = m/s. All atom exponents must be even (the current type system only represents integer dimensions), so sqrt(m) is rejected \u{2014} use flowgraph.unit.strip first if that was intentional. Absolute temperature (K) is rejected. Negative value yields NaN.".into(),
			),
			inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatSqrtNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "x")?;
		let r = q.try_sqrt().map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
	}
}

// ============================================================================
// Float min/max (same-dimension binop)
// ============================================================================

fn resolve_same_dim_pair<'a>(a: &'a Quantity, b: &'a Quantity, node: &str) -> Result<(f64, f64), NodeExecError> {
	if a.dimension() != b.dimension() {
		return Err(NodeExecError::Generic(anyhow::anyhow!(
			"{} requires same-dimension inputs but got a={} b={}",
			node,
			a.unit.canonical(),
			b.unit.canonical()
		)));
	}
	let b_in_a = b.convert_to(&a.unit).map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?;
	Ok((a.value, b_in_a.value))
}

macro_rules! float_same_dim_binop_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $fn:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Quantity),
						PortSpec::input("b", "B", SocketType::Quantity),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let a = get_required_quantity(inputs, "a")?;
				let b = get_required_quantity(inputs, "b")?;
				let (av, bv) = resolve_same_dim_pair(a, b, $feature)?;
				let op: fn(f64, f64) -> f64 = $fn;
				let r = Quantity::of(op(av, bv), a.unit.clone());
				Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
			}
		}
	};
}

float_same_dim_binop_node!(
	FloatMinNode,
	"flowgraph.math.min_float",
	"Float min",
	"Minimum of two same-dimension Quantity values. Result keeps A's unit. NaN follows f64::min semantics (NaN propagates to other operand).",
	|a: f64, b: f64| a.min(b)
);
float_same_dim_binop_node!(
	FloatMaxNode,
	"flowgraph.math.max_float",
	"Float max",
	"Maximum of two same-dimension Quantity values. Result keeps A's unit. NaN follows f64::max semantics.",
	|a: f64, b: f64| a.max(b)
);

// ============================================================================
// Float clamp (3-input, all same-dim)
// ============================================================================

pub struct FloatClampNode;

impl NodeDescriptor for FloatClampNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.clamp_float".into(),
			title: "Float clamp".into(),
			category: "math".into(),
			description: Some(
				"Clamp value to [lo, hi] on Quantity. All three inputs must share a dimension. If lo > hi after unit-normalizing into value's unit, they are swapped. Result unit follows the value input.".into(),
			),
			inputs: vec![
				PortSpec::input("value", "Value", SocketType::Quantity),
				PortSpec::input("lo", "Lo", SocketType::Quantity),
				PortSpec::input("hi", "Hi", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatClampNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let value = get_required_quantity(inputs, "value")?;
		let lo = get_required_quantity(inputs, "lo")?;
		let hi = get_required_quantity(inputs, "hi")?;
		if value.dimension() != lo.dimension() || value.dimension() != hi.dimension() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"clamp_float: value/lo/hi must share dimension; got value={} lo={} hi={}",
				value.unit.canonical(),
				lo.unit.canonical(),
				hi.unit.canonical()
			)));
		}
		let lo_v = lo
			.convert_to(&value.unit)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
			.value;
		let hi_v = hi
			.convert_to(&value.unit)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
			.value;
		let (lo_v, hi_v) = if lo_v > hi_v { (hi_v, lo_v) } else { (lo_v, hi_v) };
		let clamped = value.value.clamp(lo_v, hi_v);
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(Quantity::of(clamped, value.unit.clone()))))
	}
}

// ============================================================================
// Trig: sin/cos/tan (angle -> dimensionless)
// ============================================================================

fn quantity_to_radians(q: &Quantity, node: &str, port: &str) -> Result<f64, NodeExecError> {
	if q.is_dimensionless() {
		Ok(q.value)
	} else if q.dimension() == Dimension::ANGLE {
		let rad = q
			.convert_to(&Unit::radian())
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?;
		Ok(rad.value)
	} else {
		Err(NodeExecError::Generic(anyhow::anyhow!(
			"{} requires angle (rad/deg) or dimensionless input at port '{}'; got unit '{}'",
			node,
			port,
			q.unit.canonical()
		)))
	}
}

macro_rules! float_trig_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $fn:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let q = get_required_quantity(inputs, "x")?;
				let rad = quantity_to_radians(q, $feature, "x")?;
				let op: fn(f64) -> f64 = $fn;
				let r = Quantity::dimensionless(op(rad));
				Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
			}
		}
	};
}

float_trig_node!(
	FloatSinNode,
	"flowgraph.math.sin",
	"Float sin",
	"Sine. Input is Angle (rad/deg) or dimensionless (treated as radians for pre-\u{03BE} compatibility). Output is dimensionless.",
	|x: f64| x.sin()
);
float_trig_node!(
	FloatCosNode,
	"flowgraph.math.cos",
	"Float cos",
	"Cosine. Input is Angle (rad/deg) or dimensionless (treated as radians). Output is dimensionless.",
	|x: f64| x.cos()
);
float_trig_node!(
	FloatTanNode,
	"flowgraph.math.tan",
	"Float tan",
	"Tangent. Input is Angle (rad/deg) or dimensionless (treated as radians). Output is dimensionless. \u{00B1}(\u{03C0}/2) yields large finite values per std::f64.",
	|x: f64| x.tan()
);

// ============================================================================
// Arctrig: asin/acos/atan (dimensionless -> angle rad)
// ============================================================================

macro_rules! float_arctrig_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $fn:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let q = get_required_quantity(inputs, "x")?;
				let v = require_dimensionless(q, $feature, "x")?;
				let op: fn(f64) -> f64 = $fn;
				let r = Quantity::of(op(v), Unit::radian());
				Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
			}
		}
	};
}

float_arctrig_node!(
	FloatAsinNode,
	"flowgraph.math.asin",
	"Float asin",
	"Inverse sine. Input dimensionless. Output Angle (rad). |x| > 1 yields NaN (std::f64).",
	|x: f64| x.asin()
);
float_arctrig_node!(
	FloatAcosNode,
	"flowgraph.math.acos",
	"Float acos",
	"Inverse cosine. Input dimensionless. Output Angle (rad). |x| > 1 yields NaN.",
	|x: f64| x.acos()
);
float_arctrig_node!(
	FloatAtanNode,
	"flowgraph.math.atan",
	"Float atan",
	"Inverse tangent. Input dimensionless. Output Angle (rad) in (-\u{03C0}/2, \u{03C0}/2).",
	|x: f64| x.atan()
);

// ============================================================================
// atan2 (same-dim pair -> rad)
// ============================================================================

pub struct FloatAtan2Node;

impl NodeDescriptor for FloatAtan2Node {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.atan2".into(),
			title: "Float atan2".into(),
			category: "math".into(),
			description: Some(
				"Two-argument arctangent. y and x must share a dimension (so their ratio is dimensionless). Output is Angle (rad) in (-\u{03C0}, \u{03C0}].".into(),
			),
			inputs: vec![
				PortSpec::input("y", "Y", SocketType::Quantity),
				PortSpec::input("x", "X", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatAtan2Node {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let y = get_required_quantity(inputs, "y")?;
		let x = get_required_quantity(inputs, "x")?;
		let (yv, xv) = resolve_same_dim_pair(y, x, "flowgraph.math.atan2")?;
		let r = Quantity::of(yv.atan2(xv), Unit::radian());
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
	}
}

// ============================================================================
// pow (dimensionless pair -> dimensionless)
// ============================================================================

pub struct FloatPowNode;

impl NodeDescriptor for FloatPowNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.pow".into(),
			title: "Float pow".into(),
			category: "math".into(),
			description: Some(
				"base^exp. Both base and exp must be dimensionless (general Quantity pow requires an integer exponent for dimension algebra; for that use flowgraph.unit.* + custom). NaN and \u{00B1}inf follow f64::powf semantics.".into(),
			),
			inputs: vec![
				PortSpec::input("base", "Base", SocketType::Quantity),
				PortSpec::input("exp", "Exp", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatPowNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let base = get_required_quantity(inputs, "base")?;
		let exp = get_required_quantity(inputs, "exp")?;
		let b = require_dimensionless(base, "flowgraph.math.pow", "base")?;
		let e = require_dimensionless(exp, "flowgraph.math.pow", "exp")?;
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(Quantity::dimensionless(b.powf(e)))))
	}
}

// ============================================================================
// lerp, inverse_lerp, remap, smoothstep
// ============================================================================

pub struct FloatLerpNode;

impl NodeDescriptor for FloatLerpNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.lerp".into(),
			title: "Float lerp".into(),
			category: "math".into(),
			description: Some(
				"Linear interpolation: a + (b - a) * t. a and b share a dimension; t is dimensionless. t is not clamped (extrapolation allowed). Result unit follows a.".into(),
			),
			inputs: vec![
				PortSpec::input("a", "A", SocketType::Quantity),
				PortSpec::input("b", "B", SocketType::Quantity),
				PortSpec::input("t", "T", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatLerpNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let a = get_required_quantity(inputs, "a")?;
		let b = get_required_quantity(inputs, "b")?;
		let t = get_required_quantity(inputs, "t")?;
		let t_v = require_dimensionless(t, "flowgraph.math.lerp", "t")?;
		let (av, bv) = resolve_same_dim_pair(a, b, "flowgraph.math.lerp")?;
		let r = Quantity::of(av + (bv - av) * t_v, a.unit.clone());
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(r)))
	}
}

pub struct FloatInverseLerpNode;

impl NodeDescriptor for FloatInverseLerpNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.inverse_lerp".into(),
			title: "Float inverse_lerp".into(),
			category: "math".into(),
			description: Some(
				"Inverse of lerp: (v - a) / (b - a). All three inputs share a dimension. If a == b (after unit normalization), returns 0.0. Result is dimensionless.".into(),
			),
			inputs: vec![
				PortSpec::input("a", "A", SocketType::Quantity),
				PortSpec::input("b", "B", SocketType::Quantity),
				PortSpec::input("v", "V", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatInverseLerpNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let a = get_required_quantity(inputs, "a")?;
		let b = get_required_quantity(inputs, "b")?;
		let v = get_required_quantity(inputs, "v")?;
		if a.dimension() != b.dimension() || a.dimension() != v.dimension() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"inverse_lerp: a/b/v must share dimension; got a={} b={} v={}",
				a.unit.canonical(),
				b.unit.canonical(),
				v.unit.canonical()
			)));
		}
		let b_v = b.convert_to(&a.unit).map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?.value;
		let v_v = v.convert_to(&a.unit).map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?.value;
		let result = if b_v == a.value { 0.0 } else { (v_v - a.value) / (b_v - a.value) };
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(Quantity::dimensionless(result))))
	}
}

pub struct FloatRemapNode;

impl NodeDescriptor for FloatRemapNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.remap".into(),
			title: "Float remap".into(),
			category: "math".into(),
			description: Some(
				"Remap value from [in_lo, in_hi] to [out_lo, out_hi]. value/in_lo/in_hi share a dimension; out_lo/out_hi share another dimension. If in_lo == in_hi, out_lo is returned. Result unit follows out_lo.".into(),
			),
			inputs: vec![
				PortSpec::input("value", "Value", SocketType::Quantity),
				PortSpec::input("in_lo", "In Lo", SocketType::Quantity),
				PortSpec::input("in_hi", "In Hi", SocketType::Quantity),
				PortSpec::input("out_lo", "Out Lo", SocketType::Quantity),
				PortSpec::input("out_hi", "Out Hi", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatRemapNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let value = get_required_quantity(inputs, "value")?;
		let in_lo = get_required_quantity(inputs, "in_lo")?;
		let in_hi = get_required_quantity(inputs, "in_hi")?;
		let out_lo = get_required_quantity(inputs, "out_lo")?;
		let out_hi = get_required_quantity(inputs, "out_hi")?;
		if value.dimension() != in_lo.dimension() || value.dimension() != in_hi.dimension() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"remap: value/in_lo/in_hi must share dimension; got value={} in_lo={} in_hi={}",
				value.unit.canonical(),
				in_lo.unit.canonical(),
				in_hi.unit.canonical()
			)));
		}
		if out_lo.dimension() != out_hi.dimension() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"remap: out_lo/out_hi must share dimension; got out_lo={} out_hi={}",
				out_lo.unit.canonical(),
				out_hi.unit.canonical()
			)));
		}
		let in_lo_v = in_lo
			.convert_to(&value.unit)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
			.value;
		let in_hi_v = in_hi
			.convert_to(&value.unit)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
			.value;
		let out_hi_v = out_hi
			.convert_to(&out_lo.unit)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
			.value;
		let result_value = if in_lo_v == in_hi_v {
			out_lo.value
		} else {
			let t = (value.value - in_lo_v) / (in_hi_v - in_lo_v);
			out_lo.value + t * (out_hi_v - out_lo.value)
		};
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(Quantity::of(result_value, out_lo.unit.clone()))))
	}
}

pub struct FloatSmoothstepNode;

impl NodeDescriptor for FloatSmoothstepNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.smoothstep".into(),
			title: "Float smoothstep".into(),
			category: "math".into(),
			description: Some(
				"GLSL smoothstep: t = clamp((x - edge0) / (edge1 - edge0), 0, 1); returns t*t*(3 - 2*t). All three inputs share a dimension. If edge0 == edge1, returns 0.0. Result is dimensionless in [0, 1].".into(),
			),
			inputs: vec![
				PortSpec::input("edge0", "Edge0", SocketType::Quantity),
				PortSpec::input("edge1", "Edge1", SocketType::Quantity),
				PortSpec::input("x", "X", SocketType::Quantity),
			],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatSmoothstepNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let edge0 = get_required_quantity(inputs, "edge0")?;
		let edge1 = get_required_quantity(inputs, "edge1")?;
		let x = get_required_quantity(inputs, "x")?;
		if edge0.dimension() != edge1.dimension() || edge0.dimension() != x.dimension() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"smoothstep: edge0/edge1/x must share dimension; got edge0={} edge1={} x={}",
				edge0.unit.canonical(),
				edge1.unit.canonical(),
				x.unit.canonical()
			)));
		}
		let e1_v = edge1
			.convert_to(&edge0.unit)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
			.value;
		let x_v = x
			.convert_to(&edge0.unit)
			.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
			.value;
		let result = if e1_v == edge0.value {
			0.0
		} else {
			let t = ((x_v - edge0.value) / (e1_v - edge0.value)).clamp(0.0, 1.0);
			t * t * (3.0 - 2.0 * t)
		};
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(Quantity::dimensionless(result))))
	}
}

// ============================================================================
// deg_to_rad / rad_to_deg (Angle dim or dimensionless)
// ============================================================================

pub struct FloatDegToRadNode;

impl NodeDescriptor for FloatDegToRadNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.deg_to_rad".into(),
			title: "Float deg \u{2192} rad".into(),
			category: "math".into(),
			description: Some(
				"Convert degrees to radians. Angle-dimensioned input is converted via flowgraph.unit.convert semantics. Dimensionless input is scaled by \u{03C0}/180 and tagged with rad unit. Other dimensions are rejected.".into(),
			),
			inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatDegToRadNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "x")?;
		let out = if q.is_dimensionless() {
			Quantity::of(q.value * std::f64::consts::PI / 180.0, Unit::radian())
		} else if q.dimension() == Dimension::ANGLE {
			q.convert_to(&Unit::radian())
				.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
		} else {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"deg_to_rad requires Angle or dimensionless input; got unit '{}'",
				q.unit.canonical()
			)));
		};
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(out)))
	}
}

pub struct FloatRadToDegNode;

impl NodeDescriptor for FloatRadToDegNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.math.rad_to_deg".into(),
			title: "Float rad \u{2192} deg".into(),
			category: "math".into(),
			description: Some(
				"Convert radians to degrees. Angle-dimensioned input is converted via flowgraph.unit.convert semantics. Dimensionless input is scaled by 180/\u{03C0} and tagged with deg unit. Other dimensions are rejected.".into(),
			),
			inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
			outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for FloatRadToDegNode {
	async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
		let q = get_required_quantity(inputs, "x")?;
		let out = if q.is_dimensionless() {
			Quantity::of(q.value * 180.0 / std::f64::consts::PI, Unit::degree())
		} else if q.dimension() == Dimension::ANGLE {
			q.convert_to(&Unit::degree())
				.map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?
		} else {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"rad_to_deg requires Angle or dimensionless input; got unit '{}'",
				q.unit.canonical()
			)));
		};
		Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(out)))
	}
}

// ============================================================================
// normalize_angle_* (4 variants)
// ============================================================================

/// Extract the angular value in the requested target unit (rad or deg).
/// Dimensionless inputs are treated as already being in the target unit.
/// Returns `(value_in_target, was_dimensionless)`.
fn angle_to_unit(q: &Quantity, target: Unit, node: &str) -> Result<(f64, bool), NodeExecError> {
	if q.is_dimensionless() {
		Ok((q.value, true))
	} else if q.dimension() == Dimension::ANGLE {
		let converted = q.convert_to(&target).map_err(|e| NodeExecError::Generic(anyhow::anyhow!(e)))?;
		Ok((converted.value, false))
	} else {
		Err(NodeExecError::Generic(anyhow::anyhow!(
			"{} requires Angle or dimensionless input; got unit '{}'",
			node,
			q.unit.canonical()
		)))
	}
}

macro_rules! normalize_angle_node {
	($name:ident, $feature:literal, $title:literal, $desc:literal, $unit_fn:expr, $normalize:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "math".into(),
					description: Some($desc.into()),
					inputs: vec![PortSpec::input("x", "X", SocketType::Quantity)],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Quantity)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _p: &InputMap, inputs: &InputMap, _fired: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let q = get_required_quantity(inputs, "x")?;
				let make_unit: fn() -> Unit = $unit_fn;
				let (v, was_dimless) = angle_to_unit(q, make_unit(), $feature)?;
				let normalize: fn(f64) -> f64 = $normalize;
				let v = normalize(v);
				let out = if was_dimless {
					Quantity::dimensionless(v)
				} else {
					Quantity::of(v, make_unit())
				};
				Ok(NodeOutput::new().set_data("result", SocketValue::Quantity(out)))
			}
		}
	};
}

normalize_angle_node!(
	FloatNormalizeAngleDeg0To360Node,
	"flowgraph.math.normalize_angle_deg_0_360",
	"Normalize angle [0, 360) deg",
	"Normalize Angle into [0, 360) degrees. 1357.33 \u{2192} 277.33. Angle-dim input is converted to deg first; dimensionless is treated as deg.",
	|| Unit::degree(),
	|x: f64| x.rem_euclid(360.0)
);
normalize_angle_node!(
	FloatNormalizeAngleDegSignedNode,
	"flowgraph.math.normalize_angle_deg_signed",
	"Normalize angle [-180, 180) deg",
	"Normalize Angle into [-180, +180) degrees. 277.33 \u{2192} -82.67. Angle-dim input is converted to deg first; dimensionless is treated as deg.",
	|| Unit::degree(),
	|x: f64| (x + 180.0).rem_euclid(360.0) - 180.0
);
normalize_angle_node!(
	FloatNormalizeAngleRad0To2piNode,
	"flowgraph.math.normalize_angle_rad_0_2pi",
	"Normalize angle [0, 2\u{03C0}) rad",
	"Normalize Angle into [0, 2\u{03C0}) radians. Angle-dim input is converted to rad first; dimensionless is treated as rad.",
	|| Unit::radian(),
	|x: f64| x.rem_euclid(2.0 * std::f64::consts::PI)
);
normalize_angle_node!(
	FloatNormalizeAngleRadSignedNode,
	"flowgraph.math.normalize_angle_rad_signed",
	"Normalize angle [-\u{03C0}, \u{03C0}) rad",
	"Normalize Angle into [-\u{03C0}, +\u{03C0}) radians. Angle-dim input is converted to rad first; dimensionless is treated as rad.",
	|| Unit::radian(),
	|x: f64| (x + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI) - std::f64::consts::PI
);

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
	use super::*;
	use crate::flowgraph::quantity::parse_unit;

	fn int_in(a: i64, b: i64) -> InputMap {
		[("a".into(), SocketValue::Int(a)), ("b".into(), SocketValue::Int(b))]
			.into_iter()
			.collect()
	}

	fn q_in(key: &str, q: Quantity) -> (String, SocketValue) {
		(key.into(), SocketValue::Quantity(q))
	}

	fn unwrap_q(out: &NodeOutput) -> &Quantity {
		match out.data.get("result") {
			Some(SocketValue::Quantity(q)) => q,
			other => panic!("expected Quantity result, got {other:?}"),
		}
	}

	#[tokio::test]
	async fn int_arithmetic_existing() {
		let out = IntAddNode
			.compute(&InputMap::new(), &int_in(3, 4), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(7)));
		let out = IntSubNode
			.compute(&InputMap::new(), &int_in(10, 3), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(7)));
		let out = IntMulNode
			.compute(&InputMap::new(), &int_in(6, 7), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(42)));
		let out = IntDivNode
			.compute(&InputMap::new(), &int_in(20, 4), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(5)));
		let out = IntModNode
			.compute(&InputMap::new(), &int_in(17, 5), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(2)));
	}

	#[tokio::test]
	async fn int_div_by_zero_errors() {
		let e = IntDivNode
			.compute(&InputMap::new(), &int_in(1, 0), &ExecFireSet::new())
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn int_min_max() {
		let out = IntMinNode
			.compute(&InputMap::new(), &int_in(3, 7), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(3)));
		let out = IntMaxNode
			.compute(&InputMap::new(), &int_in(3, 7), &ExecFireSet::new())
			.await
			.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(7)));
	}

	#[tokio::test]
	async fn int_abs_and_sign() {
		let inputs: InputMap = [("x".into(), SocketValue::Int(-5))].into_iter().collect();
		let out = IntAbsNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(5)));
		let out = IntSignNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(-1)));
	}

	#[tokio::test]
	async fn int_clamp_swap() {
		let inputs: InputMap = [
			("value".into(), SocketValue::Int(15)),
			("lo".into(), SocketValue::Int(20)),
			("hi".into(), SocketValue::Int(10)),
		]
		.into_iter()
		.collect();
		let out = IntClampNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(out.data.get("result"), Some(&SocketValue::Int(15)));
	}

	#[tokio::test]
	async fn float_add_dimensionless() {
		let inputs: InputMap = [q_in("a", Quantity::dimensionless(1.5)), q_in("b", Quantity::dimensionless(2.5))]
			.into_iter()
			.collect();
		let out = FloatAddNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 4.0).abs() < 1e-12);
		assert!(unwrap_q(&out).is_dimensionless());
	}

	#[tokio::test]
	async fn float_add_dim_mismatch_errors() {
		let m = Quantity::of(1.0, parse_unit("m").unwrap());
		let s = Quantity::of(2.0, parse_unit("s").unwrap());
		let inputs: InputMap = [q_in("a", m), q_in("b", s)].into_iter().collect();
		let e = FloatAddNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn float_mul_composes_units() {
		let m = Quantity::of(3.0, parse_unit("m").unwrap());
		let s = Quantity::of(4.0, parse_unit("s").unwrap());
		let inputs: InputMap = [q_in("a", m), q_in("b", s)].into_iter().collect();
		let out = FloatMulNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 12.0).abs() < 1e-12);
		assert_eq!(unwrap_q(&out).unit.canonical(), "m\u{00B7}s");
	}

	#[tokio::test]
	async fn float_div_by_zero_errors() {
		let inputs: InputMap = [q_in("a", Quantity::dimensionless(1.0)), q_in("b", Quantity::dimensionless(0.0))]
			.into_iter()
			.collect();
		let e = FloatDivNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn float_abs_preserves_unit() {
		let m = Quantity::of(-5.0, parse_unit("m").unwrap());
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(m))].into_iter().collect();
		let out = FloatAbsNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert_eq!(unwrap_q(&out).value, 5.0);
		assert_eq!(unwrap_q(&out).unit.canonical(), "m");
	}

	#[tokio::test]
	async fn float_min_max_converts_units() {
		// 3 m vs 200 cm = 2 m; min should be 2 m, result in m (a's unit).
		let a = Quantity::of(3.0, parse_unit("m").unwrap());
		let b = Quantity::of(200.0, parse_unit("cm").unwrap());
		let inputs: InputMap = [q_in("a", a.clone()), q_in("b", b.clone())].into_iter().collect();
		let out = FloatMinNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 2.0).abs() < 1e-9);
		assert_eq!(unwrap_q(&out).unit.canonical(), "m");
		let out = FloatMaxNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 3.0).abs() < 1e-9);
	}

	#[tokio::test]
	async fn float_min_dim_mismatch_errors() {
		let m = Quantity::of(1.0, parse_unit("m").unwrap());
		let s = Quantity::of(1.0, parse_unit("s").unwrap());
		let inputs: InputMap = [q_in("a", m), q_in("b", s)].into_iter().collect();
		let e = FloatMinNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn float_clamp_swap_and_unit() {
		// value=150 cm, lo=2 m, hi=1 m (swapped); result clamps to [1 m, 2 m] -> 1.5 m (= 150 cm)
		let inputs: InputMap = [
			q_in("value", Quantity::of(150.0, parse_unit("cm").unwrap())),
			q_in("lo", Quantity::of(2.0, parse_unit("m").unwrap())),
			q_in("hi", Quantity::of(1.0, parse_unit("m").unwrap())),
		]
		.into_iter()
		.collect();
		let out = FloatClampNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		// value is kept in its source unit (cm internally = 0.01*m); numeric value is 150 (cm).
		assert!((unwrap_q(&out).value - 150.0).abs() < 1e-9);
		assert_eq!(unwrap_q(&out).dimension(), Dimension::LENGTH);
	}

	#[tokio::test]
	async fn float_sqrt_dimensionless() {
		let d = Quantity::dimensionless(9.0);
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(d))].into_iter().collect();
		let out = FloatSqrtNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 3.0).abs() < 1e-12);
		assert!(unwrap_q(&out).is_dimensionless());
	}

	#[tokio::test]
	async fn float_sqrt_m_squared_yields_length() {
		let q = Quantity::of(9.0, parse_unit("m^2").unwrap());
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(q))].into_iter().collect();
		let out = FloatSqrtNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 3.0).abs() < 1e-12);
		assert_eq!(unwrap_q(&out).dimension(), Dimension::LENGTH);
	}

	#[tokio::test]
	async fn float_sqrt_velocity_squared_yields_velocity() {
		// sqrt(m^2/s^2) = m/s
		let q = Quantity::of(25.0, parse_unit("m^2/s^2").unwrap());
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(q))].into_iter().collect();
		let out = FloatSqrtNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 5.0).abs() < 1e-12);
		assert_eq!(unwrap_q(&out).dimension(), Dimension::VELOCITY);
	}

	#[tokio::test]
	async fn float_sqrt_odd_exponent_errors() {
		// sqrt(m) would require fractional dimension L^(1/2), not representable in i8 Dimension.
		let q = Quantity::of(4.0, parse_unit("m").unwrap());
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(q))].into_iter().collect();
		let e = FloatSqrtNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn float_sinh_cosh_tanh_dimensionless() {
		let d = Quantity::dimensionless(1.0);
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(d))].into_iter().collect();
		let out = FloatSinhNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 1.0f64.sinh()).abs() < 1e-12);
		let out = FloatCoshNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 1.0f64.cosh()).abs() < 1e-12);
		let out = FloatTanhNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 1.0f64.tanh()).abs() < 1e-12);
	}

	#[tokio::test]
	async fn float_trig_accepts_deg() {
		let ninety_deg = Quantity::of(90.0, parse_unit("deg").unwrap());
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(ninety_deg))].into_iter().collect();
		let out = FloatSinNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 1.0).abs() < 1e-12);
		assert!(unwrap_q(&out).is_dimensionless());
	}

	#[tokio::test]
	async fn float_trig_rejects_non_angle() {
		let m = Quantity::of(1.0, parse_unit("m").unwrap());
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(m))].into_iter().collect();
		let e = FloatSinNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap_err();
		assert!(matches!(e, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn float_arctrig_returns_rad() {
		let d = Quantity::dimensionless(1.0);
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(d))].into_iter().collect();
		let out = FloatAtanNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
		assert_eq!(unwrap_q(&out).unit.canonical(), "rad");
	}

	#[tokio::test]
	async fn float_atan2_same_dim() {
		let y = Quantity::of(1.0, parse_unit("m").unwrap());
		let x = Quantity::of(1.0, parse_unit("m").unwrap());
		let inputs: InputMap = [q_in("y", y), q_in("x", x)].into_iter().collect();
		let out = FloatAtan2Node
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - std::f64::consts::FRAC_PI_4).abs() < 1e-12);
		assert_eq!(unwrap_q(&out).unit.canonical(), "rad");
	}

	#[tokio::test]
	async fn float_pow_dimensionless() {
		let inputs: InputMap = [
			q_in("base", Quantity::dimensionless(2.0)),
			q_in("exp", Quantity::dimensionless(10.0)),
		]
		.into_iter()
		.collect();
		let out = FloatPowNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		assert!((unwrap_q(&out).value - 1024.0).abs() < 1e-9);
	}

	#[tokio::test]
	async fn float_lerp_t_dimensionless_ab_same_dim() {
		let inputs: InputMap = [
			q_in("a", Quantity::of(0.0, parse_unit("m").unwrap())),
			q_in("b", Quantity::of(100.0, parse_unit("cm").unwrap())),
			q_in("t", Quantity::dimensionless(0.5)),
		]
		.into_iter()
		.collect();
		let out = FloatLerpNode.compute(&InputMap::new(), &inputs, &ExecFireSet::new()).await.unwrap();
		// 0m + (1m - 0m) * 0.5 = 0.5 m
		assert!((unwrap_q(&out).value - 0.5).abs() < 1e-9);
		assert_eq!(unwrap_q(&out).unit.canonical(), "m");
	}

	#[tokio::test]
	async fn float_inverse_lerp_returns_dimensionless() {
		let inputs: InputMap = [
			q_in("a", Quantity::of(0.0, parse_unit("m").unwrap())),
			q_in("b", Quantity::of(2.0, parse_unit("m").unwrap())),
			q_in("v", Quantity::of(1.0, parse_unit("m").unwrap())),
		]
		.into_iter()
		.collect();
		let out = FloatInverseLerpNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - 0.5).abs() < 1e-9);
		assert!(unwrap_q(&out).is_dimensionless());
	}

	#[tokio::test]
	async fn float_remap_cross_dim() {
		// value=5 m (of 0..10 m) -> 50 (of 0..100 dimensionless)
		let inputs: InputMap = [
			q_in("value", Quantity::of(5.0, parse_unit("m").unwrap())),
			q_in("in_lo", Quantity::of(0.0, parse_unit("m").unwrap())),
			q_in("in_hi", Quantity::of(10.0, parse_unit("m").unwrap())),
			q_in("out_lo", Quantity::dimensionless(0.0)),
			q_in("out_hi", Quantity::dimensionless(100.0)),
		]
		.into_iter()
		.collect();
		let out = FloatRemapNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - 50.0).abs() < 1e-9);
		assert!(unwrap_q(&out).is_dimensionless());
	}

	#[tokio::test]
	async fn float_smoothstep_returns_dimensionless_01() {
		let inputs: InputMap = [
			q_in("edge0", Quantity::dimensionless(0.0)),
			q_in("edge1", Quantity::dimensionless(1.0)),
			q_in("x", Quantity::dimensionless(0.5)),
		]
		.into_iter()
		.collect();
		let out = FloatSmoothstepNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - 0.5).abs() < 1e-9);
	}

	#[tokio::test]
	async fn float_deg_to_rad_scales_dimensionless() {
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(Quantity::dimensionless(180.0)))]
			.into_iter()
			.collect();
		let out = FloatDegToRadNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - std::f64::consts::PI).abs() < 1e-12);
		assert_eq!(unwrap_q(&out).unit.canonical(), "rad");
	}

	#[tokio::test]
	async fn float_deg_to_rad_converts_angle_unit() {
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(Quantity::of(180.0, parse_unit("deg").unwrap())))]
			.into_iter()
			.collect();
		let out = FloatDegToRadNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - std::f64::consts::PI).abs() < 1e-12);
	}

	#[tokio::test]
	async fn normalize_angle_deg_0_360() {
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(Quantity::of(1357.33, parse_unit("deg").unwrap())))]
			.into_iter()
			.collect();
		let out = FloatNormalizeAngleDeg0To360Node
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - 277.33).abs() < 1e-9);
		assert_eq!(unwrap_q(&out).unit.canonical(), "deg");
	}

	#[tokio::test]
	async fn normalize_angle_deg_signed() {
		let inputs: InputMap = [("x".into(), SocketValue::Quantity(Quantity::of(277.33, parse_unit("deg").unwrap())))]
			.into_iter()
			.collect();
		let out = FloatNormalizeAngleDegSignedNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - (-82.67)).abs() < 1e-9);
	}

	#[tokio::test]
	async fn normalize_angle_rad_signed() {
		// 3pi/2 -> -pi/2
		let inputs: InputMap = [(
			"x".into(),
			SocketValue::Quantity(Quantity::of(3.0 * std::f64::consts::FRAC_PI_2, parse_unit("rad").unwrap())),
		)]
		.into_iter()
		.collect();
		let out = FloatNormalizeAngleRadSignedNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - (-std::f64::consts::FRAC_PI_2)).abs() < 1e-12);
		assert_eq!(unwrap_q(&out).unit.canonical(), "rad");
	}

	#[tokio::test]
	async fn normalize_angle_rad_0_2pi_accepts_dimensionless() {
		let inputs: InputMap = [(
			"x".into(),
			SocketValue::Quantity(Quantity::dimensionless(3.0 * std::f64::consts::PI)),
		)]
		.into_iter()
		.collect();
		let out = FloatNormalizeAngleRad0To2piNode
			.compute(&InputMap::new(), &inputs, &ExecFireSet::new())
			.await
			.unwrap();
		assert!((unwrap_q(&out).value - std::f64::consts::PI).abs() < 1e-12);
		assert!(unwrap_q(&out).is_dimensionless());
	}
}
