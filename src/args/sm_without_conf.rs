use super::Args;
use crate::SharedAudioSink;
use anyhow::Result;

impl Args {
	/// conf 不要の特殊モード処理群の実行
	pub async fn execute_special_modes_without_conf(&self, _audio_sink: SharedAudioSink) -> Result<()> {
		// debug が true ならログレベルを Trace に設定
		match self.debug {
			true => {
				log::set_max_level(log::LevelFilter::Trace);
				log::warn!("コマンドライン引数で -D/--debug が指定されたためログレベルが Trace に設定されます。");
				log::trace!("コマンドライン引数のパース結果: {:?}", self);
			}
			false => log::set_max_level(log::LevelFilter::Info),
		}

		// δ-9 (v0.9.x) で V1 processor 層を除去したため、CoeiroInk/AivisSpeech/VOICEVOX 各 engine の
		// speakers 一覧や OS-TTS の簡易テストは、Flowgraph TTS ドライバ側に再実装される予定（δ-9.3）。
		// 現状は各 `--*_speakers` / `--test-os-tts` を受けても「v0.9 では一時停止中」であることだけ
		// 通知して終了する。
		if self.coeiroink_speakers || self.aivisspeech_speakers || self.voicevox_speakers || self.test_os_tts {
			log::warn!(
				"--coeiroink-speakers / --aivisspeech-speakers / --voicevox-speakers / --test-os-tts は v0.9 で一時停止中です。\
     Flowgraph TTS ドライバ経由のコマンドとして δ-9.3 で再実装予定です。"
			);
			std::process::exit(0);
		}

		if self.migrate {
			let out_dir = std::path::PathBuf::from(&self.migrate_out_dir);
			match crate::migrate::migrate_conf_file(std::path::Path::new(&self.conf), &out_dir, self.migrate_strict) {
				Ok(report) => {
					println!("{}", report.summary());
					if report.has_errors() {
						std::process::exit(1);
					}
					std::process::exit(0);
				}
				Err(e) => {
					log::error!("migrate 失敗: {e}");
					std::process::exit(1);
				}
			}
		}

		if self.flowgraph_test_dir.is_some() && self.flowgraph_test_root.is_some() {
			let message = "--flowgraph-test-dir と --flowgraph-test-root は同時に指定できません。";
			if self.flowgraph_test_json {
				println!(
					"{}",
					serde_json::to_string_pretty(&serde_json::json!({
						"ok": false,
						"error": message,
					}))?
				);
			} else {
				log::error!("{message}");
			}
			std::process::exit(1);
		}

		if let Some(dir) = &self.flowgraph_test_dir {
			let root = std::path::PathBuf::from(dir);
			match crate::flowgraph::fixture_runner::run_fixture_once_report(&root).await {
				Ok(report) => {
					if self.flowgraph_test_json {
						println!("{}", serde_json::to_string_pretty(&report)?);
					} else {
						let status = if report.ok { "OK" } else { "FAILED" };
						println!("Flowgraph fixture {status}: {}", report.root);
						println!("  nodes: {}", report.node_count);
						println!("  stateful_nodes: {}", report.capability_summary.stateful_node_count);
						println!("  generation: {}", report.generation);
						println!(
							"  capabilities: {}",
							format_capability_counts(&report.capability_summary.capability_counts)
						);
						println!("  mocks: {}", report.mock_count);
						println!("  trace: {} line(s)", report.trace_count);
						println!("  effects: {}", report.effect_count);
						println!("  stored_values: {}", report.stored_values.len());
						println!("  state_versions: {}", report.state_versions.len());
						println!("  cache: {} hit(s), {} miss(es)", report.cache_hits, report.cache_misses);
						if !report.tests.is_empty() {
							println!(
								"  tests: {} passed / {} failed",
								report.tests.len() - report.failed_tests,
								report.failed_tests
							);
							for test in &report.tests {
								if !test.ok {
									println!("    FAIL {} :: {}", test.file, test.name);
									for failure in &test.failures {
										println!("      - {failure}");
									}
								}
							}
						}
					}
					std::process::exit(if report.ok { 0 } else { 1 });
				}
				Err(e) => {
					if self.flowgraph_test_json {
						println!(
							"{}",
							serde_json::to_string_pretty(&serde_json::json!({
								"ok": false,
								"root": root.display().to_string(),
								"error": e.to_string(),
							}))?
						);
					} else {
						log::error!("Flowgraph fixture 失敗: {e}");
					}
					std::process::exit(1);
				}
			}
		}

		if let Some(root_dir) = &self.flowgraph_test_root {
			let root = std::path::PathBuf::from(root_dir);
			match crate::flowgraph::fixture_runner::run_fixture_suite_report(&root).await {
				Ok(report) => {
					if self.flowgraph_test_json {
						println!("{}", serde_json::to_string_pretty(&report)?);
					} else {
						let status = if report.ok { "OK" } else { "FAILED" };
						println!("Flowgraph fixture suite {status}: {}", report.root);
						println!("  fixtures: {} total / {} failed", report.fixture_count, report.failed_fixtures);
						println!("  tests: {} total / {} failed", report.test_count, report.failed_tests);
						println!("  triggers: {}", report.trigger_count);
						println!("  effects: {}", report.effect_count);
						println!("  capabilities: {}", format_suite_capability_counts(&report.reports));
						for fixture in &report.reports {
							if !fixture.ok {
								println!("    FAIL {}", fixture.root);
								for test in &fixture.tests {
									if !test.ok {
										println!("      {} :: {}", test.file, test.name);
										for failure in &test.failures {
											println!("        - {failure}");
										}
									}
								}
							}
						}
						for error in &report.errors {
							println!("    ERROR {}: {}", error.root, error.error);
						}
					}
					std::process::exit(if report.ok { 0 } else { 1 });
				}
				Err(e) => {
					if self.flowgraph_test_json {
						println!(
							"{}",
							serde_json::to_string_pretty(&serde_json::json!({
								"ok": false,
								"root": root.display().to_string(),
								"error": e.to_string(),
							}))?
						);
					} else {
						log::error!("Flowgraph fixture suite 失敗: {e}");
					}
					std::process::exit(1);
				}
			}
		}

		Ok(())
	}
}

fn format_suite_capability_counts(reports: &[crate::flowgraph::fixture_runner::FixtureRunReport]) -> String {
	let mut counts = std::collections::BTreeMap::new();
	for report in reports {
		for (capability, count) in &report.capability_summary.capability_counts {
			*counts.entry(capability.clone()).or_insert(0) += count;
		}
	}
	format_capability_counts(&counts)
}

fn format_capability_counts(counts: &std::collections::BTreeMap<String, usize>) -> String {
	if counts.is_empty() {
		return "none".into();
	}
	counts
		.iter()
		.map(|(capability, count)| format!("{capability}={count}"))
		.collect::<Vec<_>>()
		.join(", ")
}
