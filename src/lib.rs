pub(crate) mod ai;
mod args;
pub(crate) mod bridges;
mod conf;
pub(crate) mod datetime;
mod error;
mod logger;
mod libretranslate;
pub(crate) mod managed_app;
pub(crate) mod migrate;
mod processor;
mod resource;
mod runtime;
pub(crate) mod shutdown;
mod state;
pub(crate) mod twitch;

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

use actix_files::Files;
use actix_web::web::Data;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player};
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};

/// `MixerDeviceSink`（ストリーム）と `Player` をまとめて保持する。rodio 0.22 では前者を先に drop してはいけない。
pub struct AudioSink {
 _device: MixerDeviceSink,
 pub player: Player,
}

impl AudioSink {
 fn open_default() -> crate::Result<Self> {
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

pub async fn run() -> Result<()> {
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

 // Phase ε-1: シャットダウンブローカーを 1 本立てる。
 // 以下の 3 経路を本ブローカーに集約する:
 //   1. Ctrl+C（`spawn_ctrl_c_listener`）
 //   2. `POST /api/v1/control/shutdown`（`State.shutdown` 経由）
 //   3. 致命的エラー（今後 `run()` 内で明示的に trigger する余地として）
 let shutdown = shutdown::ShutdownBroker::new();
 shutdown::spawn_ctrl_c_listener(shutdown.clone());

 // 共有ステートを作成
 let state = State::new(&conf, audio_sink, shutdown.clone()).await?;

 // 常駐 AI サービス（[[ai.personas]]）を起動。`ai_observation_tx` 経由で Datum を各 persona に届ける。
 // Phase IV: Twitch Helix を叩くツール（`vac_twitch_set_category` 等）のために `twitch.eventsub` を共有する。
 // Phase V-c: モデレーション系ツール（BAN / timeout / chat_say / delete_message）のため `twitch.moderator` も共有。
 let ai_tx = state.read().await.ai_observation_tx.clone();
 let twitch_eventsub_for_ai = conf
  .twitch
  .as_ref()
  .and_then(|t| t.eventsub.as_ref())
  .map(|e| std::sync::Arc::new(e.clone()));
 let twitch_moderator_for_ai = conf
  .twitch
  .as_ref()
  .and_then(|t| t.moderator.as_ref())
  .map(|m| std::sync::Arc::new(m.clone()));
 let twitch_default_broadcaster_login = conf.twitch.as_ref().map(|t| {
  t.eventsub
   .as_ref()
   .and_then(|e| e.broadcaster_login.clone())
   .unwrap_or_else(|| t.username.clone())
 });
 let ai_handles = ai::spawn_all(
  &conf.ai,
  state.clone(),
  ai_tx,
  twitch_eventsub_for_ai,
  twitch_moderator_for_ai,
  twitch_default_broadcaster_login,
 )
 .await?;

 // Phase ε-1: 旧 `tokio::signal::ctrl_c()` を LibreTranslate 用に個別 spawn していた task は
 // `ShutdownBroker` に一本化したため削除した。停止要求は全て broker 経由で伝搬し、
 // LibreTranslate の stop は `run_services` から戻ったあとの cleanup フェーズで実行する。

 // Phase VI-γ-2b: Managed App 監視タスクを起動。
 // registry は State から Arc で共有されており、API 側の書き込みとも同じ RwLock を参照する。
 // Phase ε-1: shutdown broker を渡し、停止要求で即座に loop を抜けられるようにする。
 {
  let s = state.read().await;
  let registry = s.managed_apps.clone();
  let event_tx = s.control_event_tx.clone();
  let broker_for_monitor = shutdown.clone();
  tokio::spawn(async move {
   managed_app::run_monitor(registry, event_tx, broker_for_monitor).await;
  });
 }

 // δ-9 Part B/E: Flowgraph ブリッジの収集。ingress ノードごとに HTTP ルートやワーカーを準備する。
 // ζ-2c: V1 eventsub_loop スキップ判定にも使うので、V1 ingress::prepare より先に走らせる。
 let (flowgraph_bridges_catalog, flowgraph_trigger, channel_datum_tx) = {
  let s = state.read().await;
  let fg = s.flowgraph.read().await;
  let tx = s.channel_datum_tx.clone();
  if let Some(rt) = fg.as_ref() {
   (bridges::collect_all(&rt.node_meta), rt.trigger(), tx)
  } else {
   (bridges::BridgeCatalog::default(), None, tx)
  }
 };

 // ζ-2c: Flowgraph ingress が購読する broadcaster_login（正規化済み）の集合を作り、
 // V1 eventsub_loop に渡して自動スキップ判定に使わせる。
 let v2_eventsub_skip_broadcasters = {
  let username_fallback = conf
   .twitch
   .as_ref()
   .map(|t| t.username.clone())
   .unwrap_or_default();
  bridges::twitch_eventsub::v1_skip_broadcaster_logins(
   &flowgraph_bridges_catalog.twitch_eventsub,
   &username_fallback,
  )
 };

 let (ingress_handles, web_input_registry) = processor::ingress::prepare(
  &conf,
  state.clone(),
  &v2_eventsub_skip_broadcasters,
 )
 .await?;

 // ζ-3: 起動後の bridges ライフサイクルは `State.bridge_handles` に預ける。
 // reload 経路と同じ [`bridges::spawn_all_from_state`] を使うことで初回と reload の再配線コードを共通化する。
 let initial_bridges = bridges::spawn_all_from_state(&state, &channel_datum_tx).await;
 // actix ルート用のスナップショットを取ってから State に譲渡する。
 let flowgraph_web_input_endpoints =
  std::sync::Arc::new(initial_bridges.web_input_snapshot.clone());
 {
  let s = state.read().await;
  let mut slot = s.bridge_handles.lock().await;
  *slot = initial_bridges;
 }
 let flowgraph_trigger_data: std::sync::Arc<Option<flowgraph::node::TriggerHandle>> =
  std::sync::Arc::new(flowgraph_trigger.clone());

 // Control API (Phase VI-α): 認証ポリシーとトークンを解決してから actix にぶら下げる。
 // ここで一度生成・書き出しまで済ませて、以降はランタイム通じて参照するだけにする。
 let control_api_runtime = web_interface::control::ControlApiRuntime::init(&conf, &state).await?;
 log::info!(
  "《ControlAPI》 認証ポリシー: loopback={}, non_loopback={} (token_source={:?})",
  if control_api_runtime.require_token_for_loopback { "token-required" } else { "allow" },
  if control_api_runtime.require_token_for_non_loopback { "token-required" } else { "allow" },
  control_api_runtime.token_source
 );
 if let Some(path) = control_api_runtime.written_token_file.as_ref() {
  log::info!(
   "《ControlAPI》 自動生成トークンを書き出しました: {:?}（GUI クライアントはこのファイルを読み取って Authorization: Bearer <token> に使う）",
   path
  );
 }

 // 通常の動作モードで実行。内部で `disable_signals()` + `ServerHandle::stop(true)` を使い、
 // `shutdown` ブローカー経由でのみ actix を停止する。
 run_services(
  conf,
  state.clone(),
  web_input_registry,
  control_api_runtime,
  flowgraph_web_input_endpoints,
  flowgraph_trigger_data,
  shutdown.clone(),
 )
 .await?;

 // Phase ε-1: cleanup フェーズ。順序は重要:
 //   1. ManagedApp（run_with 子プロセス群）を graceful 停止してから
 //   2. bridges / ingress_handles / ai / libretranslate を順に畳む
 // ManagedApp を先に落とす理由は、子プロセスが残っていると本プロセスの stdio ハンドルが
 // 解放されず、Windows で親プロセスが exit できなくなるケースがあるため。
 let managed_stop = {
  let s = state.read().await;
  let registry = s.managed_apps.clone();
  // Phase ε-2: 各 entry の shutdown 設定（action / method / grace_ms）は spec に埋め込み済み。
  // OBS / CoeiroInk のように終了確認ダイアログを出すアプリは conf.toml で
  // `shutdown = { action = "close_only", grace_ms = 30000 }` を指定してもらう想定。
  // 既定は `CloseAndWait` + `Syscommand` + `grace_ms=10000`。
  log::info!("《Shutdown》 ManagedApp 全停止を試行します（entry ごとの shutdown cfg を使用）。");
  managed_app::stop_all_graceful(&registry).await
 };
 for (id, outcome) in &managed_stop {
  log::info!(
   "《Shutdown》 ManagedApp 停止結果 id={} closed_windows={} terminated_pids={}",
   id,
   outcome.closed_windows,
   outcome.terminated_pids
  );
 }

 // ζ-3: bridges の後処理は `State.bridge_handles` に集約。reload と同じ経路で閉じる。
 {
  let handles_arc = state.read().await.bridge_handles.clone();
  let mut slot = handles_arc.lock().await;
  let taken = std::mem::replace(&mut *slot, bridges::BridgeHandles::empty());
  taken.finish_all().await;
 }
 for h in ingress_handles.eventsub {
  h.abort();
 }
 for h in ai_handles {
  h.abort();
 }

 // LibreTranslate は ManagedApp 側の stop_entry_graceful で畳まれているはずだが、
 // VAC が直接握っている `SharedRuntime` の handle 側も念のため stop して子プロセスの
 // 待受け口を綺麗に閉じる（run_with に載せず個別起動しているケースでもここで落ちる）。
 {
  let s = state.read().await;
  let fut = async {
   s.libretranslate.lock().await.stop().await;
  };
  if tokio::time::timeout(std::time::Duration::from_secs(5), fut).await.is_err() {
   log::warn!("《Shutdown》 LibreTranslate.stop() が 5 秒以内に完了しませんでした。続行します。");
  }
 }

 log::info!("《Shutdown》 cleanup 完了。プロセスを終了します。");
 Ok(())
}

async fn run_services(
 conf: Conf,
 state: SharedState,
 web_input_registry: Arc<web_interface::web_input::WebInputRegistry>,
 control_api_runtime: web_interface::control::ControlApiRuntime,
 flowgraph_web_input_endpoints: Arc<Vec<bridges::web_input::FlowgraphWebInputEndpoint>>,
 flowgraph_trigger: Arc<Option<flowgraph::node::TriggerHandle>>,
 shutdown: Arc<shutdown::ShutdownBroker>,
) -> Result<()> {
 let workers = conf.get_workers();
 let web_ui_address = conf.get_web_ui_address().to_string();
 let output_root = conf
  .browser_source
  .as_ref()
  .and_then(|b| b.document_root.clone())
  .unwrap_or_else(|| std::path::PathBuf::from("output"));
 let server = actix_web::HttpServer::new(move || {
  let state = state.clone();
  let conf = conf.clone();
  let reg = web_input_registry.clone();
  let output_root = output_root.clone();
  let control_api_runtime = control_api_runtime.clone();
  let fg_endpoints = flowgraph_web_input_endpoints.clone();
  let fg_trigger = flowgraph_trigger.clone();
  let app = actix_web::App::new()
   .wrap(actix_web::middleware::Condition::new(
    conf.web_ui_compress,
    actix_web::middleware::Compress::default(),
   ))
   .app_data(Data::new(state))
   .app_data(Data::new(web_interface::output::OutputPaths { root: output_root.clone() }))
   .app_data(Data::new(reg.clone()))
   .app_data(Data::new(control_api_runtime))
   .app_data(Data::new(fg_trigger.clone()))
   .configure({
    let r = reg.clone();
    move |cfg| {
     web_interface::web_input::register_web_input_routes(cfg, &r);
    }
   })
   .configure({
    let eps = fg_endpoints.clone();
    move |cfg| {
     bridges::web_input::register_routes(cfg, eps.as_ref());
    }
   })
   .configure(web_interface::control::register)
   .configure({
    let gui_dist_path = conf.gui_dist_path.clone();
    move |cfg| {
     web_interface::gui::register(cfg, gui_dist_path.as_deref());
    }
   })
   .service(web_interface::websocket)
   .service(web_interface::input::get_index)
   .service(web_interface::input::get_subfile)
   .service(web_interface::output::post)
   .service(web_interface::output::get_index)
   .service(web_interface::output::get_subfile)
   .service(Files::new("/browser-output", output_root.clone()))
   .service(web_interface::status::get)
   .service(web_interface::favicon);
  if let Some(web_ui_resources_path) = conf.web_ui_resources_path {
   app.service(Files::new("/resources", web_ui_resources_path))
  } else {
   app
  }
 })
 .workers(workers)
 .bind(web_ui_address)?
 .disable_signals()
 // Phase ε-1: 既定 30 秒だと GUI WS が活きたままの shutdown で
 // 「終了」ボタン押下後にブラウザを閉じるまで 30 秒固まる。
 // 実運用では 2 秒で十分に drain できる（HTTP リクエストは短命、
 // 生き残るのは WS と browser-output の poll くらい）。
 .shutdown_timeout(2)
 .run();

 // Phase ε-1: 自前でサーバーハンドルを保持し、broker 経由の停止要求で
 // `ServerHandle::stop(true)` を呼ぶ。これで actix 内部の SIGINT フックと broker 側の
 // Ctrl+C listener が二重に動いて競合することを避ける（broker が唯一の停止経路）。
 let server_handle = server.handle();
 let shutdown_for_server = shutdown.clone();
 tokio::spawn(async move {
  shutdown_for_server.wait().await;
  log::info!("《Shutdown》 actix HTTP サーバーへの graceful stop を要求します。");
  server_handle.stop(true).await;
 });

 server.await?;
 log::info!("《Shutdown》 actix HTTP サーバーが停止しました。");
 Ok(())
}
