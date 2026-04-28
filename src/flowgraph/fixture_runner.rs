//! Flowgraph **fixture ランナー**の最小実装（language roadmap P2 / PR4 先取り）。
//!
//! `load_flowgraph_dir` でディレクトリをロードし、可能なら 1-shot [`FlowgraphProgram::execute`] を走らせる。
//! 外部 IO・ingress・`state_handle` 必須ノードを含むグラフは失敗しうるため、CI 用の厳密 runner ではなく
//! **ロード＋純粋グラフのスモーク**向け。

use crate::flowgraph::node::ExecCtx;
use crate::flowgraph::{load_flowgraph_dir, FlowgraphProgram, LoadError, NodeExecError, ProgramRun};
use std::path::Path;

/// ロードに成功したときの結果。
pub struct FixtureLoadOk {
	pub program: FlowgraphProgram,
}

/// ロード失敗、または（要求時）実行失敗。
#[derive(Debug)]
pub enum FixtureError {
	Load(LoadError),
	Execute(NodeExecError),
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
}
