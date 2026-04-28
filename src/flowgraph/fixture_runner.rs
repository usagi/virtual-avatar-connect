//! Flowgraph **fixture ランナー**の最小実装（language roadmap P2 / PR4 先取り）。
//!
//! `load_flowgraph_dir` でディレクトリをロードし、可能なら 1-shot [`FlowgraphProgram::execute`] を走らせる。
//! 外部 IO・ingress・`state_handle` 必須ノードを含むグラフは失敗しうるため、CI 用の厳密 runner ではなく
//! **ロード＋純粋グラフのスモーク**向け。

use crate::flowgraph::node::{ExecCtx, SocketValueRepr};
use crate::flowgraph::{load_flowgraph_dir, FlowgraphProgram, LoadError, NodeExecError, ProgramRun};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// ロードに成功したときの結果。
pub struct FixtureLoadOk {
	pub program: FlowgraphProgram,
}

/// ロード失敗、または（要求時）実行失敗。
#[derive(Debug)]
pub enum FixtureError {
	Load(LoadError),
	Execute(NodeExecError),
	TestIo(std::io::Error),
	TestParse { path: PathBuf, error: toml::de::Error },
}

impl std::fmt::Display for FixtureError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			FixtureError::Load(e) => write!(f, "load failed: {e}"),
			FixtureError::Execute(e) => write!(f, "execute failed: {e}"),
			FixtureError::TestIo(e) => write!(f, "test file io failed: {e}"),
			FixtureError::TestParse { path, error } => write!(f, "test file parse failed: {}: {error}", path.display()),
		}
	}
}

impl std::error::Error for FixtureError {}

#[derive(Debug, Serialize)]
pub struct FixtureTraceValue {
	pub node: String,
	pub port: String,
	pub ty: String,
	pub value: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub struct FixtureRunReport {
	pub ok: bool,
	pub root: String,
	pub generation: u64,
	pub node_count: usize,
	pub trace: Vec<String>,
	pub trace_count: usize,
	pub stored_values: Vec<FixtureTraceValue>,
	pub exec_count: Vec<(String, usize)>,
	pub pure_evaluations: Vec<(String, usize)>,
	pub cache_hits: usize,
	pub cache_misses: usize,
	pub tests: Vec<FixtureTestResult>,
	pub failed_tests: usize,
}

#[derive(Debug, Serialize)]
pub struct FixtureTestResult {
	pub file: String,
	pub name: String,
	pub ok: bool,
	pub failures: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureTestFile {
	#[serde(default)]
	tests: Vec<FixtureTestCase>,
}

#[derive(Debug, Deserialize)]
struct FixtureTestCase {
	name: Option<String>,
	#[serde(default)]
	expect: FixtureExpect,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureExpect {
	node_count: Option<usize>,
	trace_count: Option<usize>,
	trace: Option<Vec<String>>,
}

/// `flowgraph_dir` をロードする。診断付き失敗は [`FixtureError::Load`]。
pub fn load_fixture_program(root: &Path) -> Result<FixtureLoadOk, FixtureError> {
	let report = load_flowgraph_dir(root).map_err(FixtureError::Load)?;
	Ok(FixtureLoadOk { program: report.program })
}

/// ロード後に `execute` を 1 回試行する（headless `ExecCtx`）。
pub async fn load_and_execute_once(root: &Path) -> Result<ProgramRun, FixtureError> {
	let mut fixture = load_fixture_program(root)?;
	let mut ctx = ExecCtx::default();
	fixture.program.execute(&mut ctx).await.map_err(FixtureError::Execute)
}

pub async fn run_fixture_once_report(root: &Path) -> Result<FixtureRunReport, FixtureError> {
	let mut fixture = load_fixture_program(root)?;
	let node_count = fixture.program.node_ids().count();
	let mut ctx = ExecCtx::default();
	let run = fixture.program.execute(&mut ctx).await.map_err(FixtureError::Execute)?;
	let mut report = make_report(root, node_count, run, ctx);
	report.tests = run_declared_tests(root, &report)?;
	report.failed_tests = report.tests.iter().filter(|t| !t.ok).count();
	report.ok = report.failed_tests == 0;
	Ok(report)
}

fn make_report(root: &Path, node_count: usize, run: ProgramRun, ctx: ExecCtx) -> FixtureRunReport {
	let mut stored_values: Vec<FixtureTraceValue> = run
		.stored_values
		.iter()
		.map(|(port_ref, value)| FixtureTraceValue {
			node: port_ref.node.clone(),
			port: port_ref.port.clone(),
			ty: value.type_of().to_string(),
			value: SocketValueRepr::from_value(value).0,
		})
		.collect();
	stored_values.sort_by(|a, b| (&a.node, &a.port).cmp(&(&b.node, &b.port)));

	let mut exec_count: Vec<(String, usize)> = run.exec_count.into_iter().collect();
	exec_count.sort_by(|a, b| a.0.cmp(&b.0));

	let mut pure_evaluations: Vec<(String, usize)> = run.pure_evaluations.into_iter().collect();
	pure_evaluations.sort_by(|a, b| a.0.cmp(&b.0));

	FixtureRunReport {
		ok: true,
		root: root.display().to_string(),
		generation: run.generation,
		node_count,
		trace_count: ctx.trace.len(),
		trace: ctx.trace,
		stored_values,
		exec_count,
		pure_evaluations,
		cache_hits: run.cache_hits,
		cache_misses: run.cache_misses,
		tests: Vec::new(),
		failed_tests: 0,
	}
}

fn discover_test_files(root: &Path) -> Result<Vec<PathBuf>, FixtureError> {
	let mut out = Vec::new();
	for entry in std::fs::read_dir(root).map_err(FixtureError::TestIo)? {
		let entry = entry.map_err(FixtureError::TestIo)?;
		let path = entry.path();
		let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
			continue;
		};
		if name.ends_with(".flowgraph.test.toml") {
			out.push(path);
		}
	}
	out.sort();
	Ok(out)
}

fn run_declared_tests(root: &Path, report: &FixtureRunReport) -> Result<Vec<FixtureTestResult>, FixtureError> {
	let mut results = Vec::new();
	for path in discover_test_files(root)? {
		let raw = std::fs::read_to_string(&path).map_err(FixtureError::TestIo)?;
		let parsed: FixtureTestFile = toml::from_str(&raw).map_err(|error| FixtureError::TestParse {
			path: path.clone(),
			error,
		})?;
		for (index, case) in parsed.tests.iter().enumerate() {
			results.push(evaluate_test_case(&path, index, case, report));
		}
	}
	Ok(results)
}

fn evaluate_test_case(path: &Path, index: usize, case: &FixtureTestCase, report: &FixtureRunReport) -> FixtureTestResult {
	let mut failures = Vec::new();
	if let Some(expected) = case.expect.node_count {
		if report.node_count != expected {
			failures.push(format!("node_count: expected {expected}, actual {}", report.node_count));
		}
	}
	if let Some(expected) = case.expect.trace_count {
		if report.trace_count != expected {
			failures.push(format!("trace_count: expected {expected}, actual {}", report.trace_count));
		}
	}
	if let Some(expected) = &case.expect.trace {
		if &report.trace != expected {
			failures.push(format!("trace: expected {expected:?}, actual {:?}", report.trace));
		}
	}
	FixtureTestResult {
		file: path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string(),
		name: case.name.clone().unwrap_or_else(|| format!("test#{index}")),
		ok: failures.is_empty(),
		failures,
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use std::path::PathBuf;

	fn example_dir(name: &str) -> PathBuf {
		PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("flowgraph.example").join(name)
	}

	#[tokio::test]
	async fn lambda_demo_loads() {
		let dir = example_dir("lambda-demo");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}

	#[tokio::test]
	async fn lambda_demo_execute_smoke() {
		let dir = example_dir("lambda-demo");
		let run = load_and_execute_once(&dir).await.expect("execute");
		// ソースが無い／pull のみのグラフでも完走すれば generation が進む
		assert!(run.generation > 0);
	}

	#[tokio::test]
	async fn lambda_demo_report_has_trace_shape() {
		let dir = example_dir("lambda-demo");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok);
		assert!(report.generation > 0);
		assert!(report.node_count > 0);
		assert_eq!(report.trace_count, report.trace.len());
	}

	#[tokio::test]
	async fn lambda_demo_declared_tests_pass() {
		let dir = example_dir("lambda-demo");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert!(!report.tests.is_empty());
		assert_eq!(report.failed_tests, 0);
	}

	#[tokio::test]
	async fn osc_udp_ingress_example_loads() {
		let dir = example_dir("osc-udp-ingress");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}

	#[tokio::test]
	async fn vmc_blendshape_trigger_example_loads() {
		let dir = example_dir("vmc-blendshape-trigger");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}

	#[tokio::test]
	async fn vmc_ai_mode_control_example_loads() {
		let dir = example_dir("vmc-ai-mode-control");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}

	#[tokio::test]
	async fn vmc_obs_scene_control_example_loads() {
		let dir = example_dir("vmc-obs-scene-control");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}

	#[tokio::test]
	async fn http_webhook_example_loads() {
		let dir = example_dir("http-webhook");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}

	#[tokio::test]
	async fn system_monitor_example_loads() {
		let dir = example_dir("system-monitor");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}

	#[tokio::test]
	async fn twitch_stream_tools_example_loads() {
		let dir = example_dir("twitch-stream-tools");
		let ok = load_fixture_program(&dir).expect("load");
		let _ = ok.program;
	}
}
