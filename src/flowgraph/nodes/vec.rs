//! Vec2 / Vec3 nodes (Phase \u{03BF}-3).
//!
//! All PureNode. Vectors are wired as `SocketType::Json` carrying JSON arrays
//! `[x, y]` / `[x, y, z]` of finite numbers. We deliberately avoid introducing a
//! dedicated `SocketType::Vec2 / Vec3` variant because it would cascade into
//! serialization, coercion, and GUI port chip rendering for narrow gain
//! (see phase doc \u{00A7}3.3 rationale).
//!
//! Error policy: non-array / wrong length / non-finite components halt with
//! `NodeExecError::Generic` (safety-first, matches existing `json_ops`).
//! `normalize` on the zero vector is **not** an error; it returns the zero
//! vector back (documented behavior, phase doc \u{00A7}3.3).
//!
//! Scalars (`scale.k`, `lerp.t`, `dot/length/distance` outputs) use plain
//! `SocketType::Float` rather than `Quantity`. The JSON wire format carries no
//! unit metadata in \u{03BE}, so there is nothing to preserve through the vector ops;
//! downstream Quantity needs can be reattached via `flowgraph.unit.assign`.

use crate::flowgraph::node::{
	get_required_float, get_required_json, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::Value as JsonValue;

// ===========================================================================
// Decode / encode helpers
// ===========================================================================

fn decode_vec<const N: usize>(v: &JsonValue, port: &str) -> Result<[f64; N], NodeExecError> {
	let arr = v
		.as_array()
		.ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("port `{port}`: expected JSON array of {N} numbers, got: {v}")))?;
	if arr.len() != N {
		return Err(NodeExecError::Generic(anyhow::anyhow!(
			"port `{port}`: expected array of length {N}, got length {}",
			arr.len()
		)));
	}
	let mut out = [0.0_f64; N];
	for (i, x) in arr.iter().enumerate() {
		let n = x
			.as_f64()
			.ok_or_else(|| NodeExecError::Generic(anyhow::anyhow!("port `{port}`[{i}]: expected finite number, got: {x}")))?;
		if !n.is_finite() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"port `{port}`[{i}]: non-finite component ({n}) disallowed"
			)));
		}
		out[i] = n;
	}
	Ok(out)
}

fn encode_vec<const N: usize>(arr: [f64; N]) -> Result<JsonValue, NodeExecError> {
	let mut items = Vec::with_capacity(N);
	for (i, x) in arr.iter().enumerate() {
		let n = serde_json::Number::from_f64(*x).ok_or_else(|| {
			NodeExecError::Generic(anyhow::anyhow!(
				"vec encode: component [{i}] = {x} is not representable as finite JSON number"
			))
		})?;
		items.push(JsonValue::Number(n));
	}
	Ok(JsonValue::Array(items))
}

// ===========================================================================
// Pure vector math (const-N generic; shared between vec2 and vec3)
// ===========================================================================

fn add_n<const N: usize>(a: [f64; N], b: [f64; N]) -> [f64; N] {
	let mut r = [0.0; N];
	for i in 0..N {
		r[i] = a[i] + b[i];
	}
	r
}

fn sub_n<const N: usize>(a: [f64; N], b: [f64; N]) -> [f64; N] {
	let mut r = [0.0; N];
	for i in 0..N {
		r[i] = a[i] - b[i];
	}
	r
}

fn scale_n<const N: usize>(a: [f64; N], k: f64) -> [f64; N] {
	let mut r = [0.0; N];
	for i in 0..N {
		r[i] = a[i] * k;
	}
	r
}

fn dot_n<const N: usize>(a: [f64; N], b: [f64; N]) -> f64 {
	let mut s = 0.0;
	for i in 0..N {
		s += a[i] * b[i];
	}
	s
}

fn length_n<const N: usize>(a: [f64; N]) -> f64 {
	dot_n(a, a).sqrt()
}

fn normalize_n<const N: usize>(a: [f64; N]) -> [f64; N] {
	let len = length_n(a);
	if len == 0.0 || !len.is_finite() {
		[0.0; N]
	} else {
		scale_n(a, 1.0 / len)
	}
}

fn lerp_n<const N: usize>(a: [f64; N], b: [f64; N], t: f64) -> [f64; N] {
	let mut r = [0.0; N];
	for i in 0..N {
		r[i] = a[i] + (b[i] - a[i]) * t;
	}
	r
}

fn distance_n<const N: usize>(a: [f64; N], b: [f64; N]) -> f64 {
	length_n(sub_n(a, b))
}

// ===========================================================================
// Node-generating macros
// ===========================================================================

macro_rules! vec_make_node {
	($name:ident, $feature:literal, $title:literal, $n:expr, [$($comp:literal),+]) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "vec".into(),
					description: Some(concat!("Pack ", stringify!($n), " floats into a JSON array [", $($comp, ", ",)+ "]").replace(", ]", "]")),
					inputs: vec![ $( PortSpec::input($comp, $comp, SocketType::Float), )+ ],
					outputs: vec![PortSpec::output("v", "V", SocketType::Json)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _f: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let arr: [f64; $n] = [ $( get_required_float(inputs, $comp)?, )+ ];
				let v = encode_vec::<$n>(arr)?;
				Ok(NodeOutput::new().set_data("v", SocketValue::Json(v)))
			}
		}
	};
}

macro_rules! vec_unpack_node {
	($name:ident, $feature:literal, $title:literal, $n:expr, [$($comp:literal),+]) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "vec".into(),
					description: Some(concat!("Unpack a ", stringify!($n), "-component JSON array into separate Float outputs").into()),
					inputs: vec![PortSpec::input("v", "V", SocketType::Json)],
					outputs: vec![ $( PortSpec::output($comp, $comp, SocketType::Float), )+ ],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(&self, _host: &crate::flowgraph::node::PureEvalHost, _p: &InputMap, inputs: &InputMap, _f: &ExecFireSet) -> Result<NodeOutput, NodeExecError> {
				let v = get_required_json(inputs, "v")?;
				let arr = decode_vec::<$n>(v, "v")?;
				let mut out = NodeOutput::new();
				let mut idx = 0;
				$(
					out = out.set_data($comp, SocketValue::Float(arr[idx]));
					idx += 1;
				)+
				let _ = idx;
				Ok(out)
			}
		}
	};
}

macro_rules! vec_binop_node {
	($name:ident, $feature:literal, $title:literal, $n:expr, $op:ident, $desc:literal) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "vec".into(),
					description: Some($desc.into()),
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Json),
						PortSpec::input("b", "B", SocketType::Json),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Json)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(
				&self,
				_host: &crate::flowgraph::node::PureEvalHost,
				_p: &InputMap,
				inputs: &InputMap,
				_f: &ExecFireSet,
			) -> Result<NodeOutput, NodeExecError> {
				let a = decode_vec::<$n>(get_required_json(inputs, "a")?, "a")?;
				let b = decode_vec::<$n>(get_required_json(inputs, "b")?, "b")?;
				let r = $op::<$n>(a, b);
				Ok(NodeOutput::new().set_data("result", SocketValue::Json(encode_vec::<$n>(r)?)))
			}
		}
	};
}

macro_rules! vec_scalar_out_binop_node {
	($name:ident, $feature:literal, $title:literal, $n:expr, $op:ident, $desc:literal) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "vec".into(),
					description: Some($desc.into()),
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Json),
						PortSpec::input("b", "B", SocketType::Json),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Float)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(
				&self,
				_host: &crate::flowgraph::node::PureEvalHost,
				_p: &InputMap,
				inputs: &InputMap,
				_f: &ExecFireSet,
			) -> Result<NodeOutput, NodeExecError> {
				let a = decode_vec::<$n>(get_required_json(inputs, "a")?, "a")?;
				let b = decode_vec::<$n>(get_required_json(inputs, "b")?, "b")?;
				Ok(NodeOutput::new().set_data("result", SocketValue::Float($op::<$n>(a, b))))
			}
		}
	};
}

macro_rules! vec_scale_node {
	($name:ident, $feature:literal, $title:literal, $n:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "vec".into(),
					description: Some(concat!("Multiply every component of a ", stringify!($n), "-vector by scalar `k`").into()),
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Json),
						PortSpec::input("k", "K", SocketType::Float),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Json)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(
				&self,
				_host: &crate::flowgraph::node::PureEvalHost,
				_p: &InputMap,
				inputs: &InputMap,
				_f: &ExecFireSet,
			) -> Result<NodeOutput, NodeExecError> {
				let a = decode_vec::<$n>(get_required_json(inputs, "a")?, "a")?;
				let k = get_required_float(inputs, "k")?;
				Ok(NodeOutput::new().set_data("result", SocketValue::Json(encode_vec::<$n>(scale_n::<$n>(a, k))?)))
			}
		}
	};
}

macro_rules! vec_unary_pure_node {
	($name:ident, $feature:literal, $title:literal, $n:expr, $op:ident, $desc:literal, $out_ty:expr, $wrap:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "vec".into(),
					description: Some($desc.into()),
					inputs: vec![PortSpec::input("v", "V", SocketType::Json)],
					outputs: vec![PortSpec::output("result", "Result", $out_ty)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(
				&self,
				_host: &crate::flowgraph::node::PureEvalHost,
				_p: &InputMap,
				inputs: &InputMap,
				_f: &ExecFireSet,
			) -> Result<NodeOutput, NodeExecError> {
				let v = decode_vec::<$n>(get_required_json(inputs, "v")?, "v")?;
				let r = $op::<$n>(v);
				Ok(NodeOutput::new().set_data("result", ($wrap)(r)?))
			}
		}
	};
}

macro_rules! vec_lerp_node {
	($name:ident, $feature:literal, $title:literal, $n:expr) => {
		pub struct $name;
		impl NodeDescriptor for $name {
			fn describe(&self) -> NodeSpec {
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "vec".into(),
					description: Some(
						concat!(
							"Componentwise linear interpolation: a + (b - a) * t for ",
							stringify!($n),
							"-vectors. t is not clamped."
						)
						.into(),
					),
					inputs: vec![
						PortSpec::input("a", "A", SocketType::Json),
						PortSpec::input("b", "B", SocketType::Json),
						PortSpec::input("t", "t", SocketType::Float),
					],
					outputs: vec![PortSpec::output("result", "Result", SocketType::Json)],
					properties: vec![],
				}
			}
		}
		#[async_trait]
		impl PureNode for $name {
			async fn compute(
				&self,
				_host: &crate::flowgraph::node::PureEvalHost,
				_p: &InputMap,
				inputs: &InputMap,
				_f: &ExecFireSet,
			) -> Result<NodeOutput, NodeExecError> {
				let a = decode_vec::<$n>(get_required_json(inputs, "a")?, "a")?;
				let b = decode_vec::<$n>(get_required_json(inputs, "b")?, "b")?;
				let t = get_required_float(inputs, "t")?;
				Ok(NodeOutput::new().set_data("result", SocketValue::Json(encode_vec::<$n>(lerp_n::<$n>(a, b, t))?)))
			}
		}
	};
}

// ---------------------------------------------------------------------------
// Concrete vec2 + vec3 nodes (10 each = 20 total)
// ---------------------------------------------------------------------------

vec_make_node!(Vec2MakeNode, "flowgraph.vec2.make", "Vec2 make", 2, ["x", "y"]);
vec_make_node!(Vec3MakeNode, "flowgraph.vec3.make", "Vec3 make", 3, ["x", "y", "z"]);

vec_unpack_node!(Vec2UnpackNode, "flowgraph.vec2.unpack", "Vec2 unpack", 2, ["x", "y"]);
vec_unpack_node!(Vec3UnpackNode, "flowgraph.vec3.unpack", "Vec3 unpack", 3, ["x", "y", "z"]);

vec_binop_node!(
	Vec2AddNode,
	"flowgraph.vec2.add",
	"Vec2 add",
	2,
	add_n,
	"Componentwise vec2 addition"
);
vec_binop_node!(
	Vec2SubNode,
	"flowgraph.vec2.sub",
	"Vec2 sub",
	2,
	sub_n,
	"Componentwise vec2 subtraction"
);
vec_binop_node!(
	Vec3AddNode,
	"flowgraph.vec3.add",
	"Vec3 add",
	3,
	add_n,
	"Componentwise vec3 addition"
);
vec_binop_node!(
	Vec3SubNode,
	"flowgraph.vec3.sub",
	"Vec3 sub",
	3,
	sub_n,
	"Componentwise vec3 subtraction"
);

vec_scale_node!(Vec2ScaleNode, "flowgraph.vec2.scale", "Vec2 scale", 2);
vec_scale_node!(Vec3ScaleNode, "flowgraph.vec3.scale", "Vec3 scale", 3);

vec_scalar_out_binop_node!(
	Vec2DotNode,
	"flowgraph.vec2.dot",
	"Vec2 dot",
	2,
	dot_n,
	"Vec2 dot product (returns scalar)"
);
vec_scalar_out_binop_node!(
	Vec3DotNode,
	"flowgraph.vec3.dot",
	"Vec3 dot",
	3,
	dot_n,
	"Vec3 dot product (returns scalar)"
);

vec_scalar_out_binop_node!(
	Vec2DistanceNode,
	"flowgraph.vec2.distance",
	"Vec2 distance",
	2,
	distance_n,
	"Euclidean distance between two vec2s"
);
vec_scalar_out_binop_node!(
	Vec3DistanceNode,
	"flowgraph.vec3.distance",
	"Vec3 distance",
	3,
	distance_n,
	"Euclidean distance between two vec3s"
);

fn wrap_float(x: f64) -> Result<SocketValue, NodeExecError> {
	Ok(SocketValue::Float(x))
}
fn wrap_vec2(x: [f64; 2]) -> Result<SocketValue, NodeExecError> {
	Ok(SocketValue::Json(encode_vec::<2>(x)?))
}
fn wrap_vec3(x: [f64; 3]) -> Result<SocketValue, NodeExecError> {
	Ok(SocketValue::Json(encode_vec::<3>(x)?))
}

vec_unary_pure_node!(
	Vec2LengthNode,
	"flowgraph.vec2.length",
	"Vec2 length",
	2,
	length_n,
	"Euclidean magnitude of a vec2: sqrt(x^2 + y^2)",
	SocketType::Float,
	wrap_float
);
vec_unary_pure_node!(
	Vec3LengthNode,
	"flowgraph.vec3.length",
	"Vec3 length",
	3,
	length_n,
	"Euclidean magnitude of a vec3: sqrt(x^2 + y^2 + z^2)",
	SocketType::Float,
	wrap_float
);

vec_unary_pure_node!(
	Vec2NormalizeNode,
	"flowgraph.vec2.normalize",
	"Vec2 normalize",
	2,
	normalize_n,
	"Scale a vec2 to unit length. Zero vector returns [0, 0] (not an error).",
	SocketType::Json,
	wrap_vec2
);
vec_unary_pure_node!(
	Vec3NormalizeNode,
	"flowgraph.vec3.normalize",
	"Vec3 normalize",
	3,
	normalize_n,
	"Scale a vec3 to unit length. Zero vector returns [0, 0, 0] (not an error).",
	SocketType::Json,
	wrap_vec3
);

vec_lerp_node!(Vec2LerpNode, "flowgraph.vec2.lerp", "Vec2 lerp", 2);
vec_lerp_node!(Vec3LerpNode, "flowgraph.vec3.lerp", "Vec3 lerp", 3);

// ===========================================================================
// Tests
// ===========================================================================

#[cfg(test)]
mod tests {
	use super::*;
	use serde_json::json;

	fn inp(pairs: &[(&str, SocketValue)]) -> InputMap {
		pairs.iter().cloned().map(|(k, v)| (k.to_string(), v)).collect()
	}

	async fn run_pure<N: PureNode>(n: &N, props: InputMap, inputs: InputMap) -> Result<NodeOutput, NodeExecError> {
		n.compute(
			&crate::flowgraph::node::PureEvalHost::default(),
			&props,
			&inputs,
			&ExecFireSet::new(),
		)
		.await
	}

	fn as_json(out: &NodeOutput, key: &str) -> JsonValue {
		out.data
			.get(key)
			.and_then(|v| match v {
				SocketValue::Json(j) => Some(j.clone()),
				_ => None,
			})
			.unwrap()
	}

	fn as_float(out: &NodeOutput, key: &str) -> f64 {
		out.data.get(key).and_then(|v| v.as_f64().ok()).unwrap()
	}

	// ---- decode/encode invariants ----

	#[test]
	fn decode_encode_round_trip_vec2() {
		let v = json!([3.0, 4.0]);
		let a = decode_vec::<2>(&v, "v").unwrap();
		assert_eq!(a, [3.0, 4.0]);
		let re = encode_vec::<2>(a).unwrap();
		assert_eq!(re, v);
	}

	#[test]
	fn decode_rejects_wrong_length() {
		let v = json!([1.0, 2.0, 3.0]);
		assert!(decode_vec::<2>(&v, "v").is_err());
	}

	#[test]
	fn decode_rejects_non_array() {
		let v = json!({"x": 1.0, "y": 2.0});
		assert!(decode_vec::<2>(&v, "v").is_err());
	}

	#[test]
	fn decode_rejects_non_finite() {
		// NaN is not representable in JSON Number at all \u{2014} it becomes Null on encode
		// and cannot be decoded via as_f64 either. Here we just check that non-number
		// entries are rejected.
		let v = json!([1.0, "oops"]);
		assert!(decode_vec::<2>(&v, "v").is_err());
	}

	#[test]
	fn encode_rejects_nan() {
		// Must produce a clean Err, not panic.
		let r = encode_vec::<2>([1.0, f64::NAN]);
		assert!(r.is_err());
	}

	// ---- make / unpack round trip ----

	#[tokio::test]
	async fn vec2_make_unpack_round_trip() {
		let made = run_pure(
			&Vec2MakeNode,
			InputMap::new(),
			inp(&[("x", SocketValue::Float(3.0)), ("y", SocketValue::Float(-4.0))]),
		)
		.await
		.unwrap();
		let v = as_json(&made, "v");
		assert_eq!(v, json!([3.0, -4.0]));

		let un = run_pure(&Vec2UnpackNode, InputMap::new(), inp(&[("v", SocketValue::Json(v))]))
			.await
			.unwrap();
		assert!((as_float(&un, "x") - 3.0).abs() < 1e-12);
		assert!((as_float(&un, "y") + 4.0).abs() < 1e-12);
	}

	#[tokio::test]
	async fn vec3_make_unpack_round_trip() {
		let made = run_pure(
			&Vec3MakeNode,
			InputMap::new(),
			inp(&[
				("x", SocketValue::Float(1.0)),
				("y", SocketValue::Float(2.0)),
				("z", SocketValue::Float(3.0)),
			]),
		)
		.await
		.unwrap();
		let v = as_json(&made, "v");
		assert_eq!(v, json!([1.0, 2.0, 3.0]));

		let un = run_pure(&Vec3UnpackNode, InputMap::new(), inp(&[("v", SocketValue::Json(v))]))
			.await
			.unwrap();
		assert_eq!(as_float(&un, "x"), 1.0);
		assert_eq!(as_float(&un, "y"), 2.0);
		assert_eq!(as_float(&un, "z"), 3.0);
	}

	// ---- binop (add / sub) ----

	#[tokio::test]
	async fn vec2_add_sub() {
		let a = SocketValue::Json(json!([1.0, 2.0]));
		let b = SocketValue::Json(json!([10.0, 20.0]));
		let sum = run_pure(&Vec2AddNode, InputMap::new(), inp(&[("a", a.clone()), ("b", b.clone())]))
			.await
			.unwrap();
		assert_eq!(as_json(&sum, "result"), json!([11.0, 22.0]));

		let diff = run_pure(&Vec2SubNode, InputMap::new(), inp(&[("a", b), ("b", a)])).await.unwrap();
		assert_eq!(as_json(&diff, "result"), json!([9.0, 18.0]));
	}

	#[tokio::test]
	async fn vec3_add_and_dim_mismatch_errors() {
		let a = SocketValue::Json(json!([1.0, 2.0, 3.0]));
		let b = SocketValue::Json(json!([10.0, 20.0, 30.0]));
		let sum = run_pure(&Vec3AddNode, InputMap::new(), inp(&[("a", a), ("b", b)])).await.unwrap();
		assert_eq!(as_json(&sum, "result"), json!([11.0, 22.0, 33.0]));

		// Wrong dimension on one side \u{2192} decode error.
		let wrong = run_pure(
			&Vec3AddNode,
			InputMap::new(),
			inp(&[
				("a", SocketValue::Json(json!([1.0, 2.0]))),
				("b", SocketValue::Json(json!([10.0, 20.0, 30.0]))),
			]),
		)
		.await;
		assert!(matches!(wrong, Err(NodeExecError::Generic(_))));
	}

	// ---- scale ----

	#[tokio::test]
	async fn vec2_scale() {
		let out = run_pure(
			&Vec2ScaleNode,
			InputMap::new(),
			inp(&[("a", SocketValue::Json(json!([2.0, -3.0]))), ("k", SocketValue::Float(4.0))]),
		)
		.await
		.unwrap();
		assert_eq!(as_json(&out, "result"), json!([8.0, -12.0]));
	}

	// ---- dot / length / distance ----

	#[tokio::test]
	async fn vec2_dot_and_length_and_distance() {
		let d = run_pure(
			&Vec2DotNode,
			InputMap::new(),
			inp(&[
				("a", SocketValue::Json(json!([1.0, 2.0]))),
				("b", SocketValue::Json(json!([3.0, 4.0]))),
			]),
		)
		.await
		.unwrap();
		assert!((as_float(&d, "result") - 11.0).abs() < 1e-12);

		let l = run_pure(
			&Vec2LengthNode,
			InputMap::new(),
			inp(&[("v", SocketValue::Json(json!([3.0, 4.0])))]),
		)
		.await
		.unwrap();
		assert!((as_float(&l, "result") - 5.0).abs() < 1e-12);

		let dist = run_pure(
			&Vec2DistanceNode,
			InputMap::new(),
			inp(&[
				("a", SocketValue::Json(json!([0.0, 0.0]))),
				("b", SocketValue::Json(json!([3.0, 4.0]))),
			]),
		)
		.await
		.unwrap();
		assert!((as_float(&dist, "result") - 5.0).abs() < 1e-12);
	}

	#[tokio::test]
	async fn vec3_length_3_4_12_is_13() {
		let l = run_pure(
			&Vec3LengthNode,
			InputMap::new(),
			inp(&[("v", SocketValue::Json(json!([3.0, 4.0, 12.0])))]),
		)
		.await
		.unwrap();
		assert!((as_float(&l, "result") - 13.0).abs() < 1e-12);
	}

	// ---- normalize (and zero-vector spec) ----

	#[tokio::test]
	async fn vec2_normalize_unit() {
		let n = run_pure(
			&Vec2NormalizeNode,
			InputMap::new(),
			inp(&[("v", SocketValue::Json(json!([3.0, 4.0])))]),
		)
		.await
		.unwrap();
		let arr = as_json(&n, "result");
		let v: Vec<f64> = arr.as_array().unwrap().iter().map(|x| x.as_f64().unwrap()).collect();
		assert!((v[0] - 0.6).abs() < 1e-12);
		assert!((v[1] - 0.8).abs() < 1e-12);
	}

	#[tokio::test]
	async fn vec2_normalize_zero_vector_stays_zero() {
		let n = run_pure(
			&Vec2NormalizeNode,
			InputMap::new(),
			inp(&[("v", SocketValue::Json(json!([0.0, 0.0])))]),
		)
		.await
		.unwrap();
		assert_eq!(as_json(&n, "result"), json!([0.0, 0.0]));
	}

	#[tokio::test]
	async fn vec3_normalize_zero_vector_stays_zero() {
		let n = run_pure(
			&Vec3NormalizeNode,
			InputMap::new(),
			inp(&[("v", SocketValue::Json(json!([0.0, 0.0, 0.0])))]),
		)
		.await
		.unwrap();
		assert_eq!(as_json(&n, "result"), json!([0.0, 0.0, 0.0]));
	}

	// ---- lerp ----

	#[tokio::test]
	async fn vec2_lerp_endpoints_and_midpoint() {
		let a = SocketValue::Json(json!([0.0, 0.0]));
		let b = SocketValue::Json(json!([10.0, 20.0]));

		let at0 = run_pure(
			&Vec2LerpNode,
			InputMap::new(),
			inp(&[("a", a.clone()), ("b", b.clone()), ("t", SocketValue::Float(0.0))]),
		)
		.await
		.unwrap();
		assert_eq!(as_json(&at0, "result"), json!([0.0, 0.0]));

		let at1 = run_pure(
			&Vec2LerpNode,
			InputMap::new(),
			inp(&[("a", a.clone()), ("b", b.clone()), ("t", SocketValue::Float(1.0))]),
		)
		.await
		.unwrap();
		assert_eq!(as_json(&at1, "result"), json!([10.0, 20.0]));

		let at_half = run_pure(
			&Vec2LerpNode,
			InputMap::new(),
			inp(&[("a", a), ("b", b), ("t", SocketValue::Float(0.5))]),
		)
		.await
		.unwrap();
		assert_eq!(as_json(&at_half, "result"), json!([5.0, 10.0]));
	}

	#[tokio::test]
	async fn vec3_lerp_unclamped_t_extrapolates() {
		// Mirrors the phase-doc promise that t is not clamped.
		let a = SocketValue::Json(json!([0.0, 0.0, 0.0]));
		let b = SocketValue::Json(json!([1.0, 2.0, 3.0]));
		let out = run_pure(
			&Vec3LerpNode,
			InputMap::new(),
			inp(&[("a", a), ("b", b), ("t", SocketValue::Float(2.0))]),
		)
		.await
		.unwrap();
		assert_eq!(as_json(&out, "result"), json!([2.0, 4.0, 6.0]));
	}

	// ---- spec shape sanity ----

	#[test]
	fn vec2_specs_have_expected_features() {
		let features = [
			Vec2MakeNode.describe().feature,
			Vec2UnpackNode.describe().feature,
			Vec2AddNode.describe().feature,
			Vec2SubNode.describe().feature,
			Vec2ScaleNode.describe().feature,
			Vec2DotNode.describe().feature,
			Vec2LengthNode.describe().feature,
			Vec2NormalizeNode.describe().feature,
			Vec2LerpNode.describe().feature,
			Vec2DistanceNode.describe().feature,
		];
		assert_eq!(features.len(), 10);
		for f in &features {
			assert!(f.starts_with("flowgraph.vec2."), "{f}");
		}
	}

	#[test]
	fn vec3_specs_have_expected_features() {
		let features = [
			Vec3MakeNode.describe().feature,
			Vec3UnpackNode.describe().feature,
			Vec3AddNode.describe().feature,
			Vec3SubNode.describe().feature,
			Vec3ScaleNode.describe().feature,
			Vec3DotNode.describe().feature,
			Vec3LengthNode.describe().feature,
			Vec3NormalizeNode.describe().feature,
			Vec3LerpNode.describe().feature,
			Vec3DistanceNode.describe().feature,
		];
		assert_eq!(features.len(), 10);
		for f in &features {
			assert!(f.starts_with("flowgraph.vec3."), "{f}");
		}
	}
}
