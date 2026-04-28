use crate::{app_core, logger, Args, AudioSink, Conf, Result};
use std::sync::Arc;
use tokio::sync::Mutex;

pub(crate) async fn run_app_core_with_standard_bootstrap() -> Result<()> {
	let core = boot_app_core_with_standard_bootstrap().await?;
	core.serve().await?;
	core.cleanup().await?;

	Ok(())
}

pub(crate) async fn boot_app_core_with_standard_bootstrap() -> Result<app_core::AppCore> {
	// ロガーの実装を初期化
	logger::init();

	// 音声再生用の handle を生成（デバイスストリームは AudioSink 内で保持）
	let audio_sink = Arc::new(Mutex::new(AudioSink::open_default()?));

	// コマンドライン引数をパースし、ログレベルを更新
	let args = Args::new();
	// conf を必要としない特殊な動作モードを実行
	args.execute_special_modes_without_conf(audio_sink.clone()).await?;
	// 設定を読み込みし、ログレベルを更新
	let conf = Conf::new(&args)?;
	// conf を必要とする特殊な動作モードを実行
	args.execute_special_modes_with_conf(&conf).await?;

	// run_with 機能の実行
	conf.execute_run_with()?;

	app_core::AppCore::boot(conf, audio_sink).await
}
