//! Easing nodes (Phase \u{03BF}-2).
//!
//! `flowgraph.easing.apply`: single PureNode with a `curve` enum property and
//! an optional `clamp_t` bool. Input `t: Float` (dimensionless semantics, but
//! wired as plain `Float` to keep upstream flows trivial). Output `value: Float`.
//!
//! Design: one node + 19 curves (linear + 6 families x 3 modes) over 18
//! individual nodes. Rationale in `docs/roadmap/phase-omicron-flowgraph-enhancement.md` \u{00A7}5.2.
//!
//! Formulas follow Robert Penner's easing equations (public domain).
//! `elastic_*` and `bounce_*` can overshoot the [0, 1] range when `clamp_t = false`
//! and the input itself is out of range; this is intentional (the curve math is
//! defined on the full real line). The `clamp_t` default is `true` to keep the
//! output bounded to the curve's design range for `t \u{2208} [0, 1]`.

use crate::flowgraph::node::{
	get_required_float, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
	PropertySpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

// ---------------------------------------------------------------------------
// Curve enum + string lookup
// ---------------------------------------------------------------------------

/// All 19 easing curves supported by `flowgraph.easing.apply`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Curve {
	Linear,
	QuadIn,
	QuadOut,
	QuadInOut,
	CubicIn,
	CubicOut,
	CubicInOut,
	SineIn,
	SineOut,
	SineInOut,
	ExpoIn,
	ExpoOut,
	ExpoInOut,
	ElasticIn,
	ElasticOut,
	ElasticInOut,
	BounceIn,
	BounceOut,
	BounceInOut,
}

impl Curve {
	pub const ALL: &'static [(&'static str, Curve)] = &[
		("linear", Curve::Linear),
		("quad_in", Curve::QuadIn),
		("quad_out", Curve::QuadOut),
		("quad_inout", Curve::QuadInOut),
		("cubic_in", Curve::CubicIn),
		("cubic_out", Curve::CubicOut),
		("cubic_inout", Curve::CubicInOut),
		("sine_in", Curve::SineIn),
		("sine_out", Curve::SineOut),
		("sine_inout", Curve::SineInOut),
		("expo_in", Curve::ExpoIn),
		("expo_out", Curve::ExpoOut),
		("expo_inout", Curve::ExpoInOut),
		("elastic_in", Curve::ElasticIn),
		("elastic_out", Curve::ElasticOut),
		("elastic_inout", Curve::ElasticInOut),
		("bounce_in", Curve::BounceIn),
		("bounce_out", Curve::BounceOut),
		("bounce_inout", Curve::BounceInOut),
	];

	pub fn from_name(s: &str) -> Option<Curve> {
		Curve::ALL.iter().find(|(n, _)| *n == s).map(|(_, c)| *c)
	}

	pub fn all_names() -> Vec<String> {
		Curve::ALL.iter().map(|(n, _)| (*n).to_string()).collect()
	}
}

// ---------------------------------------------------------------------------
// Curve math
// ---------------------------------------------------------------------------

/// Evaluate `curve` at `t`. `t` is not clamped here \u{2014} that is the caller's job
/// (see `clamp_t` property in `EasingApplyNode`).
pub fn apply_curve(curve: Curve, t: f64) -> f64 {
	match curve {
		Curve::Linear => t,

		Curve::QuadIn => t * t,
		Curve::QuadOut => 1.0 - (1.0 - t) * (1.0 - t),
		Curve::QuadInOut => {
			if t < 0.5 {
				2.0 * t * t
			} else {
				1.0 - (-2.0 * t + 2.0).powi(2) / 2.0
			}
		}

		Curve::CubicIn => t * t * t,
		Curve::CubicOut => {
			let u = 1.0 - t;
			1.0 - u * u * u
		}
		Curve::CubicInOut => {
			if t < 0.5 {
				4.0 * t * t * t
			} else {
				1.0 - (-2.0 * t + 2.0).powi(3) / 2.0
			}
		}

		Curve::SineIn => 1.0 - (t * std::f64::consts::FRAC_PI_2).cos(),
		Curve::SineOut => (t * std::f64::consts::FRAC_PI_2).sin(),
		Curve::SineInOut => -((std::f64::consts::PI * t).cos() - 1.0) / 2.0,

		Curve::ExpoIn => {
			if t == 0.0 {
				0.0
			} else {
				2.0_f64.powf(10.0 * t - 10.0)
			}
		}
		Curve::ExpoOut => {
			if (t - 1.0).abs() < f64::EPSILON {
				1.0
			} else {
				1.0 - 2.0_f64.powf(-10.0 * t)
			}
		}
		Curve::ExpoInOut => {
			if t == 0.0 {
				0.0
			} else if (t - 1.0).abs() < f64::EPSILON {
				1.0
			} else if t < 0.5 {
				2.0_f64.powf(20.0 * t - 10.0) / 2.0
			} else {
				(2.0 - 2.0_f64.powf(-20.0 * t + 10.0)) / 2.0
			}
		}

		Curve::ElasticIn => {
			const C4: f64 = (2.0 * std::f64::consts::PI) / 3.0;
			if t == 0.0 {
				0.0
			} else if (t - 1.0).abs() < f64::EPSILON {
				1.0
			} else {
				-(2.0_f64.powf(10.0 * t - 10.0)) * ((t * 10.0 - 10.75) * C4).sin()
			}
		}
		Curve::ElasticOut => {
			const C4: f64 = (2.0 * std::f64::consts::PI) / 3.0;
			if t == 0.0 {
				0.0
			} else if (t - 1.0).abs() < f64::EPSILON {
				1.0
			} else {
				2.0_f64.powf(-10.0 * t) * ((t * 10.0 - 0.75) * C4).sin() + 1.0
			}
		}
		Curve::ElasticInOut => {
			const C5: f64 = (2.0 * std::f64::consts::PI) / 4.5;
			if t == 0.0 {
				0.0
			} else if (t - 1.0).abs() < f64::EPSILON {
				1.0
			} else if t < 0.5 {
				-(2.0_f64.powf(20.0 * t - 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0
			} else {
				(2.0_f64.powf(-20.0 * t + 10.0) * ((20.0 * t - 11.125) * C5).sin()) / 2.0 + 1.0
			}
		}

		Curve::BounceIn => 1.0 - bounce_out(1.0 - t),
		Curve::BounceOut => bounce_out(t),
		Curve::BounceInOut => {
			if t < 0.5 {
				(1.0 - bounce_out(1.0 - 2.0 * t)) / 2.0
			} else {
				(1.0 + bounce_out(2.0 * t - 1.0)) / 2.0
			}
		}
	}
}

fn bounce_out(t: f64) -> f64 {
	const N1: f64 = 7.5625;
	const D1: f64 = 2.75;
	if t < 1.0 / D1 {
		N1 * t * t
	} else if t < 2.0 / D1 {
		let t = t - 1.5 / D1;
		N1 * t * t + 0.75
	} else if t < 2.5 / D1 {
		let t = t - 2.25 / D1;
		N1 * t * t + 0.9375
	} else {
		let t = t - 2.625 / D1;
		N1 * t * t + 0.984375
	}
}

// ---------------------------------------------------------------------------
// EasingApplyNode
// ---------------------------------------------------------------------------

pub struct EasingApplyNode;

impl NodeDescriptor for EasingApplyNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.easing.apply".into(),
			title: "Easing apply".into(),
			category: "easing".into(),
			description: Some(
				"Map a scalar t (conventionally in [0, 1]) through an easing curve. \
				 Single node with a `curve` enum property (19 variants: linear, quad/cubic/sine/expo/elastic/bounce \u{00D7} in/out/inout). \
				 `clamp_t = true` (default) clamps input to [0, 1] before evaluation \u{2014} set false to let elastic/bounce overshoot naturally."
					.into(),
			),
			inputs: vec![PortSpec::input("t", "t", SocketType::Float)],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Float)],
			properties: vec![
				PropertySpec::new(
					"curve",
					"Curve",
					SocketType::String,
					SocketValue::String("linear".into()),
				)
				.description(
					"Easing curve name. One of: linear, quad_in/out/inout, cubic_in/out/inout, sine_in/out/inout, \
					 expo_in/out/inout, elastic_in/out/inout, bounce_in/out/inout.",
				)
				.with_choices(Curve::all_names()),
				PropertySpec::new("clamp_t", "Clamp t to [0, 1]", SocketType::Bool, SocketValue::Bool(true))
					.description("When true, t is clamped to [0, 1] before the curve is applied. Default true."),
			],
		}
	}
}

#[async_trait]
impl PureNode for EasingApplyNode {
	async fn compute(
		&self,
		properties: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let mut t = get_required_float(inputs, "t")?;
		let curve_name = properties
			.get("curve")
			.and_then(|v| v.as_str().ok())
			.unwrap_or("linear")
			.to_string();
		let clamp_t = properties.get("clamp_t").and_then(|v| v.as_bool().ok()).unwrap_or(true);

		let curve = Curve::from_name(&curve_name).ok_or_else(|| {
			NodeExecError::Generic(anyhow::anyhow!(
				"unknown easing curve: '{}'. Expected one of: {}",
				curve_name,
				Curve::all_names().join(", ")
			))
		})?;

		if clamp_t {
			t = t.clamp(0.0, 1.0);
		}

		let value = apply_curve(curve, t);
		Ok(NodeOutput::new().set_data("value", SocketValue::Float(value)))
	}
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
	use super::*;

	fn props(curve: &str, clamp_t: bool) -> InputMap {
		[
			("curve".into(), SocketValue::String(curve.into())),
			("clamp_t".into(), SocketValue::Bool(clamp_t)),
		]
		.into_iter()
		.collect()
	}

	fn input_t(t: f64) -> InputMap {
		[("t".into(), SocketValue::Float(t))].into_iter().collect()
	}

	async fn run(curve: &str, t: f64, clamp: bool) -> f64 {
		let out = EasingApplyNode
			.compute(&props(curve, clamp), &input_t(t), &ExecFireSet::new())
			.await
			.unwrap();
		out.data.get("value").and_then(|v| v.as_f64().ok()).unwrap()
	}

	#[test]
	fn curve_from_name_round_trip_all_19() {
		let names = Curve::all_names();
		assert_eq!(names.len(), 19);
		for n in &names {
			assert!(Curve::from_name(n).is_some(), "missing: {n}");
		}
		assert!(Curve::from_name("nonsense").is_none());
	}

	#[tokio::test]
	async fn linear_identity() {
		assert!((run("linear", 0.0, true).await - 0.0).abs() < 1e-12);
		assert!((run("linear", 0.25, true).await - 0.25).abs() < 1e-12);
		assert!((run("linear", 0.5, true).await - 0.5).abs() < 1e-12);
		assert!((run("linear", 1.0, true).await - 1.0).abs() < 1e-12);
	}

	#[tokio::test]
	async fn endpoints_are_zero_and_one_for_all_curves() {
		// Property: curve(0) = 0, curve(1) = 1 for every canonical easing.
		// (Elastic / bounce are handled with special cases at endpoints.)
		for (name, _) in Curve::ALL {
			let at0 = run(name, 0.0, true).await;
			let at1 = run(name, 1.0, true).await;
			assert!(at0.abs() < 1e-9, "{name}: f(0) != 0, got {at0}");
			assert!((at1 - 1.0).abs() < 1e-9, "{name}: f(1) != 1, got {at1}");
		}
	}

	#[tokio::test]
	async fn quad_cubic_inout_hit_half_at_half() {
		// *InOut families pass through (0.5, 0.5).
		for name in &["quad_inout", "cubic_inout", "sine_inout", "expo_inout", "elastic_inout"] {
			let y = run(name, 0.5, true).await;
			assert!((y - 0.5).abs() < 1e-9, "{name}(0.5) != 0.5, got {y}");
		}
	}

	#[tokio::test]
	async fn quad_in_slow_start() {
		// quad_in(0.5) = 0.25, cubic_in(0.5) = 0.125.
		assert!((run("quad_in", 0.5, true).await - 0.25).abs() < 1e-12);
		assert!((run("cubic_in", 0.5, true).await - 0.125).abs() < 1e-12);
	}

	#[tokio::test]
	async fn out_variants_mirror_in() {
		// f_out(t) = 1 - f_in(1 - t). Check at t = 0.3 for quad and cubic.
		for (inn, outn) in &[("quad_in", "quad_out"), ("cubic_in", "cubic_out")] {
			let a = run(inn, 0.3, true).await;
			let b = run(outn, 0.7, true).await;
			assert!((a + b - 1.0).abs() < 1e-9, "{inn}/{outn} mirror failed: {a}, {b}");
		}
	}

	#[tokio::test]
	async fn clamp_t_true_confines_to_design_range() {
		// Even with t = 1.5, clamp forces it to 1.0 \u{2192} output 1.0 for canonical curves.
		let y = run("quad_in", 1.5, true).await;
		assert!((y - 1.0).abs() < 1e-9);
		let y = run("quad_in", -0.5, true).await;
		assert!(y.abs() < 1e-9);
	}

	#[tokio::test]
	async fn clamp_t_false_lets_input_through() {
		// quad_in(2.0) = 4.0 when unclamped.
		let y = run("quad_in", 2.0, false).await;
		assert!((y - 4.0).abs() < 1e-9);
	}

	#[tokio::test]
	async fn bounce_out_checkpoints() {
		// bounce_out is piecewise parabolic. At t = 1 it hits 1 (already tested),
		// and its first "landing" back to \u{2248}0.75 area is near t \u{2248} 1.5 / 2.75.
		let y = run("bounce_out", 1.5 / 2.75, true).await;
		assert!(y > 0.7 && y < 0.8, "bounce_out(1.5/2.75) out of band: {y}");
	}

	#[tokio::test]
	async fn elastic_overshoots_below_zero_mid_anim() {
		// elastic_in has negative values in (0, 1). Sample a point known to be negative.
		let y = run("elastic_in", 0.2, true).await;
		assert!(y < 0.0, "elastic_in(0.2) expected < 0, got {y}");
	}

	#[tokio::test]
	async fn unknown_curve_errors() {
		let out = EasingApplyNode
			.compute(
				&props("my_fancy_ease", true),
				&input_t(0.5),
				&ExecFireSet::new(),
			)
			.await
			.unwrap_err();
		assert!(matches!(out, NodeExecError::Generic(_)));
	}

	#[tokio::test]
	async fn default_curve_is_linear() {
		// Omit curve property \u{2192} should fall back to "linear".
		let props: InputMap = [("clamp_t".into(), SocketValue::Bool(true))].into_iter().collect();
		let out = EasingApplyNode
			.compute(&props, &input_t(0.42), &ExecFireSet::new())
			.await
			.unwrap();
		let y = out.data.get("value").and_then(|v| v.as_f64().ok()).unwrap();
		assert!((y - 0.42).abs() < 1e-12);
	}

	#[test]
	fn spec_declares_choices_on_curve_property() {
		let spec = EasingApplyNode.describe();
		let curve_prop = spec.find_property("curve").expect("curve prop missing");
		let choices = curve_prop.choices.as_ref().expect("curve should have choices");
		assert_eq!(choices.len(), 19);
		assert!(choices.iter().any(|s| s == "linear"));
		assert!(choices.iter().any(|s| s == "bounce_inout"));
	}
}
