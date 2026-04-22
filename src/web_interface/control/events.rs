//! Control API の WebSocket (`/api/v1/control/events`) で配信するイベント型。
//!
//! GUI / 外部ツールが現状把握に使う単一ストリーム。既存の `/websocket`（現行 UI 用）とは
//! **別系統**で、serde 表現・認証・配信方針を Control API 寄りに揃える。
//!
//! 設計メモ:
//!   - `broadcast::Sender<ControlEvent>` を [`crate::state::State`] に 1 本持たせ、
//!     `push_channel_datum` 等の節目で fire-and-forget で `send()` する。
//!   - 受信者が 0 件でも致命的ではない（送信エラーは debug ログのみ）。
//!   - 「新規」「ストリーム中の更新」「確定」「黙認（quiet）」を `phase` で区別する。
//!     GUI 側は phase で UI の描画方法を切り替える（例: Updated は既存行の in-place 更新）。

use serde::Serialize;

use crate::state::ChannelDatum;

/// `ProcessorInvoked` の結果種別。GUI はこれで pulse の色（OK / break / error）を変える。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessorInvocationOutcome {
 /// 正常終了。後続 processor も実行される。
 Continued,
 /// `CompletedAnd::Break` で後続をスキップ。
 Break,
 /// `process()` が `Err` を返した。
 Error,
}

/// ChannelDatum の発生フェーズ。WebSocket 購読者はこれで UI の描画方法を切り替える。
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelDatumPhase {
 /// 新規 `push_channel_datum`（Processor 連鎖あり）
 Pushed,
 /// `push_channel_datum_quiet`（Processor 未実行・ストリーミング中間表示用）
 PushedQuiet,
 /// `update_channel_datum_content_by_id`（同一 id の content 差し替え）
 Updated,
 /// `finalize_channel_datum_and_dispatch`（is_final 付与後の Processor 連鎖起点）
 Finalized,
}

/// WebSocket で配信する 1 イベント。serde で JSON 化して `ws::text` で流す。
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ControlEvent {
 /// ChannelDatum の発生・更新・確定イベント。
 ChannelDatum {
  phase: ChannelDatumPhase,
  id: u64,
  channel: String,
  content: String,
  flags: Vec<String>,
  datetime: String,
 },
 /// 購読が追いつかず `broadcast::Receiver::Lagged(n)` を食らったことの通知。
 Lagged {
  dropped: u64,
 },
 /// 接続維持用のキープアライブ。サーバー起点で定期的に送る。
 Heartbeat {
  /// サーバー現在時刻（RFC3339）。
  now: String,
 },
 /// Control API からの pause/resume 反映をストリームにも流す（GUI の即時反映用）。
 PauseState {
  /// true = paused, false = resumed
  paused: bool,
  /// "all" / "processors" / "ais" / "processor" / "ai"
  target: &'static str,
  processors_affected: Vec<usize>,
  ais_affected: Vec<usize>,
 },
 /// Phase VI-α-4: Control API `/reload` が成功したときの通知。
 /// `detail` はターゲット種別ごとに中身が違う:
 ///   - `ai_persona`: [`crate::ai::AiReloadReport`]
 ///   - `modify_files`: [`Vec<ModifyReport>`]
 Reloaded {
  /// "ai_persona" / "modify_files"
  target: &'static str,
  /// 個別 id が指定されていたか（全体対象の場合は None）
  id: Option<String>,
  /// ターゲット別の詳細（JSON 値でそのまま埋め込む）
  detail: serde_json::Value,
 },
 /// Phase VI-β-8: Processor が 1 つ発火したときの通知。
 ///
 /// `dispatch_processors_for_incoming` が `is_channel_from(channel_from)` で一致した
 /// Processor を `process()` 呼び出しするタイミングで 1 イベント送る。GUI の Svelte Flow
 /// パイプライン・ビューが当該ノードをパルスさせるのに使う。
 ///
 /// 注意: `process()` の内部でさらに新しい ChannelDatum が push されるとそちらからも
 /// `ChannelDatum` / `ProcessorInvoked` イベントが連鎖的に飛ぶ。この設計のまま素直に
 /// 時系列に並べて食わせれば、GUI は「発火の波」を描ける。
 ProcessorInvoked {
  /// `State::processors` 配列での位置。0-origin。
  index: usize,
  /// 設定ファイルの `feature` 名（例: `"modify"`, `"openai_chat"`）。
  feature: String,
  /// conf の `id`（省略可）。
  id: Option<String>,
  /// トリガとなった ChannelDatum の連番 ID。
  channel_datum_id: u64,
  /// トリガとなった ChannelDatum の channel。`is_channel_from` がマッチしたもの。
  trigger_channel: String,
  /// `process()` 呼び出しから戻るまでの経過時間（ms）。
  elapsed_ms: u64,
  /// 結果。
  outcome: ProcessorInvocationOutcome,
 },
 /// Phase VI-γ-1: 再起動 API `/restart` が成功した直後、現プロセスが `exit` する前に送る通知。
 ///
 /// GUI は WS が切れた後の自動再接続と組み合わせて「再起動中…」のトーストを出したり、
 /// プロファイル切替を完了表示に遷移させたりできる。新プロセスの WS が生きてから
 /// `Heartbeat` が来るまでの間を「restarting」状態として扱う想定。
 Restarting {
  /// 再起動に使った conf のパス（絶対）。現 conf 継続なら現 conf と同じ値。
  new_conf_path: String,
  /// spawn した新プロセスの PID（Windows では起動直後の値で、後続で子プロセスが再生成されることはない）。
  new_pid: u32,
  /// 現プロセスが exit するまでの猶予 ms。
  graceful_ms: u64,
  /// 現プロセス (= 再起動呼び出しを受けた側) の PID。
  current_pid: u32,
 },
 /// Phase δ-6: Flowgraph がリロードされたときの通知。
 ///
 /// トリガは 2 つ:
 ///   1. `POST/PUT/DELETE /flowgraph/file*` による write 操作（編集直後に発火）
 ///   2. 明示的な `POST /flowgraph/reload` 呼び出し（GUI の「Reload」ボタン / 外部エディタ編集後）
 ///
 /// GUI は本イベントを受けて `GET /flowgraph/diagnostics` / `GET /flowgraph/tree` を再取得し、
 /// パレット以外のノードビュー全体を更新する。diagnostics 本体は bandwidth 節約のため
 /// ここでは「件数サマリ」だけ載せて、詳細は pull する設計（spec §8.3 / §8.6）。
 FlowgraphReloaded {
  /// リロード対象ディレクトリ（絶対パス寄り）。
  root_dir: String,
  /// error 診断が 1 件もなければ true。
  ok: bool,
  error_count: usize,
  warning_count: usize,
  /// ロード済みノード件数（node_meta.len()）。
  node_count: usize,
 },

 /// Phase VI-γ-2b: Managed App の running / pids が変化したとき。
 ///
 /// `managed_app::run_monitor` タスクが定期 sysinfo ポーリングで前回と差分を検出したとき、
 /// またはユーザー操作 (`/managed_apps/.../start|stop`) 完了時に送る。
 /// GUI の右肩ドロワーが受動的に反映する。
 ManagedAppState {
  /// 対象 Managed App の ID（`run_with` の `id` or 自動採番）。
  id: String,
  /// このイベント時点で running か。
  running: bool,
  /// マッチした PID 一覧（代表ではなく全部、順不同）。
  pids: Vec<u32>,
  /// 観測時刻（RFC3339）。
  checked_at: String,
 },
 /// Phase VI-α-5: Twitch Device Code Flow のセッション状態が変化したとき。
 /// Control API `/oauth/twitch/{account}/start|cancel` や、ポーリングタスク完了時に送る。
 OAuthStatus {
  /// "broadcaster" / "moderator"
  account: &'static str,
  /// 新しい status
  status: super::oauth_twitch::OAuthSessionStatus,
  /// セッションのスナップショット（`device_code` のような秘匿情報は含まない）
  view: super::oauth_twitch::OAuthSessionView,
 },
}

impl ControlEvent {
 /// `ChannelDatum` から `ChannelDatum` バリアントを組む。flags は HashSet をそのまま Vec に。
 pub fn from_channel_datum(phase: ChannelDatumPhase, cd: &ChannelDatum) -> Self {
  Self::ChannelDatum {
   phase,
   id: cd.get_id(),
   channel: cd.channel.clone(),
   content: cd.content.clone(),
   flags: cd.flags.iter().cloned().collect(),
   datetime: cd.get_datetime().to_rfc3339(),
  }
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn serializes_channel_datum_with_phase_tag() {
  let ev = ControlEvent::ChannelDatum {
   phase: ChannelDatumPhase::Pushed,
   id: 42,
   channel: "ai".to_string(),
   content: "hi".to_string(),
   flags: vec!["is_final".to_string()],
   datetime: "2026-04-17T01:00:00Z".to_string(),
  };
  let s = serde_json::to_string(&ev).unwrap();
  // 外から見える形: kind / phase / id / channel / ...
  assert!(s.contains(r#""kind":"channel_datum""#), "missing kind tag: {s}");
  assert!(s.contains(r#""phase":"pushed""#), "missing phase: {s}");
  assert!(s.contains(r#""id":42"#), "missing id: {s}");
 }

 #[test]
 fn serializes_lagged_event() {
  let ev = ControlEvent::Lagged { dropped: 7 };
  let s = serde_json::to_string(&ev).unwrap();
  assert!(s.contains(r#""kind":"lagged""#), "bad: {s}");
  assert!(s.contains(r#""dropped":7"#), "bad: {s}");
 }

 #[test]
 fn serializes_managed_app_state_event() {
  let ev = ControlEvent::ManagedAppState {
   id: "coeiroink".to_string(),
   running: true,
   pids: vec![1234, 5678],
   checked_at: "2026-04-17T01:00:00Z".to_string(),
  };
  let s = serde_json::to_string(&ev).unwrap();
  assert!(s.contains(r#""kind":"managed_app_state""#), "bad: {s}");
  assert!(s.contains(r#""id":"coeiroink""#), "bad: {s}");
  assert!(s.contains(r#""running":true"#), "bad: {s}");
  assert!(s.contains(r#""pids":[1234,5678]"#), "bad: {s}");
 }

 #[test]
 fn serializes_pause_state_with_target_string() {
  let ev = ControlEvent::PauseState {
   paused: true,
   target: "all",
   processors_affected: vec![0, 1],
   ais_affected: vec![0],
  };
  let s = serde_json::to_string(&ev).unwrap();
  assert!(s.contains(r#""kind":"pause_state""#));
  assert!(s.contains(r#""target":"all""#));
  assert!(s.contains(r#""paused":true"#));
 }

 #[test]
 fn serializes_processor_invoked() {
  let ev = ControlEvent::ProcessorInvoked {
   index: 2,
   feature: "modify".to_string(),
   id: Some("pre-command".to_string()),
   channel_datum_id: 17,
   trigger_channel: "user".to_string(),
   elapsed_ms: 3,
   outcome: ProcessorInvocationOutcome::Continued,
  };
  let s = serde_json::to_string(&ev).unwrap();
  assert!(s.contains(r#""kind":"processor_invoked""#), "missing kind: {s}");
  assert!(s.contains(r#""index":2"#));
  assert!(s.contains(r#""feature":"modify""#));
  assert!(s.contains(r#""id":"pre-command""#));
  assert!(s.contains(r#""channel_datum_id":17"#));
  assert!(s.contains(r#""trigger_channel":"user""#));
  assert!(s.contains(r#""elapsed_ms":3"#));
  assert!(s.contains(r#""outcome":"continued""#), "missing outcome: {s}");
 }

 #[test]
 fn serializes_restarting_event() {
  let ev = ControlEvent::Restarting {
   new_conf_path: "C:/path/to/conf.toml".to_string(),
   new_pid: 12345,
   graceful_ms: 800,
   current_pid: 6789,
  };
  let s = serde_json::to_string(&ev).unwrap();
  assert!(s.contains(r#""kind":"restarting""#), "missing kind: {s}");
  assert!(s.contains(r#""new_pid":12345"#));
  assert!(s.contains(r#""graceful_ms":800"#));
  assert!(s.contains(r#""current_pid":6789"#));
  assert!(s.contains(r#""new_conf_path":"C:/path/to/conf.toml""#));
 }

 #[test]
 fn processor_invocation_outcome_serializes_snake_case() {
  assert_eq!(
   serde_json::to_string(&ProcessorInvocationOutcome::Continued).unwrap(),
   r#""continued""#
  );
  assert_eq!(
   serde_json::to_string(&ProcessorInvocationOutcome::Break).unwrap(),
   r#""break""#
  );
  assert_eq!(
   serde_json::to_string(&ProcessorInvocationOutcome::Error).unwrap(),
   r#""error""#
  );
 }
}
