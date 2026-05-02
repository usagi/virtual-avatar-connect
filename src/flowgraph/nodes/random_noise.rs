//! Phase ο-5: `flowgraph.random.*` and `flowgraph.noise.*` (PureNode, `rand::rng()` + `noise` crate).
//!
//! Random sampling uses `rand::random_range` (thread-local RNG). Seeded reproducibility is **not**
//! implemented here (Phase ο+ TBD per roadmap §3.6).
//!
//! Perlin instances are cached per `seed` (as `u32`) in a process-global `Mutex<HashMap>`.

use crate::flowgraph::node::{
	get_required_float, get_required_int, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec, PureNode,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use noise::{NoiseFn, Perlin};
use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

static PERLIN_CACHE: LazyLock<Mutex<HashMap<u32, Perlin>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

fn perlin(seed: i64) -> Perlin {
	let u = seed.rem_euclid(u32::MAX as i64) as u32;
	let mut g = PERLIN_CACHE.lock().unwrap();
	*g.entry(u).or_insert_with(|| Perlin::new(u))
}

// ---------------------------------------------------------------------
// random.uniform_int
// ---------------------------------------------------------------------

pub struct RandomUniformIntNode;

impl NodeDescriptor for RandomUniformIntNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.random.uniform_int".into(),
			title: "Random uniform (int)".into(),
			category: "random".into(),
			description: Some("Uniform random integer in [lo, hi] inclusive.".into()),
			inputs: vec![
				PortSpec::input("lo", "Low", SocketType::Int),
				PortSpec::input("hi", "High", SocketType::Int),
			],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Int)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for RandomUniformIntNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let mut lo = get_required_int(inputs, "lo")?;
		let mut hi = get_required_int(inputs, "hi")?;
		if lo > hi {
			std::mem::swap(&mut lo, &mut hi);
		}
		let v = rand::random_range(lo..=hi);
		Ok(NodeOutput::new().set_data("value", SocketValue::Int(v)))
	}
}

// ---------------------------------------------------------------------
// random.uniform_float
// ---------------------------------------------------------------------

pub struct RandomUniformFloatNode;

impl NodeDescriptor for RandomUniformFloatNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.random.uniform_float".into(),
			title: "Random uniform (float)".into(),
			category: "random".into(),
			description: Some("Uniform random float in [lo, hi) half-open interval.".into()),
			inputs: vec![
				PortSpec::input("lo", "Low", SocketType::Float),
				PortSpec::input("hi", "High", SocketType::Float),
			],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Float)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for RandomUniformFloatNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let mut lo = get_required_float(inputs, "lo")?;
		let mut hi = get_required_float(inputs, "hi")?;
		if !lo.is_finite() || !hi.is_finite() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"random.uniform_float: lo/hi must be finite"
			)));
		}
		if lo > hi {
			std::mem::swap(&mut lo, &mut hi);
		}
		if lo == hi {
			return Ok(NodeOutput::new().set_data("value", SocketValue::Float(lo)));
		}
		let v: f64 = rand::random_range(lo..hi);
		Ok(NodeOutput::new().set_data("value", SocketValue::Float(v)))
	}
}

// ---------------------------------------------------------------------
// random.normal
// ---------------------------------------------------------------------

pub struct RandomNormalNode;

impl NodeDescriptor for RandomNormalNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.random.normal".into(),
			title: "Random normal".into(),
			category: "random".into(),
			description: Some(
				"Gaussian sample (Box–Muller) with given mean and stddev. stddev must be non-negative; 0 yields mean.".into(),
			),
			inputs: vec![
				PortSpec::input("mean", "Mean", SocketType::Float),
				PortSpec::input("stddev", "Std dev", SocketType::Float),
			],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Float)],
			properties: vec![],
		}
	}
}

fn box_muller_pair() -> (f64, f64) {
	let u1 = loop {
		let u: f64 = rand::random_range(f64::MIN_POSITIVE..1.0);
		if u > 0.0 {
			break u;
		}
	};
	let u2: f64 = rand::random();
	let r = (-2.0 * u1.ln()).sqrt();
	let theta = 2.0 * std::f64::consts::PI * u2;
	(r * theta.cos(), r * theta.sin())
}

#[async_trait]
impl PureNode for RandomNormalNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let mean = get_required_float(inputs, "mean")?;
		let stddev = get_required_float(inputs, "stddev")?;
		if !mean.is_finite() {
			return Err(NodeExecError::Generic(anyhow::anyhow!("random.normal: mean must be finite")));
		}
		if stddev < 0.0 || !stddev.is_finite() {
			return Err(NodeExecError::Generic(anyhow::anyhow!(
				"random.normal: stddev must be finite and >= 0"
			)));
		}
		if stddev == 0.0 {
			return Ok(NodeOutput::new().set_data("value", SocketValue::Float(mean)));
		}
		let (z0, _) = box_muller_pair();
		Ok(NodeOutput::new().set_data("value", SocketValue::Float(mean + stddev * z0)))
	}
}

// ---------------------------------------------------------------------
// noise.perlin_1d
// ---------------------------------------------------------------------

pub struct NoisePerlin1dNode;

impl NodeDescriptor for NoisePerlin1dNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.noise.perlin_1d".into(),
			title: "Perlin 1D".into(),
			category: "noise".into(),
			description: Some("1D Perlin noise at coordinate `t` with integer `seed` (cached Perlin per seed).".into()),
			inputs: vec![
				PortSpec::input("t", "t", SocketType::Float),
				PortSpec::input("seed", "Seed", SocketType::Int).with_default(SocketValue::Int(0)),
			],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Float)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for NoisePerlin1dNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let t = get_required_float(inputs, "t")?;
		let seed = get_required_int(inputs, "seed")?;
		if !t.is_finite() {
			return Err(NodeExecError::Generic(anyhow::anyhow!("noise.perlin_1d: t must be finite")));
		}
		let p = perlin(seed);
		let v = p.get([t]);
		Ok(NodeOutput::new().set_data("value", SocketValue::Float(v)))
	}
}

// ---------------------------------------------------------------------
// noise.perlin_2d
// ---------------------------------------------------------------------

pub struct NoisePerlin2dNode;

impl NodeDescriptor for NoisePerlin2dNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.noise.perlin_2d".into(),
			title: "Perlin 2D".into(),
			category: "noise".into(),
			description: Some("2D Perlin noise at `(x, y)` with integer `seed`.".into()),
			inputs: vec![
				PortSpec::input("x", "x", SocketType::Float),
				PortSpec::input("y", "y", SocketType::Float),
				PortSpec::input("seed", "Seed", SocketType::Int).with_default(SocketValue::Int(0)),
			],
			outputs: vec![PortSpec::output("value", "Value", SocketType::Float)],
			properties: vec![],
		}
	}
}

#[async_trait]
impl PureNode for NoisePerlin2dNode {
	async fn compute(
		&self,
		_host: &crate::flowgraph::node::PureEvalHost,
		_props: &InputMap,
		inputs: &InputMap,
		_fired: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		let x = get_required_float(inputs, "x")?;
		let y = get_required_float(inputs, "y")?;
		let seed = get_required_int(inputs, "seed")?;
		if !x.is_finite() || !y.is_finite() {
			return Err(NodeExecError::Generic(anyhow::anyhow!("noise.perlin_2d: x/y must be finite")));
		}
		let p = perlin(seed);
		let v = p.get([x, y]);
		Ok(NodeOutput::new().set_data("value", SocketValue::Float(v)))
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[tokio::test]
	async fn uniform_int_range_inclusive() {
		let n = RandomUniformIntNode;
		let inputs: InputMap = [("lo".into(), SocketValue::Int(10)), ("hi".into(), SocketValue::Int(10))]
			.into_iter()
			.collect();
		let out = n
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(out.data.get("value").unwrap().as_i64().unwrap(), 10);
	}

	#[tokio::test]
	async fn uniform_float_half_open() {
		let n = RandomUniformFloatNode;
		let inputs: InputMap = [("lo".into(), SocketValue::Float(0.0)), ("hi".into(), SocketValue::Float(1.0))]
			.into_iter()
			.collect();
		let out = n
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let v = out.data.get("value").unwrap().as_f64().unwrap();
		assert!((0.0..1.0).contains(&v), "v={v}");
	}

	#[tokio::test]
	async fn normal_zero_stddev_is_mean() {
		let n = RandomNormalNode;
		let inputs: InputMap = [("mean".into(), SocketValue::Float(3.5)), ("stddev".into(), SocketValue::Float(0.0))]
			.into_iter()
			.collect();
		let out = n
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert!((out.data.get("value").unwrap().as_f64().unwrap() - 3.5).abs() < 1e-12);
	}

	#[tokio::test]
	async fn perlin_1d_deterministic_for_seed() {
		let n = NoisePerlin1dNode;
		let inputs: InputMap = [("t".into(), SocketValue::Float(0.25)), ("seed".into(), SocketValue::Int(42))]
			.into_iter()
			.collect();
		let a = n
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		let b = n
			.compute(
				&crate::flowgraph::node::PureEvalHost::default(),
				&InputMap::new(),
				&inputs,
				&ExecFireSet::new(),
			)
			.await
			.unwrap();
		assert_eq!(a.data.get("value"), b.data.get("value"));
		let v = a.data.get("value").unwrap().as_f64().unwrap();
		assert!(v.is_finite() && v.abs() <= 2.0, "perlin sample: {v}");
	}
}
