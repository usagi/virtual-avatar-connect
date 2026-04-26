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

		Ok(())
	}
}
