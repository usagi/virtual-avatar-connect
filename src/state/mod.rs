//! ランタイム共有状態。`flowgraph` / `ai` / `bridges` 等と接続する。
//!
//! ## レイヤ（Step 4 メモ）
//!
//! Control イベント型は [`crate::control_events`] にあり、本モジュールは `web_interface` に依存しない。

mod channel_attach;
mod channel_datum;
mod runtime_mode_apply;
mod speech_floor;

pub use channel_attach::{Attachment, DataSource};
pub use runtime_mode_apply::apply_runtime_mode_change;
pub use channel_datum::{ChannelData, ChannelDatum, SharedChannelData};
pub use speech_floor::SpeechFloorManager;

use crate::ai::Observation;
use crate::conf::Twitch;
use crate::flowgraph::{shared_flowgraph_new, SharedFlowgraph};
use crate::runtime::RuntimePaths;
use crate::shutdown::ShutdownBroker;
use crate::control_events::{ChannelDatumPhase, ControlEvent};
use crate::twitch_oauth_sessions::OAuthSessions;
use crate::{Arc, Conf, RwLock, SharedAudioSink};
use anyhow::Result;
use std::collections::{HashSet, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use tokio::sync::broadcast;

impl ControlEvent {
	/// `ChannelDatum` から `ChannelDatum` バリアントを組む。flags は HashSet をそのまま Vec に。
	pub fn from_channel_datum(phase: ChannelDatumPhase, cd: &ChannelDatum) -> Self {
		Self::ChannelDatum {
			phase,
			id: cd.get_id(),
			channel: cd.channel.clone(),
			content: cd.content.clone(),
			flags: cd.flags.iter().cloned().collect(),
			datetime: cd.get_datetime().to_string(),
		}
	}
}

pub type SharedState = Arc<RwLock<State>>;

const DEFAULT_STATE_DATA_CAPACITY: usize = 256;
/// AI サービスに通知する観測イベント（`ChannelDatum` の発行）のブロードキャストキャパシティ。
/// 観測を取りこぼしてもログに `Lagged` が出るだけで致命的ではないが、短時間に大量に発行される配信負荷を考慮し広めに取る。
const AI_OBSERVATION_CHANNEL_CAPACITY: usize = 1024;
/// Control API WebSocket (`/api/v1/control/events`) 用のイベント配信キャパシティ。
/// 接続されている GUI が遅い場合に `Lagged` 通知で補足するため、こちらも余裕をもって取る。
const CONTROL_EVENT_CHANNEL_CAPACITY: usize = 2048;
/// δ-9 Part E: Flowgraph の `channel.subscribe` ingress が購読する `ChannelDatum` ブロードキャストの容量。
/// 複数 bridge が独立して受信するため、遅い購読者がいても後続を取りこぼさないよう広めに取る。
const CHANNEL_DATUM_BROADCAST_CAPACITY: usize = 2048;

/// `respect_speech_floor` が設定されているとき、対応する speech floor が空くまで非同期待機する（スキップしない）。
pub async fn wait_respect_speech_floor_key(state: &SharedState, key: Option<&str>) {
	let key = key.map(str::trim).filter(|s| !s.is_empty());
	let Some(k) = key else {
		return;
	};
	state.read().await.speech_floor.wait_until_free(k).await;
}

/// 各 AI persona のランタイム状態。`AiService::run` / `run_heartbeat` は起動時にこの `paused` を共有し、
/// `true` の間は新規 Datum 処理・heartbeat 評価をスキップする（既に走っている応答は中断しない soft suspend）。
#[derive(Debug, Clone)]
pub struct AiRuntime {
	pub persona_id: Option<String>,
	pub paused: Arc<AtomicBool>,
	/// Phase VI-α-4: Control API からの hot-reload ハンドル。`spawn_all` が AiService 生成直後に登録する。
	/// cycle 回避のため、本ハンドルは AiService の一部フィールド (`persona` / `request_template` / `decision`)
	/// しか指さない（`state: SharedState` を参照しない）。
	pub reload_handle: Option<crate::ai::AiReloadHandle>,
}

impl AiRuntime {
	pub fn new(persona_id: Option<String>) -> Self {
		Self {
			persona_id,
			paused: Arc::new(AtomicBool::new(false)),
			reload_handle: None,
		}
	}

	pub fn is_paused(&self) -> bool {
		self.paused.load(Ordering::SeqCst)
	}

	pub fn set_paused(&self, v: bool) {
		self.paused.store(v, Ordering::SeqCst);
	}
}

#[derive(Debug, Clone)]
pub struct State {
	pub state_data_capacity: usize,
	pub state_data_path: Option<PathBuf>,
	pub state_data_auto_save: bool,
	pub state_data_pretty: bool,
	pub channel_data: SharedChannelData,
	/// 起動済み AI persona のランタイム情報。`ai::spawn_all` が登録する。
	pub ai_runtimes: Arc<RwLock<Vec<AiRuntime>>>,
	pub audio_sink: SharedAudioSink,
	/// 《Voice》発話中などの speech floor（キーごとに `Notify` で待機解除）
	pub speech_floor: Arc<SpeechFloorManager>,
	/// 《LibreTranslate》: 自動起動した子プロセスの停止用
	pub libretranslate: crate::libretranslate::SharedRuntime,
	/// 添付ファイル用のランタイム一時ディレクトリ情報（セッション単位）。
	pub runtime_paths: Arc<RuntimePaths>,
	/// AI サービスへ観測イベントを流す `broadcast` 送信側（Phase I で導入）。
	/// 受信側は `[[ai.personas]]` ごとに subscribe される常駐タスクのみ。
	pub ai_observation_tx: broadcast::Sender<Observation>,
	/// Control API WebSocket の配信チャネル（Phase VI-α-3）。
	/// 各 WS 接続が `subscribe()` で受信側を作り、切断で drop する使い方。受信者 0 でも `send()` は失敗しない設計にする。
	pub control_event_tx: broadcast::Sender<ControlEvent>,
	/// δ-9 Part E: `push_channel_datum` / `push_channel_datum_quiet` / `finalize_channel_datum_and_dispatch`
	/// で `ChannelDatum` を配信するブロードキャスト送信側。
	///
	/// `flowgraph.ingress.channel_subscribe` の bridge (src/bridges/channel_subscribe.rs) が subscribe して、
	/// マッチする datum を `TriggerEvent` 化して Flowgraph ワーカーへ投入する。受信者 0 でも `send()` は失敗しない。
	pub channel_datum_tx: broadcast::Sender<ChannelDatum>,
	/// Phase V: Twitch outbound / モデレーション系プロセッサーが参照する `[twitch]` 全体のスナップショット。
	/// `conf.twitch` が `Some` のときだけセットされる。Arc 共有で低コスト。
	pub twitch: Option<Arc<Twitch>>,
	/// Phase VI-α-5: Control API からの Twitch Device Code Flow セッション管理。
	/// broadcaster / moderator 各 1 本まで並行保持する（同一アカウントへの重複 start は既存セッションを冪等に返す）。
	pub twitch_oauth: Arc<OAuthSessions>,
	/// RM-3: 現在の Runtime Mode ID（Control API で更新可）。`None` のときは `conf.default_runtime_mode` を意味する。
	pub runtime_mode_id: std::sync::Arc<std::sync::RwLock<Option<String>>>,
	/// Phase VI-γ-1: この State を組み立てた conf の読み込み元パス（絶対パス寄りに正規化済み）。
	///
	/// `/api/v1/control/restart` が「現在の conf」を識別する、`/api/v1/control/profiles` が
	/// 同ディレクトリを走査する起点になる、といった運用に使う。`Conf.source_path` を clone して置く。
	pub conf_source_path: Option<PathBuf>,
	/// Phase VI-γ-2a: `[browser_source] document_root` のスナップショット。
	///
	/// `/api/v1/control/bos` が走査するルート。GUI Live タブの iframe 埋め込み用 URL の根拠。
	/// `lib.rs` の actix-web 側とは別経由で参照したいので、ここに持たせておく。
	pub browser_source_document_root: Option<PathBuf>,
	/// Phase VI-γ-2b: Managed App（`run_with` の GUI 管理版）レジストリ。
	///
	/// `spec`（conf から組み立てる静的定義）と `statuses`（監視タスクが書き込む現在状態）を 1 つの
	/// `RwLock` に束ねている。`/api/v1/control/managed_apps` API と監視タスクが共有する。
	/// `Arc<RwLock<_>>` にしておくのは `State` 自体の `RwLock::read` を取らなくても独立に操作できるようにするため。
	pub managed_apps: std::sync::Arc<tokio::sync::RwLock<crate::managed_app::ManagedAppRegistry>>,
	/// Phase V: Twitch IRC ingress でスキップする送信者 login の集合（小文字・正規化済み）。
	///
	/// AI → `twitch-out` → 自 chat → ingress という **エコー**を防ぐ目的で、以下から充填される:
	///   1. `[twitch.ignore_logins]` の明示指定
	///   2. `[twitch.moderator].login`（設定されていれば）
	///   3. `TwitchOut` 初期化時に Helix `validate` で解決した自 bot の login
	/// ingress 起動後も動的に追加されうるため `Arc<RwLock<..>>`。
	pub twitch_ignore_logins: Arc<RwLock<HashSet<String>>>,

	/// Phase δ-6: Flowgraph ランタイム（ロード済み program + 診断）。
	///
	/// `conf.flowgraph_dir` が指すディレクトリを起動時にロードした結果を保持する。
	/// 本フェーズでは **GUI / Control API の受け皿**として働き、実行は V1 processor 側が担う。
	/// δ-8 以降で `run_forever` を接続する。未設定 / ディレクトリ非存在時は `None` が入る。
	pub flowgraph: SharedFlowgraph,

	/// ζ-3: Flowgraph bridges (twitch / twitch_eventsub / voice / channel_subscribe) のライフサイクル束。
	///
	/// 初回起動は `lib.rs::run` が [`crate::bridges::spawn_all_from_state`] で populate する。
	/// flowgraph reload 時は `web_interface::control::flowgraph::reload_runtime` が旧ハンドルを取り出して
	/// [`crate::bridges::BridgeHandles::finish_all`] で停止し、新 runtime 上で再 spawn して差し替える。
	///
	/// - `Arc<tokio::sync::Mutex<...>>` なのは、respawn 中に他のリクエストが触りに来ないよう
	///   独立ロックを取るため（`SharedState::write()` を長時間保持したくない）。
	/// - web_input は actix route 登録制で hot-swap 不可なので、`finish_all` 対象には含まない。
	///   差分は `web_input_snapshot` で検出し、`ControlEvent::RestartRecommended` で GUI に促す。
	pub bridge_handles: std::sync::Arc<tokio::sync::Mutex<crate::bridges::BridgeHandles>>,

	/// Flowgraph `tts.speak` + `engine=voicepeak` で `endpoint` が空のときに埋める CLI パス（`[voicepeak]` + OS 既定）。
	pub voicepeak_fallback_exe: String,

	/// Phase ε-1: 統合シャットダウンブローカー。
	///
	/// Ctrl+C / `POST /api/v1/control/shutdown` / 致命的エラー の全てをここに集約する。
	/// 登録タイミングは `lib.rs::run()` 冒頭で `ShutdownBroker::new()` を生成し、
	/// `State::new()` の引数として受け取る。
	pub shutdown: Arc<ShutdownBroker>,
}

impl State {
	pub async fn new(conf: &Conf, audio_sink: SharedAudioSink, shutdown: Arc<ShutdownBroker>) -> Result<SharedState> {
		let channel_data = match &conf.state_data_path {
			Some(path) => load_channel_data(path).await?,
			None => Arc::new(RwLock::new(VecDeque::new())),
		};
		log::trace!("ChannelData の初期化が完了しました。");

		let runtime_paths = Arc::new(RuntimePaths::init(conf)?);

		let (ai_observation_tx, _rx) = broadcast::channel::<Observation>(AI_OBSERVATION_CHANNEL_CAPACITY);
		let (control_event_tx, _rx) = broadcast::channel::<ControlEvent>(CONTROL_EVENT_CHANNEL_CAPACITY);
		let (channel_datum_tx, _rx) = broadcast::channel::<ChannelDatum>(CHANNEL_DATUM_BROADCAST_CAPACITY);

		let twitch_snapshot = conf.twitch.as_ref().map(|t| Arc::new(t.clone()));

		// ingress で無視する login 集合を、`[twitch.ignore_logins]` と `[twitch.moderator].login` から先に充填しておく。
		// `TwitchOut` 初期化時に Helix validate 経由で解決した自 bot login もここに追加される。
		let mut twitch_ignore_set: HashSet<String> = HashSet::new();
		if let Some(t) = conf.twitch.as_ref() {
			if let Some(list) = t.ignore_logins.as_ref() {
				for s in list {
					let n = s.trim().trim_start_matches('#').to_lowercase();
					if !n.is_empty() {
						twitch_ignore_set.insert(n);
					}
				}
			}
			if let Some(ml) = t.moderator.as_ref().and_then(|m| m.login.as_ref()) {
				let n = ml.trim().trim_start_matches('#').to_lowercase();
				if !n.is_empty() {
					twitch_ignore_set.insert(n);
				}
			}
		}
		let twitch_ignore_logins = Arc::new(RwLock::new(twitch_ignore_set));

		let voicepeak_fallback_exe = crate::conf::resolve_voicepeak_fallback_executable(conf);

		let runtime_mode_id = std::sync::Arc::new(std::sync::RwLock::new(conf.default_runtime_mode.clone()));
		let runtime_mode_id_for_flowgraph = runtime_mode_id.clone();

		// δ-6: Flowgraph ランタイム共有ハンドル。opt-in なので `flowgraph_dir` が None / 非存在なら `None` 保持。
		// δ-9 Part A: ロード + 実行ワーカー spawn は `State` 生成後に遅延実行する（`Weak<RwLock<State>>` が必要なため）。
		let flowgraph = shared_flowgraph_new();

		let state = Arc::new(RwLock::new(Self {
			state_data_capacity: conf.state_data_capacity.unwrap_or(DEFAULT_STATE_DATA_CAPACITY),
			state_data_path: conf.state_data_path.clone(),
			state_data_auto_save: conf.state_data_auto_save.unwrap_or(false),
			state_data_pretty: conf.state_data_pretty.unwrap_or(false),
			channel_data,
			ai_runtimes: Arc::new(RwLock::new(Vec::new())),
			audio_sink,
			speech_floor: Arc::new(SpeechFloorManager::new()),
			libretranslate: crate::libretranslate::runtime_new(),
			runtime_paths,
			ai_observation_tx,
			control_event_tx,
			channel_datum_tx,
			twitch: twitch_snapshot,
			twitch_oauth: OAuthSessions::new(),
			runtime_mode_id,
			conf_source_path: conf.source_path.clone(),
			browser_source_document_root: conf.browser_source.as_ref().and_then(|b| b.document_root.clone()),
			managed_apps: std::sync::Arc::new(tokio::sync::RwLock::new(crate::managed_app::ManagedAppRegistry::from_conf(conf))),
			twitch_ignore_logins,
			flowgraph,
			bridge_handles: std::sync::Arc::new(tokio::sync::Mutex::new(crate::bridges::BridgeHandles::empty())),
			voicepeak_fallback_exe,
			shutdown,
		}));
		log::trace!("State の生成が完了しました。");

		// Phase δ-9 D.2/D.3 (v1.0): V1 `[[processors]]` は全廃。`conf.processors` が残っていても warning を出すだけ。
		// Part E.2 (v1.0.1-): voice も Flowgraph に完全移行。残存は twitch (IRC/EventSub) のみ。
		if !conf.processors.is_empty() {
			log::warn!(
				"conf.processors ({} 件) が設定されていますが、V1 `[[processors]]` は v1.0 で廃止されました。\
				 transform 系 / voice は全て Flowgraph ノードに移行してください。\
				 twitch (IRC/EventSub) のみ当面 `[[processors]]` 経由でも起動します（Part E 次回で bridges に移管予定）。",
				conf.processors.len()
			);
		}

		// δ-9 Part A: Flowgraph ロード + ワーカー spawn。`State` 全体への `Weak` を ExecCtx に渡して
		// `channel.emit` 等が `State::push_channel_datum` を呼べるようにする。
		if let Some(root) = conf.flowgraph_dir.as_deref() {
			let (audio_sink_clone, flowgraph_arc, mode_for_gate) = {
				let s = state.read().await;
				let m = s.runtime_mode_id.read().ok().and_then(|g| g.clone());
				(s.audio_sink.clone(), s.flowgraph.clone(), m)
			};
			let state_weak = Arc::downgrade(&state);
			let rt = crate::flowgraph::FlowgraphRuntime::load_and_spawn(
				root,
				state_weak,
				Some(audio_sink_clone),
				Some(conf),
				mode_for_gate.as_deref(),
				Some(runtime_mode_id_for_flowgraph),
			);
			crate::flowgraph::runtime::log_load_outcome(&rt);
			*flowgraph_arc.write().await = Some(rt);
		}
		Ok(state)
	}

	pub async fn rfind_channel_datum(&self, id: u64) -> Option<ChannelDatum> {
		let channel_data = self.channel_data.read().await;
		let datum = channel_data.iter().rfind(|cd| cd.get_id() == id);
		datum.cloned()
	}

	pub async fn push_channel_data(&self, channel_data: ChannelData) {
		for cd in channel_data.into_iter() {
			self.push_channel_datum(cd).await;
		}
	}

	pub async fn push_channel_datum(&self, cd: ChannelDatum) {
		let id = cd.get_id();
		let channel_from = cd.channel.clone();

		log::trace!("ChannelDatum を追加します: {:?}", cd);
		self.append_channel_datum_trim_capacity(cd).await;

		// Control API WS 購読者へ通知。δ-9 D.3 (v1.0) で V1 Processor dispatch は除去済み。
		self.emit_channel_datum_event(ChannelDatumPhase::Pushed, id).await;
		self.broadcast_channel_datum(id).await;
		self.notify_ai_observation(id, &channel_from);

		if self.state_data_auto_save {
			log::trace!("state_data_auto_save が有効になっているため、保存処理を行います。");
			self.save().await.unwrap();
		}
		log::trace!("push_channel_datum の処理が完了しました。")
	}

	/// `channel_data` にだけ追加し **Processor は実行しない**（OpenAI ストリーミングの中間表示用）。
	pub async fn push_channel_datum_quiet(&self, cd: ChannelDatum) {
		let id = cd.get_id();
		log::trace!("ChannelDatum を quiet に追加します（Processor は未実行）: {:?}", cd);
		self.append_channel_datum_trim_capacity(cd).await;
		// GUI にはストリーム中間も見えた方が嬉しいので PushedQuiet で通知する。
		self.emit_channel_datum_event(ChannelDatumPhase::PushedQuiet, id).await;
		// δ-9 Part E: Flowgraph channel.subscribe にも quiet push を届ける（is_final=false なので
		// require_final フィルタのある購読者には届かない）。
		self.broadcast_channel_datum(id).await;
	}

	/// `id` に一致する `ChannelDatum` の `content` を置き換える（ストリーミング追記のたびに呼ぶ）。
	pub async fn update_channel_datum_content_by_id(&self, id: u64, content: String) -> bool {
		let len = content.chars().count();
		let updated = {
			let mut channel_data = self.channel_data.write().await;
			let mut hit = false;
			for cd in channel_data.iter_mut().rev() {
				if cd.get_id() == id {
					cd.content = content;
					log::trace!("ChannelDatum id={} の content を更新しました（len={}）。", id, len);
					hit = true;
					break;
				}
			}
			hit
		};
		if updated {
			self.emit_channel_datum_event(ChannelDatumPhase::Updated, id).await;
		} else {
			log::warn!("update_channel_datum_content_by_id: id={} が見つかりませんでした。", id);
		}
		updated
	}

	/// 内容を確定し `is_final` と追加フラグを付けたうえで、`push_channel_datum` と同等の Processor 連鎖と auto_save を行う。
	pub async fn finalize_channel_datum_and_dispatch(&self, id: u64, content: String, processor_marker_flag: &str) {
		let channel_from = {
			let mut channel_data = self.channel_data.write().await;
			let Some(cd) = channel_data.iter_mut().rev().find(|d| d.get_id() == id) else {
				log::error!("finalize_channel_datum_and_dispatch: id={} が channel_data にありません。", id);
				return;
			};
			cd.content = content;
			cd.flags.insert(ChannelDatum::FLAG_IS_FINAL.to_string());
			cd.flags.insert(processor_marker_flag.to_string());
			cd.channel.clone()
		};
		log::trace!("ChannelDatum id={} を確定しました。", id);
		// δ-9 D.3 (v1.0): 旧 V1 Processor への dispatch は除去済み。Flowgraph 側は `ingress.*` 経由で
		// 別系統に流れているため、ここでは WS 購読者 / AI observation / Flowgraph channel 購読者の 3 系統に通知する。
		self.emit_channel_datum_event(ChannelDatumPhase::Finalized, id).await;
		self.broadcast_channel_datum(id).await;
		self.notify_ai_observation(id, &channel_from);
		let _ = &channel_from;
		if self.state_data_auto_save {
			log::trace!("state_data_auto_save が有効になっているため、保存処理を行います。");
			self.save().await.unwrap();
		}
	}

	async fn append_channel_datum_trim_capacity(&self, cd: ChannelDatum) {
		let mut channel_data = self.channel_data.write().await;
		channel_data.push_back(cd);
		if channel_data.len() > self.state_data_capacity {
			log::trace!("channel_data の容量が上限を超えたため、先頭の要素を削除します。");
			channel_data.pop_front();
		}
		log::trace!("channel_data の容量: {}", channel_data.len());
	}

	/// Control API WebSocket (`/api/v1/control/events`) に 1 件流す。受信者 0 件は正常系として飲み込む。
	///
	/// `cd` を `RwLock` 越しに読んで組み立てる。**呼び出し側が書き込みロックを保持したまま** 呼ぶとデッドロック
	/// もしくは無用なロック伸長を招くため、必ず write lock を解放してから呼ぶこと。
	async fn emit_channel_datum_event(&self, phase: ChannelDatumPhase, id: u64) {
		// 先に channel_data の read ロックで必要情報を取り、ロック外で send する。
		let payload = {
			let channel_data = self.channel_data.read().await;
			channel_data
				.iter()
				.rev()
				.find(|c| c.get_id() == id)
				.map(|cd| ControlEvent::from_channel_datum(phase, cd))
		};
		if let Some(ev) = payload {
			if let Err(e) = self.control_event_tx.send(ev) {
				// 受信者 0 件（GUI 未接続）は想定内。
				log::trace!("control_event_tx.send に失敗（受信者 0 件の可能性）: {e}");
			}
		}
	}

	/// δ-9 Part E: `channel_datum_tx` に現在格納されている `ChannelDatum` のクローンを送る。
	/// Flowgraph `channel.subscribe` bridge が subscribe して `TriggerEvent` に変換する。
	/// 受信者 0 件は正常系として trace ログにとどめる。
	async fn broadcast_channel_datum(&self, id: u64) {
		let payload = {
			let channel_data = self.channel_data.read().await;
			channel_data.iter().rev().find(|c| c.get_id() == id).cloned()
		};
		if let Some(cd) = payload {
			if let Err(e) = self.channel_datum_tx.send(cd) {
				log::trace!("channel_datum_tx.send: 受信者 0 件の可能性 ({e})");
			}
		}
	}

	/// `ai_observation_tx` に観測通知を送る。受信者がいなくてもエラーにしない（`[[ai.personas]]` 未設定時を許容）。
	fn notify_ai_observation(&self, id: u64, channel: &str) {
		// 受信者が 0 件なら SendError になるが、AI サービスが居ないだけの正常系なので debug ログのみに留める。
		if let Err(e) = self.ai_observation_tx.send(Observation {
			datum_id: id,
			channel: channel.to_string(),
		}) {
			log::trace!("AI observation 送信先が 0 件でした（`[[ai.personas]]` 未設定の可能性）: {:?}", e);
		}
	}

	// δ-9 D.2/D.3 (v1.0): V1 `Processor` trait / `dispatch_processors_for_incoming` は全廃。
	// 旧 `[[processors]]` パイプラインに相当する処理は Flowgraph ノードチェーン側へ移管済み。
	// ingress → push_channel_datum で届いた Datum に対しては、Flowgraph `ingress.*` ノード / WS 購読者
	// のみが反応する。

	pub async fn save(&self) -> Result<()> {
		if self.state_data_path.is_none() {
			log::warn!("state_data_path が設定されていないため、保存処理を行えませんでした。");
			return Ok(());
		}
		let path = self.state_data_path.as_ref().unwrap();
		save_channel_data(path, &self.channel_data).await?;
		Ok(())
	}

	pub async fn load(&mut self) -> Result<()> {
		let channel_data = load_channel_data(self.state_data_path.as_ref().unwrap()).await?;
		self.channel_data = channel_data;
		Ok(())
	}
}

async fn load_channel_data<P: AsRef<Path>>(path: P) -> Result<SharedChannelData> {
	let path = path.as_ref();
	if !path.exists() {
		log::warn!(
			"指定されたファイルが存在しないため、読み込み処理を行えませんでした。 path = {:?}",
			path
		);
		return Ok(Arc::new(RwLock::new(VecDeque::new())));
	}
	let serialized_channel_data = tokio::fs::read_to_string(path).await?;
	let channel_data = ron::de::from_str(&serialized_channel_data)?;
	Ok(Arc::new(RwLock::new(channel_data)))
}

async fn save_channel_data<P: AsRef<Path>>(path: P, channel_data: &SharedChannelData) -> Result<()> {
	let channel_data = channel_data.read().await;
	let channel_data = channel_data.iter().collect::<VecDeque<_>>();
	let serialized_channel_data = ron::ser::to_string_pretty(&channel_data, ron::ser::PrettyConfig::new().indentor(" ".to_string()))?;
	tokio::fs::write(path, serialized_channel_data).await?;
	Ok(())
}
