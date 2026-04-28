//! Phase tau: process control ノード。
//!
//! 常駐VACが外部補助アプリや開発用ツールを Flowgraph から扱うための first slice。
//! shell 文字列ではなく「command + args」を基本にして、意図しない shell 展開を避ける。

use crate::flowgraph::node::{
	get_optional_bool, get_optional_int, get_optional_string, get_required_int, get_required_json, get_required_list, get_required_string,
	EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use std::process::Stdio;
use std::time::{Duration, Instant};
use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate, System};

pub struct ProcessSpawnNode;
pub struct ProcessRunningNode;
pub struct ProcessKillNode;
pub struct ProcessWaitNode;

impl NodeDescriptor for ProcessSpawnNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.process.spawn".into(),
			title: "Process: Spawn".into(),
			category: "process".into(),
			description: Some("外部 process を起動する。shell は介さず command + args を直接実行する。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("command", "Command", SocketType::String),
				PortSpec::input("args", "Args", SocketType::List(Box::new(SocketType::String))).with_default(SocketValue::List(Vec::new())),
				PortSpec::input("working_dir", "Working Dir", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("env", "Env", SocketType::Json).with_default(SocketValue::Json(JsonValue::Object(Default::default()))),
				PortSpec::input("wait", "Wait", SocketType::Bool).with_default(SocketValue::Bool(false)),
				PortSpec::input("timeout_ms", "Timeout ms", SocketType::Int).with_default(SocketValue::Int(0)),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("pid", "PID", SocketType::Int),
				PortSpec::output("exit_code", "Exit Code", SocketType::Int),
				PortSpec::output("stdout", "Stdout", SocketType::String),
				PortSpec::output("stderr", "Stderr", SocketType::String),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ProcessSpawnNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let spec = SpawnSpec {
			command: get_required_string(inputs, "command")?,
			args: get_string_list(inputs, "args")?,
			working_dir: get_optional_string(inputs, "working_dir", "")?,
			env: get_required_json(inputs, "env")?.clone(),
			wait: get_optional_bool(inputs, "wait", false)?,
			timeout_ms: get_optional_int(inputs, "timeout_ms", 0)?,
		};
		match spawn_process(spec).await {
			Ok(result) => {
				ctx.log(format!(
					"process.spawn: pid={} exit_code={}",
					result.pid.unwrap_or(0),
					result.exit_code.unwrap_or(-1)
				));
				let ok = result.exit_code.map(|code| code == 0).unwrap_or(true);
				let out = NodeOutput::new()
					.set_data("pid", SocketValue::Int(result.pid.map(u32_to_i64).unwrap_or(0)))
					.set_data("exit_code", SocketValue::Int(result.exit_code.unwrap_or(-1)))
					.set_data("stdout", SocketValue::String(result.stdout))
					.set_data("stderr", SocketValue::String(result.stderr))
					.set_data("error", SocketValue::String(String::new()));
				Ok(if ok {
					out.fire_exec("on_success")
				} else {
					out.fire_exec("on_error")
				})
			}
			Err(e) => {
				ctx.log(format!("process.spawn: error: {e}"));
				Ok(process_error_output(e))
			}
		}
	}
}

impl NodeDescriptor for ProcessRunningNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.process.running".into(),
			title: "Process: Running".into(),
			category: "process".into(),
			description: Some("pid または name_filter で process の生存状態を確認する。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("pid", "PID", SocketType::Int).with_default(SocketValue::Int(0)),
				PortSpec::input("name_filter", "Name Filter", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("exact", "Exact Name", SocketType::Bool).with_default(SocketValue::Bool(false)),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("running", "Running", SocketType::Bool),
				PortSpec::output("count", "Count", SocketType::Int),
				PortSpec::output("pids", "PIDs", SocketType::Json),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ProcessRunningNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let query = ProcessQuery::from_inputs(inputs)?;
		let pids = find_matching_pids(&query);
		ctx.log(format!("process.running: {} matches", pids.len()));
		Ok(process_query_output(pids).fire_exec("exec_out"))
	}
}

impl NodeDescriptor for ProcessKillNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.process.kill".into(),
			title: "Process: Kill".into(),
			category: "process".into(),
			description: Some("pid または name_filter に一致する process を終了する。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("pid", "PID", SocketType::Int).with_default(SocketValue::Int(0)),
				PortSpec::input("name_filter", "Name Filter", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("exact", "Exact Name", SocketType::Bool).with_default(SocketValue::Bool(false)),
				PortSpec::input("force", "Force", SocketType::Bool).with_default(SocketValue::Bool(true)),
			],
			outputs: vec![
				PortSpec::exec_output("on_success", "On Success"),
				PortSpec::exec_output("on_error", "On Error"),
				PortSpec::output("killed_count", "Killed Count", SocketType::Int),
				PortSpec::output("pids", "PIDs", SocketType::Json),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ProcessKillNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let query = ProcessQuery::from_inputs(inputs)?;
		if query.pid == 0 && query.name_filter.trim().is_empty() {
			return Ok(kill_output(Vec::new(), 0, "pid or name_filter is required").fire_exec("on_error"));
		}
		let _force = get_optional_bool(inputs, "force", true)?;
		let pids = find_matching_pids(&query);
		let killed_count = kill_pids(&pids);
		ctx.log(format!("process.kill: {} / {} requested", killed_count, pids.len()));
		let out = kill_output(pids, killed_count, "");
		Ok(if killed_count > 0 {
			out.fire_exec("on_success")
		} else {
			out.fire_exec("on_error")
		})
	}
}

impl NodeDescriptor for ProcessWaitNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.process.wait".into(),
			title: "Process: Wait".into(),
			category: "process".into(),
			description: Some("指定 pid が終了するまで polling で待つ。exit_code は未取得時 -1。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("pid", "PID", SocketType::Int),
				PortSpec::input("timeout_ms", "Timeout ms", SocketType::Int).with_default(SocketValue::Int(30000)),
				PortSpec::input("poll_interval_ms", "Poll Interval ms", SocketType::Int).with_default(SocketValue::Int(250)),
			],
			outputs: vec![
				PortSpec::exec_output("on_exit", "On Exit"),
				PortSpec::exec_output("on_timeout", "On Timeout"),
				PortSpec::output("exited", "Exited", SocketType::Bool),
				PortSpec::output("exit_code", "Exit Code", SocketType::Int),
				PortSpec::output("error", "Error", SocketType::String),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for ProcessWaitNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let pid = i64_to_pid_u32(get_required_int(inputs, "pid")?)?;
		let timeout_ms = get_optional_int(inputs, "timeout_ms", 30000)?.clamp(0, 86_400_000) as u64;
		let poll_interval_ms = get_optional_int(inputs, "poll_interval_ms", 250)?.clamp(10, 10_000) as u64;
		let exited = wait_until_exit(pid, Duration::from_millis(timeout_ms), Duration::from_millis(poll_interval_ms)).await;
		ctx.log(format!("process.wait: pid={} exited={}", pid, exited));
		let out = NodeOutput::new()
			.set_data("exited", SocketValue::Bool(exited))
			.set_data("exit_code", SocketValue::Int(-1))
			.set_data("error", SocketValue::String(String::new()));
		Ok(if exited {
			out.fire_exec("on_exit")
		} else {
			out.fire_exec("on_timeout")
		})
	}
}

#[derive(Debug, Clone)]
struct SpawnSpec {
	command: String,
	args: Vec<String>,
	working_dir: String,
	env: JsonValue,
	wait: bool,
	timeout_ms: i64,
}

#[derive(Debug, Clone)]
struct SpawnResult {
	pid: Option<u32>,
	exit_code: Option<i64>,
	stdout: String,
	stderr: String,
}

#[derive(Debug, Clone)]
struct ProcessQuery {
	pid: u32,
	name_filter: String,
	exact: bool,
}

impl ProcessQuery {
	fn from_inputs(inputs: &InputMap) -> Result<Self, NodeExecError> {
		Ok(Self {
			pid: i64_to_pid_u32(get_optional_int(inputs, "pid", 0)?)?,
			name_filter: get_optional_string(inputs, "name_filter", "")?,
			exact: get_optional_bool(inputs, "exact", false)?,
		})
	}
}

async fn spawn_process(spec: SpawnSpec) -> Result<SpawnResult, String> {
	let command = spec.command.trim();
	if command.is_empty() {
		return Err("command is empty".into());
	}
	let env = parse_env(&spec.env)?;
	let mut cmd = tokio::process::Command::new(command);
	cmd.args(spec.args.iter());
	if !spec.working_dir.trim().is_empty() {
		cmd.current_dir(spec.working_dir.trim());
	}
	for (k, v) in env {
		cmd.env(k, v);
	}
	if spec.wait {
		cmd.stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
		let timeout_ms = spec.timeout_ms.clamp(0, 86_400_000) as u64;
		if timeout_ms > 0 {
			cmd.kill_on_drop(true);
		}
		let pid_holder;
		let child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
		pid_holder = child.id();
		let wait_future = child.wait_with_output();
		let output = if timeout_ms > 0 {
			match tokio::time::timeout(Duration::from_millis(timeout_ms), wait_future).await {
				Ok(r) => r.map_err(|e| format!("wait failed: {e}"))?,
				Err(_) => return Err(format!("timeout after {timeout_ms} ms")),
			}
		} else {
			wait_future.await.map_err(|e| format!("wait failed: {e}"))?
		};
		Ok(SpawnResult {
			pid: pid_holder,
			exit_code: Some(i64::from(output.status.code().unwrap_or(-1))),
			stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
			stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
		})
	} else {
		cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
		let child = cmd.spawn().map_err(|e| format!("spawn failed: {e}"))?;
		Ok(SpawnResult {
			pid: child.id(),
			exit_code: None,
			stdout: String::new(),
			stderr: String::new(),
		})
	}
}

fn get_string_list(inputs: &InputMap, key: &str) -> Result<Vec<String>, NodeExecError> {
	let values = match inputs.get(key) {
		None => return Ok(Vec::new()),
		Some(_) => get_required_list(inputs, key)?,
	};
	let mut out = Vec::with_capacity(values.len());
	for (index, value) in values.iter().enumerate() {
		let s = value
			.as_str()
			.map_err(|_| NodeExecError::Generic(anyhow::anyhow!("{key}[{index}] must be string, got {}", value.type_of())))?;
		out.push(s.to_owned());
	}
	Ok(out)
}

fn parse_env(v: &JsonValue) -> Result<Vec<(String, String)>, String> {
	let Some(obj) = v.as_object() else {
		return Err("env must be a JSON object".into());
	};
	let mut out = Vec::with_capacity(obj.len());
	for (key, value) in obj {
		if key.is_empty() || key.contains('=') {
			return Err(format!("invalid env key: {key:?}"));
		}
		let value = match value {
			JsonValue::String(s) => s.clone(),
			JsonValue::Number(_) | JsonValue::Bool(_) => value.to_string(),
			JsonValue::Null => String::new(),
			_ => return Err(format!("env value for {key:?} must be string/number/bool/null")),
		};
		out.push((key.clone(), value));
	}
	Ok(out)
}

fn find_matching_pids(query: &ProcessQuery) -> Vec<u32> {
	let mut system = System::new();
	system.refresh_processes_specifics(ProcessesToUpdate::All, false, ProcessRefreshKind::everything().without_cpu());
	if query.pid > 0 {
		return system
			.process(Pid::from_u32(query.pid))
			.map(|_| vec![query.pid])
			.unwrap_or_default();
	}
	let filter = query.name_filter.trim().to_ascii_lowercase();
	if filter.is_empty() {
		return Vec::new();
	}
	let mut pids: Vec<u32> = system
		.processes()
		.iter()
		.filter_map(|(pid, process)| {
			let name = process.name().to_string_lossy().to_ascii_lowercase();
			let matched = if query.exact { name == filter } else { name.contains(&filter) };
			matched.then_some(pid.as_u32())
		})
		.collect();
	pids.sort_unstable();
	pids
}

fn kill_pids(pids: &[u32]) -> i64 {
	let mut system = System::new();
	system.refresh_processes_specifics(ProcessesToUpdate::All, false, ProcessRefreshKind::everything().without_cpu());
	let mut killed = 0i64;
	for pid in pids {
		if let Some(process) = system.process(Pid::from_u32(*pid)) {
			if process.kill() {
				killed += 1;
			}
		}
	}
	killed
}

async fn wait_until_exit(pid: u32, timeout: Duration, poll_interval: Duration) -> bool {
	let deadline = Instant::now() + timeout;
	loop {
		if !is_pid_alive(pid) {
			return true;
		}
		if timeout.is_zero() || Instant::now() >= deadline {
			return false;
		}
		tokio::time::sleep(poll_interval.min(deadline.saturating_duration_since(Instant::now()))).await;
	}
}

fn is_pid_alive(pid: u32) -> bool {
	let mut system = System::new();
	system.refresh_processes_specifics(ProcessesToUpdate::All, false, ProcessRefreshKind::everything().without_cpu());
	system.process(Pid::from_u32(pid)).is_some()
}

fn process_query_output(pids: Vec<u32>) -> NodeOutput {
	NodeOutput::new()
		.set_data("running", SocketValue::Bool(!pids.is_empty()))
		.set_data("count", SocketValue::Int(pids.len() as i64))
		.set_data("pids", SocketValue::Json(json!(pids)))
		.set_data("error", SocketValue::String(String::new()))
}

fn kill_output(pids: Vec<u32>, killed_count: i64, error: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("killed_count", SocketValue::Int(killed_count))
		.set_data("pids", SocketValue::Json(json!(pids)))
		.set_data("error", SocketValue::String(error.into()))
}

fn process_error_output(e: impl Into<String>) -> NodeOutput {
	NodeOutput::new()
		.set_data("pid", SocketValue::Int(0))
		.set_data("exit_code", SocketValue::Int(-1))
		.set_data("stdout", SocketValue::String(String::new()))
		.set_data("stderr", SocketValue::String(String::new()))
		.set_data("error", SocketValue::String(e.into()))
		.fire_exec("on_error")
}

fn i64_to_pid_u32(v: i64) -> Result<u32, NodeExecError> {
	if v < 0 || v > i64::from(u32::MAX) {
		return Err(NodeExecError::Generic(anyhow::anyhow!("pid out of range: {v}")));
	}
	Ok(v as u32)
}

fn u32_to_i64(v: u32) -> i64 {
	i64::from(v)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn exec_in_fire() -> ExecFireSet {
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		fired
	}

	#[tokio::test]
	async fn process_nodes_no_fire_are_noop() {
		let mut ctx = ExecCtx::default();
		let nodes: Vec<Box<dyn EffectfulNode>> = vec![
			Box::new(ProcessSpawnNode),
			Box::new(ProcessRunningNode),
			Box::new(ProcessKillNode),
			Box::new(ProcessWaitNode),
		];
		for node in nodes {
			let out = node
				.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
				.await
				.unwrap();
			assert!(out.fired_exec.is_empty());
			assert!(out.data.is_empty());
		}
	}

	#[test]
	fn parse_env_accepts_scalar_values() {
		let env = parse_env(&json!({
			"A": "alpha",
			"B": 42,
			"C": false,
			"D": null
		}))
		.unwrap();
		assert_eq!(env[0], ("A".into(), "alpha".into()));
		assert_eq!(env[1], ("B".into(), "42".into()));
		assert_eq!(env[2], ("C".into(), "false".into()));
		assert_eq!(env[3], ("D".into(), "".into()));
	}

	#[tokio::test]
	async fn running_finds_current_process_by_pid() {
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [("pid".to_string(), SocketValue::Int(i64::from(std::process::id())))]
			.into_iter()
			.collect();
		let out = ProcessRunningNode
			.execute(&mut ctx, &InputMap::new(), &inputs, &exec_in_fire())
			.await
			.unwrap();
		assert!(matches!(out.data.get("running"), Some(SocketValue::Bool(true))));
		assert!(matches!(out.data.get("count"), Some(SocketValue::Int(1))));
		assert!(out.fired_exec.contains("exec_out"));
	}

	#[tokio::test]
	async fn spawn_wait_reports_exit_code() {
		let exe = std::env::current_exe().unwrap();
		let inputs: InputMap = [
			("command".to_string(), SocketValue::String(exe.to_string_lossy().to_string())),
			("args".to_string(), SocketValue::List(vec![SocketValue::String("--help".into())])),
			("env".to_string(), SocketValue::Json(JsonValue::Object(Default::default()))),
			("wait".to_string(), SocketValue::Bool(true)),
			("timeout_ms".to_string(), SocketValue::Int(30_000)),
		]
		.into_iter()
		.collect();
		let mut ctx = ExecCtx::default();
		let out = ProcessSpawnNode
			.execute(&mut ctx, &InputMap::new(), &inputs, &exec_in_fire())
			.await
			.unwrap();
		assert!(matches!(out.data.get("exit_code"), Some(SocketValue::Int(0))));
		assert!(out.fired_exec.contains("on_success"));
	}

	#[tokio::test]
	async fn wait_on_nonexistent_pid_exits_immediately() {
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [
			("pid".to_string(), SocketValue::Int(i64::from(u32::MAX))),
			("timeout_ms".to_string(), SocketValue::Int(1000)),
		]
		.into_iter()
		.collect();
		let out = ProcessWaitNode
			.execute(&mut ctx, &InputMap::new(), &inputs, &exec_in_fire())
			.await
			.unwrap();
		assert!(matches!(out.data.get("exited"), Some(SocketValue::Bool(true))));
		assert!(out.fired_exec.contains("on_exit"));
	}
}
