pub(crate) mod ai;
mod app_core;
mod args;
pub(crate) mod bridges;
mod conf;
mod control_events;
pub(crate) mod datetime;
mod desktop;
mod error;
mod libretranslate;
mod logger;
pub(crate) mod managed_app;
pub(crate) mod migrate;
mod motion;
mod processor;
mod resource;
mod runtime;
pub(crate) mod shutdown;
mod state;
pub(crate) mod twitch;
mod twitch_oauth_sessions;

pub mod flowgraph;
pub mod utility;
pub mod web_interface;

pub use crate::{
	args::Args,
	conf::Conf,
	conf::*,
	error::{Error, Result},
	processor::*,
	state::{Attachment, ChannelData, ChannelDatum, DataSource, SharedChannelData, SharedState, State},
};
pub use std::sync::Arc;
pub use tokio::sync::RwLock;

use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player};
use tokio::sync::Mutex;

/// `MixerDeviceSink`（ストリーム）と `Player` をまとめて保持する。rodio 0.22 では前者を先に drop してはいけない。
pub struct AudioSink {
	_device: MixerDeviceSink,
	pub player: Player,
}

impl AudioSink {
	pub(crate) fn open_default() -> crate::Result<Self> {
		let device = DeviceSinkBuilder::open_default_sink()?;
		let player = Player::connect_new(device.mixer());
		Ok(Self { _device: device, player })
	}
}

pub type SharedAudioSink = Arc<Mutex<AudioSink>>;
impl std::fmt::Debug for AudioSink {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "AudioSink")
	}
}

async fn run_with_standard_bootstrap() -> Result<()> {
	let core = boot_with_standard_bootstrap().await?;
	core.serve().await?;
	core.cleanup().await?;

	Ok(())
}

async fn boot_with_standard_bootstrap() -> Result<app_core::AppCore> {
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

/// 互換入口。移行期間中は CLI runner と同じ動きをする。
pub async fn run() -> Result<()> {
	run_cli().await
}

/// コンソール付きの玄人・開発者向け runner。
pub async fn run_cli() -> Result<()> {
	run_with_standard_bootstrap().await
}

/// Tauri / tray を使わない desktop runner 入口。
///
/// 非 Windows fallback として CLI と同じ runtime を起動する。
pub async fn run_desktop_headless() -> Result<()> {
	run_with_standard_bootstrap().await
}

/// system tray 付きの desktop runner。
///
/// Windows では tray menu から Tauri WebView GUI を開き、`ShutdownBroker` 経由で VAC を終了する。
/// それ以外の OS は Tauri shell 導入まで headless desktop runner と同じ起動にする。
pub fn run_desktop() -> Result<()> {
	desktop::run()
}
