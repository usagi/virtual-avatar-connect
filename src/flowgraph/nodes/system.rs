//! Phase sigma: system metrics ノード。
//!
//! 常駐VACが自分の動作モードや負荷制御を判断するための first slice。

use crate::flowgraph::node::{
	get_optional_int, get_optional_string, EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput,
	NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};
use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System, MINIMUM_CPU_UPDATE_INTERVAL};

pub struct SystemCpuUsageNode;
pub struct SystemMemoryNode;
pub struct SystemLoadAvgNode;
pub struct SystemProcessListNode;

impl NodeDescriptor for SystemCpuUsageNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.system.cpu_usage".into(),
			title: "System: CPU Usage".into(),
			category: "system".into(),
			description: Some("sysinfo で全体 CPU 使用率と logical CPU ごとの使用率を取得する。".into()),
			inputs: vec![PortSpec::exec_input("exec_in", "Exec")],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("usage_percent", "Usage %", SocketType::Float),
				PortSpec::output("core_count", "Core Count", SocketType::Int),
				PortSpec::output("per_cpu", "Per CPU", SocketType::Json),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for SystemCpuUsageNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		_inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let mut system = System::new_all();
		tokio::time::sleep(MINIMUM_CPU_UPDATE_INTERVAL).await;
		system.refresh_cpu_usage();
		let cpus: Vec<JsonValue> = system
			.cpus()
			.iter()
			.enumerate()
			.map(|(index, cpu)| {
				json!({
					"index": index,
					"name": cpu.name(),
					"brand": cpu.brand(),
					"frequency_hz": cpu.frequency(),
					"usage_percent": f64::from(cpu.cpu_usage()),
				})
			})
			.collect();
		let usage = f64::from(system.global_cpu_usage());
		ctx.log(format!("system.cpu_usage: {usage:.1}%"));
		Ok(NodeOutput::new()
			.set_data("usage_percent", SocketValue::Float(usage))
			.set_data("core_count", SocketValue::Int(cpus.len() as i64))
			.set_data("per_cpu", SocketValue::Json(JsonValue::Array(cpus)))
			.fire_exec("exec_out"))
	}
}

impl NodeDescriptor for SystemMemoryNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.system.memory".into(),
			title: "System: Memory".into(),
			category: "system".into(),
			description: Some("sysinfo で RAM / swap の使用量を byte 単位で取得する。".into()),
			inputs: vec![PortSpec::exec_input("exec_in", "Exec")],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("mem_used", "Memory Used", SocketType::Int),
				PortSpec::output("mem_total", "Memory Total", SocketType::Int),
				PortSpec::output("mem_available", "Memory Available", SocketType::Int),
				PortSpec::output("mem_usage_percent", "Memory Usage %", SocketType::Float),
				PortSpec::output("swap_used", "Swap Used", SocketType::Int),
				PortSpec::output("swap_total", "Swap Total", SocketType::Int),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for SystemMemoryNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		_inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let mut system = System::new();
		system.refresh_memory();
		let used = system.used_memory();
		let total = system.total_memory();
		let usage_percent = if total > 0 { (used as f64 / total as f64) * 100.0 } else { 0.0 };
		ctx.log(format!("system.memory: {} / {} bytes", used, total));
		Ok(NodeOutput::new()
			.set_data("mem_used", SocketValue::Int(u64_to_i64(used)))
			.set_data("mem_total", SocketValue::Int(u64_to_i64(total)))
			.set_data("mem_available", SocketValue::Int(u64_to_i64(system.available_memory())))
			.set_data("mem_usage_percent", SocketValue::Float(usage_percent))
			.set_data("swap_used", SocketValue::Int(u64_to_i64(system.used_swap())))
			.set_data("swap_total", SocketValue::Int(u64_to_i64(system.total_swap())))
			.fire_exec("exec_out"))
	}
}

impl NodeDescriptor for SystemLoadAvgNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.system.load_avg".into(),
			title: "System: Load Average".into(),
			category: "system".into(),
			description: Some("OS の load average を取得する。未対応OSでは sysinfo の値をそのまま返す。".into()),
			inputs: vec![PortSpec::exec_input("exec_in", "Exec")],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("one", "1 min", SocketType::Float),
				PortSpec::output("five", "5 min", SocketType::Float),
				PortSpec::output("fifteen", "15 min", SocketType::Float),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for SystemLoadAvgNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		_inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let load = System::load_average();
		ctx.log(format!("system.load_avg: {:.2} {:.2} {:.2}", load.one, load.five, load.fifteen));
		Ok(NodeOutput::new()
			.set_data("one", SocketValue::Float(load.one))
			.set_data("five", SocketValue::Float(load.five))
			.set_data("fifteen", SocketValue::Float(load.fifteen))
			.fire_exec("exec_out"))
	}
}

impl NodeDescriptor for SystemProcessListNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.system.process_list".into(),
			title: "System: Process List".into(),
			category: "system".into(),
			description: Some("sysinfo で process 一覧を JSON 配列として取得する。name_filter と limit で絞り込める。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("name_filter", "Name Filter", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("limit", "Limit", SocketType::Int).with_default(SocketValue::Int(100)),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("count", "Count", SocketType::Int),
				PortSpec::output("processes", "Processes", SocketType::Json),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for SystemProcessListNode {
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
		let filter = get_optional_string(inputs, "name_filter", "")?.trim().to_ascii_lowercase();
		let limit = get_optional_int(inputs, "limit", 100)?.clamp(1, 1000) as usize;
		let mut system = System::new();
		system.refresh_processes_specifics(ProcessesToUpdate::All, false, ProcessRefreshKind::everything());
		let mut processes: Vec<JsonValue> = system
			.processes()
			.values()
			.filter(|process| filter.is_empty() || process.name().to_string_lossy().to_ascii_lowercase().contains(&filter))
			.map(|process| {
				json!({
					"pid": process.pid().as_u32(),
					"parent_pid": process.parent().map(|pid| pid.as_u32()),
					"name": process.name().to_string_lossy(),
					"exe": process.exe().map(|path| path.to_string_lossy().to_string()).unwrap_or_default(),
					"cmd": process.cmd().iter().map(|v| v.to_string_lossy().to_string()).collect::<Vec<_>>(),
					"status": format!("{:?}", process.status()),
					"cpu_usage_percent": f64::from(process.cpu_usage()),
					"memory": process.memory(),
					"virtual_memory": process.virtual_memory(),
					"start_time": process.start_time(),
					"run_time": process.run_time(),
				})
			})
			.collect();
		processes.sort_by(|a, b| json_u64(b, "memory").cmp(&json_u64(a, "memory")));
		processes.truncate(limit);
		ctx.log(format!("system.process_list: {} processes", processes.len()));
		Ok(NodeOutput::new()
			.set_data("count", SocketValue::Int(processes.len() as i64))
			.set_data("processes", SocketValue::Json(JsonValue::Array(processes)))
			.fire_exec("exec_out"))
	}
}

fn u64_to_i64(v: u64) -> i64 {
	i64::try_from(v).unwrap_or(i64::MAX)
}

fn json_u64(v: &JsonValue, key: &str) -> u64 {
	v.get(key).and_then(JsonValue::as_u64).unwrap_or(0)
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
	async fn system_nodes_no_fire_are_noop() {
		let mut ctx = ExecCtx::default();
		let nodes: Vec<Box<dyn EffectfulNode>> = vec![
			Box::new(SystemCpuUsageNode),
			Box::new(SystemMemoryNode),
			Box::new(SystemLoadAvgNode),
			Box::new(SystemProcessListNode),
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

	#[tokio::test]
	async fn memory_node_reports_basic_values() {
		let mut ctx = ExecCtx::default();
		let out = SystemMemoryNode
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &exec_in_fire())
			.await
			.unwrap();
		assert!(matches!(out.data.get("mem_total"), Some(SocketValue::Int(v)) if *v >= 0));
		assert!(out.fired_exec.contains("exec_out"));
	}

	#[tokio::test]
	async fn process_list_limit_is_applied() {
		let mut ctx = ExecCtx::default();
		let inputs: InputMap = [("limit".to_string(), SocketValue::Int(1))].into_iter().collect();
		let out = SystemProcessListNode
			.execute(&mut ctx, &InputMap::new(), &inputs, &exec_in_fire())
			.await
			.unwrap();
		assert!(matches!(out.data.get("count"), Some(SocketValue::Int(v)) if *v <= 1));
		assert!(out.fired_exec.contains("exec_out"));
	}
}
