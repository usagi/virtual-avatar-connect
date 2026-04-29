//! Flowgraph **fixture ランナー**の最小実装（language roadmap P2 / PR4 先取り）。
//!
//! `load_flowgraph_dir` でディレクトリをロードし、可能なら 1-shot [`FlowgraphProgram::execute`] を走らせる。
//! 外部 IO・ingress・`state_handle` 必須ノードを含むグラフは失敗しうるため、CI 用の厳密 runner ではなく
//! **ロード＋純粋グラフのスモーク**向け。

use crate::flowgraph::engine::create_trigger_bus;
use crate::flowgraph::node::{
	EffectMocks, ExecCtx, FileReadMockResponse, FileWriteMockResponse, HttpMockResponse, SocketValueRepr, TriggerEvent,
};
use crate::flowgraph::socket::{from_toml_value, SocketType};
use crate::flowgraph::{load_flowgraph_dir, FlowgraphProgram, LoadError, NodeExecError, ProgramRun};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

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
	TestParse {
		path: PathBuf,
		error: toml::de::Error,
	},
	NoFixtureTestFiles {
		root: PathBuf,
	},
	TriggerValue {
		path: PathBuf,
		trigger: usize,
		port: String,
		reason: String,
	},
	TriggerSend {
		node: String,
		reason: String,
	},
}

impl std::fmt::Display for FixtureError {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			FixtureError::Load(e) => write!(f, "load failed: {e}"),
			FixtureError::Execute(e) => write!(f, "execute failed: {e}"),
			FixtureError::TestIo(e) => write!(f, "test file io failed: {e}"),
			FixtureError::TestParse { path, error } => write!(f, "test file parse failed: {}: {error}", path.display()),
			FixtureError::NoFixtureTestFiles { root } => {
				write!(f, "no *.flowgraph.test.toml files found: {}", root.display())
			}
			FixtureError::TriggerValue {
				path,
				trigger,
				port,
				reason,
			} => {
				write!(
					f,
					"trigger value parse failed: {} trigger#{trigger} port={port}: {reason}",
					path.display()
				)
			}
			FixtureError::TriggerSend { node, reason } => write!(f, "trigger send failed: node={node}: {reason}"),
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
pub struct FixtureRecordedEffect {
	pub kind: String,
	pub node: String,
	pub method: Option<String>,
	pub url: Option<String>,
	pub path: Option<String>,
	pub status: Option<i64>,
	pub bytes: Option<i64>,
	pub contents: Option<String>,
	pub request_body: Option<serde_json::Value>,
	pub response_body: Option<String>,
	pub error: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct FixtureRunReport {
	pub ok: bool,
	pub root: String,
	pub generation: u64,
	pub node_count: usize,
	pub mock_count: usize,
	pub mocks: Vec<FixtureMockSummary>,
	pub trigger_count: usize,
	pub trigger_history: Vec<FixtureTriggerHistory>,
	pub trace: Vec<String>,
	pub trace_count: usize,
	pub effect_count: usize,
	pub recorded_effects: Vec<FixtureRecordedEffect>,
	pub stored_values: Vec<FixtureTraceValue>,
	pub exec_count: Vec<(String, usize)>,
	pub pure_evaluations: Vec<(String, usize)>,
	pub cache_hits: usize,
	pub cache_misses: usize,
	pub tests: Vec<FixtureTestResult>,
	pub failed_tests: usize,
}

#[derive(Debug, Serialize)]
pub struct FixtureSuiteReport {
	pub ok: bool,
	pub root: String,
	pub fixture_count: usize,
	pub failed_fixtures: usize,
	pub reports: Vec<FixtureRunReport>,
	pub errors: Vec<FixtureSuiteError>,
}

#[derive(Debug, Serialize)]
pub struct FixtureSuiteError {
	pub root: String,
	pub error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureTriggerHistory {
	pub node: String,
	pub exec: Vec<String>,
	pub delay_ms: u64,
	pub overrides: Vec<FixtureTriggerHistoryOverride>,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureTriggerHistoryOverride {
	pub port: String,
	pub ty: String,
	pub value: serde_json::Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct FixtureMockSummary {
	pub kind: String,
	pub node: String,
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
	mocks: FixtureMockSpec,
	#[serde(default)]
	triggers: Vec<FixtureTriggerSpec>,
	#[serde(default)]
	tests: Vec<FixtureTestCase>,
}

struct ParsedFixtureTestFile {
	path: PathBuf,
	parsed: FixtureTestFile,
}

#[derive(Debug, Deserialize)]
struct FixtureTriggerSpec {
	node: String,
	#[serde(default = "default_trigger_exec")]
	exec: Vec<String>,
	#[serde(default)]
	delay_ms: u64,
	#[serde(default)]
	overrides: Vec<FixtureTriggerOverride>,
}

#[derive(Debug, Deserialize)]
struct FixtureTriggerOverride {
	port: String,
	ty: SocketType,
	value: toml::Value,
}

#[derive(Debug, Default, Deserialize)]
struct FixtureMockSpec {
	#[serde(default)]
	http: Vec<FixtureHttpMockSpec>,
	#[serde(default)]
	file_read: Vec<FixtureFileReadMockSpec>,
	#[serde(default)]
	file_write: Vec<FixtureFileWriteMockSpec>,
}

#[derive(Debug, Deserialize)]
struct FixtureHttpMockSpec {
	node: String,
	#[serde(default = "default_http_mock_status")]
	status: i64,
	#[serde(default)]
	body: String,
	#[serde(default)]
	json: Option<toml::Value>,
	#[serde(default)]
	error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureFileReadMockSpec {
	node: String,
	#[serde(default)]
	contents: String,
	#[serde(default)]
	error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureFileWriteMockSpec {
	node: String,
	#[serde(default)]
	error: Option<String>,
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
	mock_count: Option<usize>,
	trigger_count: Option<usize>,
	trace_count: Option<usize>,
	trace: Option<Vec<String>>,
	effect_count: Option<usize>,
	#[serde(default)]
	trigger_history: Vec<FixtureExpectedTriggerHistory>,
	#[serde(default)]
	exec_count: Vec<FixtureExpectedCount>,
	#[serde(default)]
	stored_values: Vec<FixtureExpectedValue>,
	#[serde(default)]
	file_reads: Vec<FixtureExpectedFileRead>,
	#[serde(default)]
	file_writes: Vec<FixtureExpectedFileWrite>,
	#[serde(default)]
	http_requests: Vec<FixtureExpectedHttpRequest>,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectedTriggerHistory {
	node: String,
	#[serde(default)]
	exec: Option<Vec<String>>,
	#[serde(default)]
	delay_ms: Option<u64>,
	#[serde(default)]
	overrides: Vec<FixtureExpectedTriggerOverride>,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectedTriggerOverride {
	port: String,
	#[serde(default)]
	ty: Option<String>,
	value: toml::Value,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectedCount {
	node: String,
	count: usize,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectedValue {
	node: String,
	port: String,
	#[serde(default)]
	ty: Option<String>,
	value: toml::Value,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectedFileRead {
	node: String,
	path: String,
	#[serde(default)]
	bytes: Option<i64>,
	#[serde(default)]
	contents: Option<String>,
	#[serde(default)]
	error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectedFileWrite {
	node: String,
	path: String,
	#[serde(default)]
	bytes: Option<i64>,
	#[serde(default)]
	contents: Option<String>,
	#[serde(default)]
	error: Option<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureExpectedHttpRequest {
	node: String,
	method: String,
	url: String,
	#[serde(default)]
	status: Option<i64>,
	#[serde(default)]
	request_body: Option<toml::Value>,
	#[serde(default)]
	response_body: Option<String>,
	#[serde(default)]
	error: Option<String>,
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
	let declared_tests = read_declared_tests(root)?;
	let (run, ctx, mock_summaries, trigger_history) = run_fixture_program(&mut fixture.program, &declared_tests).await?;
	let mut report = make_report(root, node_count, run, ctx, mock_summaries, trigger_history);
	report.tests = evaluate_declared_tests(&declared_tests, &report);
	report.failed_tests = report.tests.iter().filter(|t| !t.ok).count();
	report.ok = report.failed_tests == 0;
	Ok(report)
}

pub async fn run_fixture_suite_report(root: &Path) -> Result<FixtureSuiteReport, FixtureError> {
	let fixture_roots = discover_fixture_roots(root)?;
	if fixture_roots.is_empty() {
		return Err(FixtureError::NoFixtureTestFiles { root: root.to_path_buf() });
	}
	let mut reports = Vec::new();
	let mut errors = Vec::new();
	for fixture_root in fixture_roots {
		match run_fixture_once_report(&fixture_root).await {
			Ok(report) => reports.push(report),
			Err(error) => errors.push(FixtureSuiteError {
				root: fixture_root.display().to_string(),
				error: error.to_string(),
			}),
		}
	}
	let failed_reports = reports.iter().filter(|report| !report.ok).count();
	let failed_fixtures = failed_reports + errors.len();
	Ok(FixtureSuiteReport {
		ok: failed_fixtures == 0,
		root: root.display().to_string(),
		fixture_count: reports.len() + errors.len(),
		failed_fixtures,
		reports,
		errors,
	})
}

async fn run_fixture_program(
	program: &mut FlowgraphProgram,
	declared_tests: &[ParsedFixtureTestFile],
) -> Result<(ProgramRun, ExecCtx, Vec<FixtureMockSummary>, Vec<FixtureTriggerHistory>), FixtureError> {
	let trigger_events = build_trigger_events(declared_tests)?;
	let trigger_history = trigger_events.iter().map(|trigger| trigger.history.clone()).collect();
	let mut ctx = ExecCtx::default();
	let effect_mocks = build_effect_mocks(declared_tests);
	let mock_summaries = summarize_effect_mocks(&effect_mocks);
	if !effect_mocks.is_empty() {
		ctx.effect_mocks = Some(Arc::new(effect_mocks));
	}
	if trigger_events.is_empty() {
		let run = program.execute(&mut ctx).await.map_err(FixtureError::Execute)?;
		return Ok((run, ctx, mock_summaries, trigger_history));
	}

	let total_delay_ms = trigger_events
		.iter()
		.fold(0_u64, |sum, trigger| sum.saturating_add(trigger.delay_ms));
	let shutdown = tokio::time::sleep(Duration::from_millis(total_delay_ms.saturating_add(150)));
	let (handle, rx) = create_trigger_bus();
	let sender = handle.clone();
	let trigger_task = tokio::spawn(async move {
		for trigger in trigger_events {
			if trigger.delay_ms > 0 {
				tokio::time::sleep(Duration::from_millis(trigger.delay_ms)).await;
			}
			sender.send(trigger.event).map_err(|e| FixtureError::TriggerSend {
				node: trigger.node,
				reason: e.to_string(),
			})?;
		}
		Ok::<(), FixtureError>(())
	});

	let run = program
		.run_forever_with_bus(&mut ctx, handle, rx, shutdown, None)
		.await
		.map_err(FixtureError::Execute)?;
	trigger_task.await.map_err(|e| FixtureError::TriggerSend {
		node: "<trigger-task>".into(),
		reason: e.to_string(),
	})??;
	Ok((run, ctx, mock_summaries, trigger_history))
}

fn build_effect_mocks(declared_tests: &[ParsedFixtureTestFile]) -> EffectMocks {
	let mut http = HashMap::new();
	let mut file_read = HashMap::new();
	let mut file_write = HashMap::new();
	for file in declared_tests {
		for mock in &file.parsed.mocks.http {
			let body_json = mock
				.json
				.as_ref()
				.map(toml_value_to_json)
				.unwrap_or_else(|| serde_json::from_str(&mock.body).unwrap_or(serde_json::Value::Null));
			http.insert(
				mock.node.clone(),
				HttpMockResponse {
					status: mock.status,
					body_text: mock.body.clone(),
					body_json,
					error: mock.error.clone(),
				},
			);
		}
		for mock in &file.parsed.mocks.file_read {
			file_read.insert(
				mock.node.clone(),
				FileReadMockResponse {
					contents: mock.contents.clone(),
					error: mock.error.clone(),
				},
			);
		}
		for mock in &file.parsed.mocks.file_write {
			file_write.insert(mock.node.clone(), FileWriteMockResponse { error: mock.error.clone() });
		}
	}
	EffectMocks {
		http,
		file_read,
		file_write,
	}
}

fn summarize_effect_mocks(effect_mocks: &EffectMocks) -> Vec<FixtureMockSummary> {
	let mut out: Vec<FixtureMockSummary> = effect_mocks
		.http
		.keys()
		.map(|node| FixtureMockSummary {
			kind: "http".into(),
			node: node.clone(),
		})
		.collect();
	out.extend(effect_mocks.file_read.keys().map(|node| FixtureMockSummary {
		kind: "file_read".into(),
		node: node.clone(),
	}));
	out.extend(effect_mocks.file_write.keys().map(|node| FixtureMockSummary {
		kind: "file_write".into(),
		node: node.clone(),
	}));
	out.sort_by(|a, b| (&a.kind, &a.node).cmp(&(&b.kind, &b.node)));
	out
}

struct FixtureTriggerEvent {
	node: String,
	delay_ms: u64,
	event: TriggerEvent,
	history: FixtureTriggerHistory,
}

fn build_trigger_events(declared_tests: &[ParsedFixtureTestFile]) -> Result<Vec<FixtureTriggerEvent>, FixtureError> {
	let mut events = Vec::new();
	for file in declared_tests {
		for (index, trigger) in file.parsed.triggers.iter().enumerate() {
			let mut event = TriggerEvent::new(&trigger.node);
			for exec in &trigger.exec {
				event = event.with_exec(exec);
			}
			for override_value in &trigger.overrides {
				let value = from_toml_value(&override_value.ty, &override_value.value).map_err(|e| FixtureError::TriggerValue {
					path: file.path.clone(),
					trigger: index,
					port: override_value.port.clone(),
					reason: e.to_string(),
				})?;
				event = event.with_override(&override_value.port, value);
			}
			let history = trigger_history_from_event(trigger.delay_ms, &event);
			events.push(FixtureTriggerEvent {
				node: trigger.node.clone(),
				delay_ms: trigger.delay_ms,
				event,
				history,
			});
		}
	}
	Ok(events)
}

fn trigger_history_from_event(delay_ms: u64, event: &TriggerEvent) -> FixtureTriggerHistory {
	let mut overrides: Vec<FixtureTriggerHistoryOverride> = event
		.data_overrides
		.iter()
		.map(|(port, value)| FixtureTriggerHistoryOverride {
			port: port.clone(),
			ty: value.type_of().to_string(),
			value: SocketValueRepr::from_value(value).0,
		})
		.collect();
	overrides.sort_by(|a, b| a.port.cmp(&b.port));
	FixtureTriggerHistory {
		node: event.node_id.clone(),
		exec: event.fired_exec.clone(),
		delay_ms,
		overrides,
	}
}

fn make_report(
	root: &Path,
	node_count: usize,
	run: ProgramRun,
	ctx: ExecCtx,
	mocks: Vec<FixtureMockSummary>,
	trigger_history: Vec<FixtureTriggerHistory>,
) -> FixtureRunReport {
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

	let mut recorded_effects: Vec<FixtureRecordedEffect> = ctx
		.recorded_effects
		.iter()
		.map(|effect| FixtureRecordedEffect {
			kind: effect.kind.clone(),
			node: effect.node.clone(),
			method: effect.method.clone(),
			url: effect.url.clone(),
			path: effect.path.clone(),
			status: effect.status,
			bytes: effect.bytes,
			contents: effect.contents.clone(),
			request_body: effect.request_body.clone(),
			response_body: effect.response_body.clone(),
			error: effect.error.clone(),
		})
		.collect();
	recorded_effects.sort_by(|a, b| (&a.kind, &a.node, &a.path).cmp(&(&b.kind, &b.node, &b.path)));

	FixtureRunReport {
		ok: true,
		root: root.display().to_string(),
		generation: run.generation,
		node_count,
		mock_count: mocks.len(),
		mocks,
		trigger_count: trigger_history.len(),
		trigger_history,
		trace_count: ctx.trace.len(),
		trace: ctx.trace,
		effect_count: recorded_effects.len(),
		recorded_effects,
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

fn discover_fixture_roots(root: &Path) -> Result<Vec<PathBuf>, FixtureError> {
	let mut out = Vec::new();
	discover_fixture_roots_inner(root, &mut out)?;
	out.sort();
	Ok(out)
}

fn discover_fixture_roots_inner(root: &Path, out: &mut Vec<PathBuf>) -> Result<(), FixtureError> {
	let entries = std::fs::read_dir(root).map_err(FixtureError::TestIo)?;
	let mut children = Vec::new();
	let mut has_test_file = false;
	for entry in entries {
		let entry = entry.map_err(FixtureError::TestIo)?;
		let path = entry.path();
		let Some(name) = path.file_name().and_then(|s| s.to_str()) else {
			continue;
		};
		if path.is_file() && name.ends_with(".flowgraph.test.toml") {
			has_test_file = true;
		} else if path.is_dir() && !matches!(name, ".git" | "target" | "node_modules") {
			children.push(path);
		}
	}
	if has_test_file {
		out.push(root.to_path_buf());
		return Ok(());
	}
	children.sort();
	for child in children {
		discover_fixture_roots_inner(&child, out)?;
	}
	Ok(())
}

fn read_declared_tests(root: &Path) -> Result<Vec<ParsedFixtureTestFile>, FixtureError> {
	let mut results = Vec::new();
	for path in discover_test_files(root)? {
		let raw = std::fs::read_to_string(&path).map_err(FixtureError::TestIo)?;
		let parsed: FixtureTestFile = toml::from_str(&raw).map_err(|error| FixtureError::TestParse { path: path.clone(), error })?;
		results.push(ParsedFixtureTestFile { path, parsed });
	}
	if results.is_empty() {
		return Err(FixtureError::NoFixtureTestFiles { root: root.to_path_buf() });
	}
	Ok(results)
}

fn evaluate_declared_tests(declared_tests: &[ParsedFixtureTestFile], report: &FixtureRunReport) -> Vec<FixtureTestResult> {
	let mut results = Vec::new();
	for file in declared_tests {
		for (index, case) in file.parsed.tests.iter().enumerate() {
			results.push(evaluate_test_case(&file.path, index, case, report));
		}
	}
	results
}

fn default_trigger_exec() -> Vec<String> {
	vec!["__trigger__".into()]
}

fn default_http_mock_status() -> i64 {
	200
}

fn evaluate_test_case(path: &Path, index: usize, case: &FixtureTestCase, report: &FixtureRunReport) -> FixtureTestResult {
	let mut failures = Vec::new();
	if let Some(expected) = case.expect.node_count {
		if report.node_count != expected {
			failures.push(format!("node_count: expected {expected}, actual {}", report.node_count));
		}
	}
	if let Some(expected) = case.expect.mock_count {
		if report.mock_count != expected {
			failures.push(format!("mock_count: expected {expected}, actual {}", report.mock_count));
		}
	}
	if let Some(expected) = case.expect.trigger_count {
		if report.trigger_count != expected {
			failures.push(format!("trigger_count: expected {expected}, actual {}", report.trigger_count));
		}
	}
	if let Some(expected) = case.expect.trace_count {
		if report.trace_count != expected {
			failures.push(format!("trace_count: expected {expected}, actual {}", report.trace_count));
		}
	}
	if let Some(expected) = case.expect.effect_count {
		if report.effect_count != expected {
			failures.push(format!("effect_count: expected {expected}, actual {}", report.effect_count));
		}
	}
	if let Some(expected) = &case.expect.trace {
		if &report.trace != expected {
			failures.push(format!("trace: expected {expected:?}, actual {:?}", report.trace));
		}
	}
	for expected in &case.expect.trigger_history {
		match report.trigger_history.iter().find(|actual| actual.node == expected.node) {
			Some(actual) => assert_trigger_history(&mut failures, expected, actual),
			None => failures.push(format!("trigger_history {}: missing", expected.node)),
		}
	}
	for expected in &case.expect.exec_count {
		let actual = report
			.exec_count
			.iter()
			.find(|(node, _)| node == &expected.node)
			.map(|(_, count)| *count)
			.unwrap_or(0);
		if actual != expected.count {
			failures.push(format!(
				"exec_count {}: expected {}, actual {}",
				expected.node, expected.count, actual
			));
		}
	}
	for expected in &case.expect.stored_values {
		match report
			.stored_values
			.iter()
			.find(|actual| actual.node == expected.node && actual.port == expected.port)
		{
			Some(actual) => {
				if let Some(expected_ty) = &expected.ty {
					if &actual.ty != expected_ty {
						failures.push(format!(
							"stored_value {}:{} type: expected {}, actual {}",
							expected.node, expected.port, expected_ty, actual.ty
						));
					}
				}
				let expected_value = toml_value_to_json(&expected.value);
				if actual.value != expected_value {
					failures.push(format!(
						"stored_value {}:{} value: expected {}, actual {}",
						expected.node,
						expected.port,
						compact_json(&expected_value),
						compact_json(&actual.value)
					));
				}
			}
			None => failures.push(format!("stored_value {}:{}: missing", expected.node, expected.port)),
		}
	}
	for expected in &case.expect.file_reads {
		match find_recorded_path_effect(report, "file_read", &expected.node, &expected.path) {
			Some(actual) => assert_file_effect(
				&mut failures,
				"file_read",
				&expected.node,
				&expected.path,
				actual,
				expected.bytes,
				expected.contents.as_deref(),
				expected.error.as_deref(),
			),
			None => failures.push(format!("file_read {}:{}: missing", expected.node, expected.path)),
		}
	}
	for expected in &case.expect.file_writes {
		match find_recorded_path_effect(report, "file_write", &expected.node, &expected.path) {
			Some(actual) => assert_file_effect(
				&mut failures,
				"file_write",
				&expected.node,
				&expected.path,
				actual,
				expected.bytes,
				expected.contents.as_deref(),
				expected.error.as_deref(),
			),
			None => failures.push(format!("file_write {}:{}: missing", expected.node, expected.path)),
		}
	}
	for expected in &case.expect.http_requests {
		match find_recorded_http_effect(report, &expected.node, &expected.method, &expected.url) {
			Some(actual) => assert_http_effect(&mut failures, expected, actual),
			None => failures.push(format!(
				"http_request {}:{} {}: missing",
				expected.node, expected.method, expected.url
			)),
		}
	}
	FixtureTestResult {
		file: path.file_name().and_then(|s| s.to_str()).unwrap_or("").to_string(),
		name: case.name.clone().unwrap_or_else(|| format!("test#{index}")),
		ok: failures.is_empty(),
		failures,
	}
}

fn find_recorded_path_effect<'a>(report: &'a FixtureRunReport, kind: &str, node: &str, path: &str) -> Option<&'a FixtureRecordedEffect> {
	report
		.recorded_effects
		.iter()
		.find(|actual| actual.kind == kind && actual.node == node && actual.path.as_deref() == Some(path))
}

fn find_recorded_http_effect<'a>(report: &'a FixtureRunReport, node: &str, method: &str, url: &str) -> Option<&'a FixtureRecordedEffect> {
	report.recorded_effects.iter().find(|actual| {
		actual.kind == "http" && actual.node == node && actual.method.as_deref() == Some(method) && actual.url.as_deref() == Some(url)
	})
}

fn assert_file_effect(
	failures: &mut Vec<String>,
	kind: &str,
	node: &str,
	path: &str,
	actual: &FixtureRecordedEffect,
	expected_bytes: Option<i64>,
	expected_contents: Option<&str>,
	expected_error: Option<&str>,
) {
	if let Some(expected_bytes) = expected_bytes {
		if actual.bytes != Some(expected_bytes) {
			failures.push(format!(
				"{kind} {node}:{path} bytes: expected {expected_bytes}, actual {:?}",
				actual.bytes
			));
		}
	}
	if let Some(expected_contents) = expected_contents {
		if actual.contents.as_deref() != Some(expected_contents) {
			failures.push(format!(
				"{kind} {node}:{path} contents: expected {:?}, actual {:?}",
				expected_contents, actual.contents
			));
		}
	}
	if actual.error.as_deref() != expected_error {
		failures.push(format!(
			"{kind} {node}:{path} error: expected {:?}, actual {:?}",
			expected_error, actual.error
		));
	}
}

fn assert_trigger_history(failures: &mut Vec<String>, expected: &FixtureExpectedTriggerHistory, actual: &FixtureTriggerHistory) {
	if let Some(expected_exec) = &expected.exec {
		if &actual.exec != expected_exec {
			failures.push(format!(
				"trigger_history {} exec: expected {:?}, actual {:?}",
				expected.node, expected_exec, actual.exec
			));
		}
	}
	if let Some(expected_delay_ms) = expected.delay_ms {
		if actual.delay_ms != expected_delay_ms {
			failures.push(format!(
				"trigger_history {} delay_ms: expected {}, actual {}",
				expected.node, expected_delay_ms, actual.delay_ms
			));
		}
	}
	for expected_override in &expected.overrides {
		match actual.overrides.iter().find(|actual| actual.port == expected_override.port) {
			Some(actual_override) => {
				if let Some(expected_ty) = &expected_override.ty {
					if &actual_override.ty != expected_ty {
						failures.push(format!(
							"trigger_history {} override {} type: expected {}, actual {}",
							expected.node, expected_override.port, expected_ty, actual_override.ty
						));
					}
				}
				let expected_value = toml_value_to_json(&expected_override.value);
				if actual_override.value != expected_value {
					failures.push(format!(
						"trigger_history {} override {} value: expected {}, actual {}",
						expected.node,
						expected_override.port,
						compact_json(&expected_value),
						compact_json(&actual_override.value)
					));
				}
			}
			None => failures.push(format!(
				"trigger_history {} override {}: missing",
				expected.node, expected_override.port
			)),
		}
	}
}

fn assert_http_effect(failures: &mut Vec<String>, expected: &FixtureExpectedHttpRequest, actual: &FixtureRecordedEffect) {
	if actual.status != expected.status {
		failures.push(format!(
			"http_request {}:{} {} status: expected {:?}, actual {:?}",
			expected.node, expected.method, expected.url, expected.status, actual.status
		));
	}
	if let Some(expected_body) = &expected.request_body {
		let expected_json = toml_value_to_json(expected_body);
		if actual.request_body.as_ref() != Some(&expected_json) {
			failures.push(format!(
				"http_request {}:{} {} request_body: expected {}, actual {:?}",
				expected.node,
				expected.method,
				expected.url,
				compact_json(&expected_json),
				actual.request_body
			));
		}
	}
	if actual.response_body != expected.response_body {
		failures.push(format!(
			"http_request {}:{} {} response_body: expected {:?}, actual {:?}",
			expected.node, expected.method, expected.url, expected.response_body, actual.response_body
		));
	}
	if actual.error != expected.error {
		failures.push(format!(
			"http_request {}:{} {} error: expected {:?}, actual {:?}",
			expected.node, expected.method, expected.url, expected.error, actual.error
		));
	}
}

fn toml_value_to_json(value: &toml::Value) -> serde_json::Value {
	match value {
		toml::Value::String(v) => serde_json::Value::String(v.clone()),
		toml::Value::Integer(v) => serde_json::Value::Number((*v).into()),
		toml::Value::Float(v) => serde_json::Number::from_f64(*v)
			.map(serde_json::Value::Number)
			.unwrap_or(serde_json::Value::Null),
		toml::Value::Boolean(v) => serde_json::Value::Bool(*v),
		toml::Value::Datetime(v) => serde_json::Value::String(v.to_string()),
		toml::Value::Array(values) => serde_json::Value::Array(values.iter().map(toml_value_to_json).collect()),
		toml::Value::Table(values) => {
			serde_json::Value::Object(values.iter().map(|(key, value)| (key.clone(), toml_value_to_json(value))).collect())
		}
	}
}

fn compact_json(value: &serde_json::Value) -> String {
	serde_json::to_string(value).unwrap_or_else(|_| format!("{value:?}"))
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
	async fn twitch_echo_declared_trigger_test_passes() {
		let dir = example_dir("twitch-echo");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert_eq!(report.trace, vec!["log: hello fixture"]);
		assert_eq!(report.failed_tests, 0);
	}

	#[tokio::test]
	async fn http_webhook_declared_mock_test_passes() {
		let dir = example_dir("http-webhook");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert_eq!(report.mock_count, 1);
		assert!(report.trace.iter().any(|line| line.contains("http.request mock: POST")));
		assert_eq!(report.failed_tests, 0);
	}

	#[tokio::test]
	async fn http_webhook_error_declared_mock_test_passes() {
		let dir = example_dir("http-webhook-error");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert!(report.trace.iter().any(|line| line.contains("fixture network down")));
		assert_eq!(report.failed_tests, 0);
	}

	#[tokio::test]
	async fn example_fixture_suite_passes() {
		let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("flowgraph.example");
		let report = run_fixture_suite_report(&dir).await.expect("report");
		assert!(report.ok, "errors: {:?}", report.errors);
		assert_eq!(report.fixture_count, 8);
		assert_eq!(report.failed_fixtures, 0);
	}

	#[tokio::test]
	async fn table_load_tsv_mock_declared_test_passes() {
		let dir = example_dir("table-load-tsv-mock");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert_eq!(report.mock_count, 1);
		assert!(report.trace.iter().any(|line| line.contains("file.read mock: fixture.tsv")));
		assert_eq!(report.failed_tests, 0);
	}

	#[tokio::test]
	async fn table_load_tsv_mock_error_declared_test_passes() {
		let dir = example_dir("table-load-tsv-mock-error");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert_eq!(report.mock_count, 1);
		assert!(report.trace.iter().any(|line| line.contains("fixture missing file")));
		assert_eq!(report.failed_tests, 0);
	}

	#[tokio::test]
	async fn table_write_tsv_mock_declared_test_passes() {
		let dir = example_dir("table-write-tsv-mock");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert_eq!(report.mock_count, 2);
		assert!(report.trace.iter().any(|line| line.contains("file.write mock: out.tsv")));
		assert_eq!(report.failed_tests, 0);
	}

	#[tokio::test]
	async fn table_write_tsv_mock_error_declared_test_passes() {
		let dir = example_dir("table-write-tsv-mock-error");
		let report = run_fixture_once_report(&dir).await.expect("report");
		assert!(report.ok, "report: {:?}", report.tests);
		assert_eq!(report.mock_count, 2);
		assert!(report.trace.iter().any(|line| line.contains("fixture disk full")));
		assert_eq!(report.failed_tests, 0);
	}

	#[test]
	fn stored_value_assertion_passes() {
		let report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		let case = FixtureTestCase {
			name: Some("stored".into()),
			expect: FixtureExpect {
				stored_values: vec![FixtureExpectedValue {
					node: "n".into(),
					port: "out".into(),
					ty: Some("string".into()),
					value: toml::Value::String("ok".into()),
				}],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(result.ok, "{:?}", result.failures);
	}

	#[test]
	fn stored_value_assertion_reports_missing_type_and_value() {
		let report = report_with_stored_value("n", "out", "string", serde_json::json!("actual"));
		let case = FixtureTestCase {
			name: Some("stored".into()),
			expect: FixtureExpect {
				stored_values: vec![
					FixtureExpectedValue {
						node: "n".into(),
						port: "out".into(),
						ty: Some("int".into()),
						value: toml::Value::String("expected".into()),
					},
					FixtureExpectedValue {
						node: "missing".into(),
						port: "value".into(),
						ty: None,
						value: toml::Value::Integer(1),
					},
				],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(!result.ok);
		assert_eq!(result.failures.len(), 3);
		assert!(result.failures.iter().any(|f| f.contains("type: expected int")));
		assert!(result
			.failures
			.iter()
			.any(|f| f.contains("value: expected \"expected\", actual \"actual\"")));
		assert!(result.failures.iter().any(|f| f == "stored_value missing:value: missing"));
	}

	#[test]
	fn trigger_history_assertion_passes() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.trigger_count = 1;
		report.trigger_history.push(FixtureTriggerHistory {
			node: "in".into(),
			exec: vec!["__trigger__".into()],
			delay_ms: 10,
			overrides: vec![FixtureTriggerHistoryOverride {
				port: "text".into(),
				ty: "string".into(),
				value: serde_json::json!("hello"),
			}],
		});
		let case = FixtureTestCase {
			name: Some("trigger".into()),
			expect: FixtureExpect {
				trigger_count: Some(1),
				trigger_history: vec![FixtureExpectedTriggerHistory {
					node: "in".into(),
					exec: Some(vec!["__trigger__".into()]),
					delay_ms: Some(10),
					overrides: vec![FixtureExpectedTriggerOverride {
						port: "text".into(),
						ty: Some("string".into()),
						value: toml::Value::String("hello".into()),
					}],
				}],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(result.ok, "{:?}", result.failures);
	}

	#[test]
	fn trigger_history_assertion_reports_missing_and_mismatch() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.trigger_count = 1;
		report.trigger_history.push(FixtureTriggerHistory {
			node: "in".into(),
			exec: vec!["custom".into()],
			delay_ms: 10,
			overrides: vec![FixtureTriggerHistoryOverride {
				port: "text".into(),
				ty: "string".into(),
				value: serde_json::json!("actual"),
			}],
		});
		let case = FixtureTestCase {
			name: Some("trigger".into()),
			expect: FixtureExpect {
				trigger_history: vec![
					FixtureExpectedTriggerHistory {
						node: "in".into(),
						exec: Some(vec!["__trigger__".into()]),
						delay_ms: Some(20),
						overrides: vec![
							FixtureExpectedTriggerOverride {
								port: "text".into(),
								ty: Some("int".into()),
								value: toml::Value::String("expected".into()),
							},
							FixtureExpectedTriggerOverride {
								port: "missing".into(),
								ty: None,
								value: toml::Value::Integer(1),
							},
						],
					},
					FixtureExpectedTriggerHistory {
						node: "missing".into(),
						exec: None,
						delay_ms: None,
						overrides: Vec::new(),
					},
				],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(!result.ok);
		assert_eq!(result.failures.len(), 6);
		assert!(result.failures.iter().any(|f| f.contains("trigger_history in exec")));
		assert!(result.failures.iter().any(|f| f.contains("trigger_history in delay_ms")));
		assert!(result.failures.iter().any(|f| f.contains("trigger_history in override text type")));
		assert!(result.failures.iter().any(|f| f.contains("trigger_history in override text value")));
		assert!(result.failures.iter().any(|f| f == "trigger_history in override missing: missing"));
		assert!(result.failures.iter().any(|f| f == "trigger_history missing: missing"));
	}

	#[test]
	fn file_read_assertion_passes() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.effect_count = 1;
		report.recorded_effects.push(FixtureRecordedEffect {
			kind: "file_read".into(),
			node: "load".into(),
			method: None,
			url: None,
			path: Some("input.tsv".into()),
			status: None,
			bytes: Some(8),
			contents: Some("a\tb\n1\t2\n".into()),
			request_body: None,
			response_body: None,
			error: None,
		});
		let case = FixtureTestCase {
			name: Some("read".into()),
			expect: FixtureExpect {
				effect_count: Some(1),
				file_reads: vec![FixtureExpectedFileRead {
					node: "load".into(),
					path: "input.tsv".into(),
					bytes: Some(8),
					contents: Some("a\tb\n1\t2\n".into()),
					error: None,
				}],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(result.ok, "{:?}", result.failures);
	}

	#[test]
	fn file_read_assertion_reports_missing_and_mismatch() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.effect_count = 1;
		report.recorded_effects.push(FixtureRecordedEffect {
			kind: "file_read".into(),
			node: "load".into(),
			method: None,
			url: None,
			path: Some("input.tsv".into()),
			status: None,
			bytes: Some(8),
			contents: Some("actual".into()),
			request_body: None,
			response_body: None,
			error: Some("missing".into()),
		});
		let case = FixtureTestCase {
			name: Some("read".into()),
			expect: FixtureExpect {
				effect_count: Some(2),
				file_reads: vec![
					FixtureExpectedFileRead {
						node: "load".into(),
						path: "input.tsv".into(),
						bytes: Some(9),
						contents: Some("expected".into()),
						error: None,
					},
					FixtureExpectedFileRead {
						node: "missing".into(),
						path: "missing.tsv".into(),
						bytes: None,
						contents: None,
						error: None,
					},
				],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(!result.ok);
		assert_eq!(result.failures.len(), 5);
		assert!(result.failures.iter().any(|f| f.contains("effect_count: expected 2")));
		assert!(result.failures.iter().any(|f| f.contains("bytes: expected 9")));
		assert!(result.failures.iter().any(|f| f.contains("contents: expected")));
		assert!(result.failures.iter().any(|f| f.contains("error: expected None")));
		assert!(result.failures.iter().any(|f| f == "file_read missing:missing.tsv: missing"));
	}

	#[test]
	fn file_write_assertion_passes() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.effect_count = 1;
		report.recorded_effects.push(FixtureRecordedEffect {
			kind: "file_write".into(),
			node: "write".into(),
			method: None,
			url: None,
			path: Some("out.tsv".into()),
			status: None,
			bytes: Some(12),
			contents: Some("a\tb\n1\t2\n".into()),
			request_body: None,
			response_body: None,
			error: None,
		});
		let case = FixtureTestCase {
			name: Some("write".into()),
			expect: FixtureExpect {
				effect_count: Some(1),
				file_writes: vec![FixtureExpectedFileWrite {
					node: "write".into(),
					path: "out.tsv".into(),
					bytes: Some(12),
					contents: Some("a\tb\n1\t2\n".into()),
					error: None,
				}],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(result.ok, "{:?}", result.failures);
	}

	#[test]
	fn file_write_assertion_reports_missing_and_mismatch() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.effect_count = 1;
		report.recorded_effects.push(FixtureRecordedEffect {
			kind: "file_write".into(),
			node: "write".into(),
			method: None,
			url: None,
			path: Some("out.tsv".into()),
			status: None,
			bytes: Some(12),
			contents: Some("actual".into()),
			request_body: None,
			response_body: None,
			error: Some("disk full".into()),
		});
		let case = FixtureTestCase {
			name: Some("write".into()),
			expect: FixtureExpect {
				effect_count: Some(2),
				file_writes: vec![
					FixtureExpectedFileWrite {
						node: "write".into(),
						path: "out.tsv".into(),
						bytes: Some(13),
						contents: Some("expected".into()),
						error: None,
					},
					FixtureExpectedFileWrite {
						node: "missing".into(),
						path: "missing.tsv".into(),
						bytes: None,
						contents: None,
						error: None,
					},
				],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(!result.ok);
		assert_eq!(result.failures.len(), 5);
		assert!(result.failures.iter().any(|f| f.contains("effect_count: expected 2")));
		assert!(result.failures.iter().any(|f| f.contains("bytes: expected 13")));
		assert!(result.failures.iter().any(|f| f.contains("contents: expected")));
		assert!(result.failures.iter().any(|f| f.contains("error: expected None")));
		assert!(result.failures.iter().any(|f| f == "file_write missing:missing.tsv: missing"));
	}

	#[test]
	fn http_request_assertion_passes() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.effect_count = 1;
		report.recorded_effects.push(FixtureRecordedEffect {
			kind: "http".into(),
			node: "http".into(),
			method: Some("POST".into()),
			url: Some("http://localhost/test".into()),
			path: None,
			status: Some(202),
			bytes: None,
			contents: None,
			request_body: Some(serde_json::json!({"a": 1})),
			response_body: Some("{\"ok\":true}".into()),
			error: None,
		});
		let case = FixtureTestCase {
			name: Some("http".into()),
			expect: FixtureExpect {
				http_requests: vec![FixtureExpectedHttpRequest {
					node: "http".into(),
					method: "POST".into(),
					url: "http://localhost/test".into(),
					status: Some(202),
					request_body: Some(toml::Value::Table([("a".into(), toml::Value::Integer(1))].into_iter().collect())),
					response_body: Some("{\"ok\":true}".into()),
					error: None,
				}],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(result.ok, "{:?}", result.failures);
	}

	#[test]
	fn http_request_assertion_reports_missing_and_mismatch() {
		let mut report = report_with_stored_value("n", "out", "string", serde_json::json!("ok"));
		report.recorded_effects.push(FixtureRecordedEffect {
			kind: "http".into(),
			node: "http".into(),
			method: Some("POST".into()),
			url: Some("http://localhost/test".into()),
			path: None,
			status: Some(500),
			bytes: None,
			contents: None,
			request_body: Some(serde_json::json!({"actual": true})),
			response_body: Some("actual".into()),
			error: Some("boom".into()),
		});
		let case = FixtureTestCase {
			name: Some("http".into()),
			expect: FixtureExpect {
				http_requests: vec![
					FixtureExpectedHttpRequest {
						node: "http".into(),
						method: "POST".into(),
						url: "http://localhost/test".into(),
						status: Some(200),
						request_body: Some(toml::Value::Table(
							[("expected".into(), toml::Value::Boolean(true))].into_iter().collect(),
						)),
						response_body: Some("expected".into()),
						error: None,
					},
					FixtureExpectedHttpRequest {
						node: "missing".into(),
						method: "GET".into(),
						url: "http://localhost/missing".into(),
						status: None,
						request_body: None,
						response_body: None,
						error: None,
					},
				],
				..FixtureExpect::default()
			},
		};

		let result = evaluate_test_case(Path::new("x.flowgraph.test.toml"), 0, &case, &report);

		assert!(!result.ok);
		assert_eq!(result.failures.len(), 5);
		assert!(result.failures.iter().any(|f| f.contains("status: expected Some(200)")));
		assert!(result.failures.iter().any(|f| f.contains("request_body: expected")));
		assert!(result.failures.iter().any(|f| f.contains("response_body: expected")));
		assert!(result.failures.iter().any(|f| f.contains("error: expected None")));
		assert!(result
			.failures
			.iter()
			.any(|f| f == "http_request missing:GET http://localhost/missing: missing"));
	}

	fn report_with_stored_value(node: &str, port: &str, ty: &str, value: serde_json::Value) -> FixtureRunReport {
		FixtureRunReport {
			ok: true,
			root: "test".into(),
			generation: 1,
			node_count: 1,
			mock_count: 0,
			mocks: Vec::new(),
			trigger_count: 0,
			trigger_history: Vec::new(),
			trace: Vec::new(),
			trace_count: 0,
			effect_count: 0,
			recorded_effects: Vec::new(),
			stored_values: vec![FixtureTraceValue {
				node: node.into(),
				port: port.into(),
				ty: ty.into(),
				value,
			}],
			exec_count: Vec::new(),
			pure_evaluations: Vec::new(),
			cache_hits: 0,
			cache_misses: 0,
			tests: Vec::new(),
			failed_tests: 0,
		}
	}

	#[test]
	fn discover_fixture_roots_finds_nested_test_dirs() {
		let root = make_temp_dir("fixture-roots");
		std::fs::create_dir_all(root.join("a")).unwrap();
		std::fs::create_dir_all(root.join("nested").join("b")).unwrap();
		std::fs::write(root.join("a").join("main.flowgraph.test.toml"), "").unwrap();
		std::fs::write(root.join("nested").join("b").join("case.flowgraph.test.toml"), "").unwrap();
		std::fs::write(root.join("nested").join("ignore.txt"), "").unwrap();

		let roots = discover_fixture_roots(&root).unwrap();
		let labels: Vec<String> = roots
			.iter()
			.map(|path| path.strip_prefix(&root).unwrap().display().to_string().replace('\\', "/"))
			.collect();

		assert_eq!(labels, vec!["a", "nested/b"]);
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn discover_fixture_roots_stops_at_fixture_dir() {
		let root = make_temp_dir("fixture-root-stop");
		std::fs::create_dir_all(root.join("fixture").join("child")).unwrap();
		std::fs::write(root.join("fixture").join("main.flowgraph.test.toml"), "").unwrap();
		std::fs::write(root.join("fixture").join("child").join("nested.flowgraph.test.toml"), "").unwrap();

		let roots = discover_fixture_roots(&root).unwrap();
		let labels: Vec<String> = roots
			.iter()
			.map(|path| path.strip_prefix(&root).unwrap().display().to_string().replace('\\', "/"))
			.collect();

		assert_eq!(labels, vec!["fixture"]);
		let _ = std::fs::remove_dir_all(root);
	}

	#[test]
	fn read_declared_tests_rejects_dir_without_test_files() {
		let root = make_temp_dir("fixture-no-tests");
		let error = match read_declared_tests(&root) {
			Ok(_) => panic!("expected missing fixture test file error"),
			Err(error) => error,
		};

		assert!(error.to_string().contains("no *.flowgraph.test.toml files found"));
		let _ = std::fs::remove_dir_all(root);
	}

	#[tokio::test]
	async fn fixture_suite_rejects_root_without_fixtures() {
		let root = make_temp_dir("fixture-suite-empty");
		let error = run_fixture_suite_report(&root).await.unwrap_err();

		assert!(error.to_string().contains("no *.flowgraph.test.toml files found"));
		let _ = std::fs::remove_dir_all(root);
	}

	fn make_temp_dir(label: &str) -> PathBuf {
		let path = std::env::temp_dir().join(format!(
			"vac-{label}-{}-{}",
			std::process::id(),
			std::time::SystemTime::now()
				.duration_since(std::time::UNIX_EPOCH)
				.unwrap()
				.as_nanos()
		));
		std::fs::create_dir_all(&path).unwrap();
		path
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
