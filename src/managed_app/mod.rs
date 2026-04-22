//! Phase VI-γ-2b: Managed App 機能。
//!
//! ## 目的
//!
//! `conf.run_with` に並ぶ「VAC 起動時に一緒に立ち上げる周辺アプリ（OBS, CoeiroInk, …）」を、
//! **起動時の一発実行だけではなく**、GUI から
//!
//!   - 起動中かどうかの状態監視
//!   - 明示的な起動（`POST /managed_apps/{id}/start`）
//!   - 明示的な停止（`POST /managed_apps/{id}/stop`、WM_CLOSE → grace → TerminateProcess）
//!   - 明示的な最小化（`POST /managed_apps/{id}/minimize`）
//!
//! できるようにする。配信中 GUI の右肩ドロワーから「OBS を今立ち上げる」「CoeiroInk 落とす」を
//! 即時操作する、という User Persona A/B のユースケースが主対象。
//!
//! ## 仕様の要点
//!
//! - **対象は conf の `[[run_with]]` entry のみ**。API で任意 exe を起動できるようにはしない
//!   （LAN 経由で叩かれてもサンドボックスを出さないため）。
//! - **状態監視可能なのは `if_not_running` が指定された entry だけ**。`RunWith::Command(..)` 単純系は
//!   プロセス名で同定できないので `supports_status=false` となり、start のみ提供。
//! - **監視は `managed_app::Monitor` タスクが定期的に sysinfo を走査**、変化を
//!   `ControlEvent::ManagedAppState` として broadcast。GUI は受動的に反映すれば良い。
//! - **stop は 2 段階**: Windows なら WM_CLOSE を子ウィンドウに送って graceful exit を待ち、
//!   `grace_ms` 経過後に残 PID を TerminateProcess。他 OS はひとまず未対応（`STATUS_UNSUPPORTED`）。
//! - **minimize も Windows 専用**（既存 `conf::run_with_schedule_minimize_child_windows` を再利用）。
//!
//! ## ID 解決ルール
//!
//! `RunWith::CommandIfProcessIsNotRunning.id` が明示されていればそれを採用。
//! 無ければ `run-with-<1-origin index>` で自動採番。`Command(..)` 形式も index ベースの自動採番対象だが、
//! `supports_status=false` として扱う。

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

pub mod app_specific;

use chrono::{DateTime, Utc};
use serde::Serialize;
use tokio::sync::RwLock;

use crate::conf::{Conf, RunWith, RunWithShutdownAction, RunWithShutdownMethod, RunWithShutdownSpec};
use crate::shutdown::ShutdownBroker;
use crate::web_interface::control::events::ControlEvent;

/// Phase ε-2: `run_with` entry の shutdown 設定の **既定 grace_ms**（ミリ秒）。
///
/// `conf` で個別に指定がなければこの値を使う。OBS / CoeiroInk のように終了時にディスクへ
/// 設定保存を行うアプリの緩衝として、10 秒を初期値にしている（`stop_entry_graceful` 側で
/// ポーリング早抜けが効くので、通常は数秒で抜ける）。
pub const DEFAULT_SHUTDOWN_GRACE_MS: u64 = 10_000;

/// Phase ε-2/ε-3: shutdown 1 回分の解決済み設定。`RunWithShutdownSpec`（TOML 上の Option 群）から
/// [`ShutdownCfg::from_spec`] で defaults を埋めて作る。`stop_entry_graceful` の入力。
#[derive(Debug, Clone, Serialize)]
pub struct ShutdownCfg {
 pub action: RunWithShutdownAction,
 pub method: RunWithShutdownMethod,
 pub grace_ms: u64,
 /// Phase ε-3: 指定されていれば `app_specific::try_run(kind, pids)` をポーリング中に毎 tick 呼ぶ。
 /// `None` なら generic パスのみ。現在サポートする値は [`app_specific::SUPPORTED_KINDS`] 参照。
 pub app_specific: Option<String>,
}

impl Default for ShutdownCfg {
 fn default() -> Self {
  Self {
   action: RunWithShutdownAction::default(),
   method: RunWithShutdownMethod::default(),
   grace_ms: DEFAULT_SHUTDOWN_GRACE_MS,
   app_specific: None,
  }
 }
}

impl ShutdownCfg {
 /// TOML 上の `RunWithShutdownSpec` から defaults を埋めた `ShutdownCfg` を作る。
 /// `None` を渡した場合は完全な既定値。
 pub fn from_spec(spec: Option<&RunWithShutdownSpec>) -> Self {
  let mut out = Self::default();
  if let Some(s) = spec {
   if let Some(a) = s.action {
    out.action = a;
   }
   if let Some(m) = s.method {
    out.method = m;
   }
   if let Some(g) = s.grace_ms {
    out.grace_ms = g;
   }
   if let Some(k) = &s.app_specific {
    let k_trim = k.trim();
    if !k_trim.is_empty() {
     if !app_specific::SUPPORTED_KINDS.contains(&k_trim) {
      log::warn!(
       "《ManagedApp》 shutdown.app_specific={:?} は未知のキーです。無視します（有効値: {:?}）",
       k_trim,
       app_specific::SUPPORTED_KINDS
      );
     } else {
      out.app_specific = Some(k_trim.to_string());
     }
    }
   }
  }
  out
 }

 /// 呼び出し側から `grace_ms` だけ override する共通パターン（API 経由の `body.grace_ms` を反映する用途）。
 pub fn with_grace_ms(mut self, grace_ms: u64) -> Self {
  self.grace_ms = grace_ms;
  self
 }
}

/// Managed App 1 つぶんの定義。conf の `run_with` entry を 1:1 で写したもの + GUI 向けメタ情報。
#[derive(Debug, Clone, Serialize)]
pub struct ManagedAppSpec {
 pub id: String,
 pub label: String,
 pub command: String,
 /// `if_not_running` の値（プロセス名の部分一致検索に使う）。`None` なら状態監視不可。
 pub process_marker: Option<String>,
 /// `run_with` 側で `minimized = true` が付いていたか。GUI の「このアプリは最小化で立ち上げる」チップ表示用。
 pub minimized: bool,
 /// `run_with` 側で `run_as_admin = true` が付いていたか。
 pub run_as_admin: bool,
 /// `run_with` 側の `working_dir`。デバッグ表示のためだけに持つ。
 pub working_dir: Option<String>,
 /// 状態監視・停止・最小化が可能か（= `process_marker` が `Some`）。
 pub supports_status: bool,
 /// Phase ε-2: シャットダウン挙動（action / method / grace_ms）を defaults 込みで解決した値。
 /// API 側では `body.grace_ms` で上書き可能、VAC 自身の shutdown フローではそのまま使う。
 pub shutdown: ShutdownCfg,
}

/// sysinfo ポーリング 1 回分の状態スナップショット。
#[derive(Debug, Clone, Serialize)]
pub struct ManagedAppStatus {
 pub id: String,
 /// このポーリング時点で「該当プロセスが動いている」か。`supports_status=false` の entry は常に false。
 pub running: bool,
 /// マッチした PID の一覧（代表 pid ではなく全部）。
 pub pids: Vec<u32>,
 /// 観測した時刻（RFC3339）。
 pub checked_at: DateTime<Utc>,
}

impl ManagedAppStatus {
 pub fn unknown(id: &str) -> Self {
  Self {
   id: id.to_string(),
   running: false,
   pids: Vec::new(),
   checked_at: Utc::now(),
  }
 }
}

/// Spec 一覧と、最新ステータスのマップ。
#[derive(Debug, Default)]
pub struct ManagedAppRegistry {
 pub specs: Vec<ManagedAppSpec>,
 pub statuses: HashMap<String, ManagedAppStatus>,
}

impl ManagedAppRegistry {
 /// `Conf` 全体から specs を組み立てる。`run_with` の長さに合わせて 0..n を 1-origin の自動 ID に使う。
 pub fn from_conf(conf: &Conf) -> Self {
  Self::from_run_with(&conf.run_with)
 }

 /// 直接 `run_with` スライスを渡す版（テストおよび reload 経路用）。
 pub fn from_run_with(run_with: &[RunWith]) -> Self {
  let specs = build_specs(run_with);
  let statuses = specs
   .iter()
   .map(|s| (s.id.clone(), ManagedAppStatus::unknown(&s.id)))
   .collect();
  Self { specs, statuses }
 }

 /// ID で spec を引く。
 pub fn find_spec(&self, id: &str) -> Option<&ManagedAppSpec> {
  self.specs.iter().find(|s| s.id == id)
 }
}

/// `run_with` スライスから specs を組み立てるコアロジック。
fn build_specs(run_with: &[RunWith]) -> Vec<ManagedAppSpec> {
 let mut specs = Vec::with_capacity(run_with.len());
 let mut used_ids: std::collections::HashSet<String> = std::collections::HashSet::new();
 for (idx, rw) in run_with.iter().enumerate() {
  let auto_id = format!("run-with-{}", idx + 1);
  // 明示 id の重複はログで警告のうえ、後勝ち
  let id = match rw.explicit_id() {
   Some(id_str) => {
    if !used_ids.insert(id_str.to_string()) {
     log::warn!(
      "《ManagedApp》 `run_with` の id={:?} が重複しています。以前の entry より後勝ちで上書きされます。",
      id_str
     );
    }
    id_str.to_string()
   },
   None => {
    used_ids.insert(auto_id.clone());
    auto_id
   },
  };
  specs.push(ManagedAppSpec {
   id,
   label: rw.display_label(),
   command: rw.command().to_string(),
   process_marker: rw.process_marker().map(|s| s.to_string()),
   minimized: rw.minimized(),
   run_as_admin: rw.run_as_admin(),
   working_dir: rw.working_dir().map(|s| s.to_string()),
   supports_status: rw.process_marker().is_some(),
   shutdown: ShutdownCfg::from_spec(rw.shutdown_spec()),
  });
 }
 specs
}

/// sysinfo スナップショットから、各 spec の running/pids を判定する。
/// 戻り値はマップ（id → status）。呼び出し側で registry に merge する。
pub fn probe_all(specs: &[ManagedAppSpec]) -> HashMap<String, ManagedAppStatus> {
 use sysinfo::{ProcessRefreshKind, ProcessesToUpdate};

 let mut system = sysinfo::System::new();
 system.refresh_processes_specifics(
  ProcessesToUpdate::All,
  false,
  ProcessRefreshKind::everything().without_cpu(),
 );
 let now = Utc::now();

 let mut out = HashMap::with_capacity(specs.len());
 for spec in specs {
  let pids = if let Some(marker) = &spec.process_marker {
   system
    .processes()
    .iter()
    .filter(|(_, p)| p.name().to_string_lossy().contains(marker))
    .map(|(pid, _)| pid.as_u32())
    .collect::<Vec<u32>>()
  } else {
   Vec::new()
  };
  out.insert(
   spec.id.clone(),
   ManagedAppStatus {
    id: spec.id.clone(),
    running: !pids.is_empty(),
    pids,
    checked_at: now,
   },
  );
 }
 out
}

/// 定期的に probe_all して registry を更新し、変化があれば `ControlEvent::ManagedAppState` を broadcast するタスク。
///
/// 呼び出し側は `tokio::spawn(run_monitor(..))` で起動し、VAC 終了と同時に drop される。
///
/// Phase ε-1: `ShutdownBroker` を受け取り、停止要求が来た時点で即座に loop を抜ける。
/// これで cleanup 時に `stop_all_graceful` が走ったあと、裏でポーリングが続いて
/// ManagedAppState イベントを吐き続けることがなくなる（GUI の「停止確認」系表示の邪魔を防ぐ）。
pub async fn run_monitor(
 registry: Arc<RwLock<ManagedAppRegistry>>,
 event_tx: tokio::sync::broadcast::Sender<ControlEvent>,
 shutdown: Arc<ShutdownBroker>,
) {
 const INTERVAL: Duration = Duration::from_secs(3);
 log::info!("《ManagedApp》 監視タスクを開始します（interval={}s）。", INTERVAL.as_secs());

 // 初回は即実行
 let mut interval = tokio::time::interval(INTERVAL);
 interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

 loop {
  // tick と shutdown のどちらが先に来るか race させる。shutdown が来たら即 break。
  tokio::select! {
   _ = interval.tick() => {},
   _ = shutdown.wait() => {
    log::info!("《ManagedApp》 shutdown 要求を受けたため、監視タスクを終了します。");
    return;
   }
  }

  // specs だけ先に clone（probe_all は同期 + CPU バウンドだが、sysinfo は素早いので spawn_blocking までは不要）
  let specs = {
   let r = registry.read().await;
   r.specs.clone()
  };
  if specs.is_empty() {
   // 何も監視対象がなければタスクを終わってよいが、runtime で reload すると specs が増える可能性もあるので続ける
   continue;
  }

  let new_statuses = probe_all(&specs);

  // 変化検出 + registry 更新
  let changed: Vec<ManagedAppStatus> = {
   let mut w = registry.write().await;
   let mut diffs = Vec::new();
   for (id, new_s) in new_statuses {
    let prev = w.statuses.get(&id);
    let changed = match prev {
     Some(p) => p.running != new_s.running || p.pids != new_s.pids,
     None => true,
    };
    if changed {
     diffs.push(new_s.clone());
    }
    w.statuses.insert(id, new_s);
   }
   diffs
  };

  for s in changed {
   log::debug!(
    "《ManagedApp》 状態変化 id={:?} running={} pids={:?}",
    s.id,
    s.running,
    s.pids
   );
   let ev = ControlEvent::ManagedAppState {
    id: s.id.clone(),
    running: s.running,
    pids: s.pids.clone(),
    checked_at: s.checked_at.to_rfc3339(),
   };
   if let Err(e) = event_tx.send(ev) {
    log::trace!("ManagedAppState の broadcast に失敗（受信者 0 件の可能性）: {e}");
   }
  }
 }
}

/// Managed App を 1 つ起動する（`conf::Conf::run_with_entry` のラッパー）。
///
/// 呼び出し側は事前に [`ManagedAppStatus::running`] を確認して、起動中なら API レイヤで 409 を返すこと。
/// ここでは純粋に起動処理のみ実行する。
pub fn start_entry(run_with: &RunWith) -> crate::Result<()> {
 Conf::run_with_entry(run_with, None)?;
 Ok(())
}

/// Managed App を段階的に停止する（Phase ε-2 で `ShutdownCfg` ベースへ刷新）。
///
/// 動作:
///  1. `cfg.action == LeaveRunning` なら即座に 0 件結果で return。
///  2. `cfg.method` に従って close 通知を送信:
///     - `Syscommand`: `SendMessageTimeoutW(WM_SYSCOMMAND, SC_CLOSE)` を 2 秒タイムアウトで同期送信（×ボタン相当）
///     - `WmClose`   : `PostMessageW(WM_CLOSE)` を非同期送信（旧挙動）
///     - `Both`      : 両方送る（どちらか片方だけを処理するアプリ保険）
///  3. `cfg.grace_ms` まで 200ms 間隔でポーリングし、プロセスが exit したら早抜け。
///  4. grace を超えて残っていた場合:
///     - `CloseAndWait`: 残 PID を `TerminateProcess` で強制終了
///     - `CloseOnly`   : 強制終了せずログのみ出して放置（OBS 等の確認ダイアログ対応）
///
/// Windows 専用。他 OS は `Err` で戻す（API 側で 501 にする）。
#[cfg(target_os = "windows")]
pub async fn stop_entry_graceful(pids: &[u32], cfg: ShutdownCfg) -> crate::Result<StopOutcome> {
 use crate::conf::run_with_collect_process_tree_pids;

 if pids.is_empty() {
  return Ok(StopOutcome {
   closed_windows: 0,
   terminated_pids: 0,
  });
 }

 if matches!(cfg.action, RunWithShutdownAction::LeaveRunning) {
  log::info!(
   "《ManagedApp》 shutdown action=leave_running のため、対象 PID {} 件には何もしません。",
   pids.len()
  );
  return Ok(StopOutcome {
   closed_windows: 0,
   terminated_pids: 0,
  });
 }

 // 子孫 PID まで包括する。複数の root pid から集めた union。
 let mut all_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();
 for &p in pids {
  for d in run_with_collect_process_tree_pids(p) {
   all_pids.insert(d);
  }
 }

 let closed = send_close_to_pids(&all_pids, cfg.method);
 log::info!(
  "《ManagedApp》 close 通知を {} ウィンドウに送信しました（method={:?}, 対象 PID 数 {}）。",
  closed,
  cfg.method,
  all_pids.len()
 );

 // graceful 待ち: 一定間隔でポーリングし、全プロセスが自分で落ちきった時点で
 // 早抜けする。OBS / CoeiroInk は終了時に scene・モデル設定のディスク書き出しが
 // あって数秒かかるので、固定 sleep だと短すぎて TerminateProcess まで行ってしまい、
 // アプリ側で「異常終了」扱いになる（次回起動時にセーフモードダイアログ等）。
 // 早抜けできるのでユーザー体感は速いままで、最大待ち上限だけ grace_ms で守る。
 //
 // Phase ε-3: app_specific が設定されていれば毎 tick でアプリ固有ハンドラを呼ぶ。
 // CoeiroInk などは SC_CLOSE 直後に「終了の確認」ダイアログを出すので、
 // 最初の数 tick でハンドラがそれを検出して「終了」ボタンを押す → ダイアログが閉じ →
 // CoeiroInk が自力で正規終了する、という流れになる。
 let poll_interval = Duration::from_millis(200);
 let deadline = tokio::time::Instant::now() + Duration::from_millis(cfg.grace_ms);
 let mut exited_gracefully = false;
 // Unsupported は毎 tick 警告しても煩いので初回だけ出す。
 let mut app_specific_warned_unsupported = false;
 loop {
  tokio::time::sleep(poll_interval).await;

  if let Some(kind) = cfg.app_specific.as_deref() {
   match app_specific::try_run(kind, &all_pids).await {
    app_specific::AppSpecificOutcome::Clicked { detail } => {
     log::debug!("《ManagedApp》 app_specific ハンドラがアクションを実行: {detail}");
    },
    app_specific::AppSpecificOutcome::NothingToDo => {},
    app_specific::AppSpecificOutcome::Unsupported => {
     if !app_specific_warned_unsupported {
      log::warn!(
       "《ManagedApp》 app_specific={:?} はこの OS / ビルドで未対応のため generic フローのみで処理します。",
       kind
      );
      app_specific_warned_unsupported = true;
     }
    },
   }
  }

  let remaining = filter_alive_pids(&all_pids);
  if remaining.is_empty() {
   log::info!(
    "《ManagedApp》 graceful 終了を確認しました（残 PID 0, 対象 PID 数 {}）。",
    all_pids.len()
   );
   exited_gracefully = true;
   break;
  }
  if tokio::time::Instant::now() >= deadline {
   break;
  }
 }

 if exited_gracefully {
  return Ok(StopOutcome {
   closed_windows: closed,
   terminated_pids: 0,
  });
 }

 // grace 超過後の挙動は action で分岐する。
 let remaining = filter_alive_pids(&all_pids);
 match cfg.action {
  RunWithShutdownAction::CloseAndWait => {
   log::warn!(
    "《ManagedApp》 grace_ms={}ms 以内に終了しなかった PID が {} 件残っています。action=close_and_wait のため強制終了に移行します。",
    cfg.grace_ms,
    remaining.len()
   );
   let mut terminated = 0usize;
   for pid in &remaining {
    if terminate_process(*pid) {
     terminated += 1;
    }
   }
   if terminated > 0 {
    log::info!("《ManagedApp》 TerminateProcess で {} プロセスを強制終了しました。", terminated);
   }
   Ok(StopOutcome {
    closed_windows: closed,
    terminated_pids: terminated,
   })
  },
  RunWithShutdownAction::CloseOnly => {
   // OBS などで「保存しますか？」ダイアログが出ているケース。ユーザーが後で応答すれば
   // 綺麗に終わるので、ここでは強制終了しない。VAC 自身は先に exit する。
   log::info!(
    "《ManagedApp》 grace_ms={}ms 以内に終了しなかった PID が {} 件残っていますが、action=close_only のため強制終了しません（アプリ側の確認ダイアログ等が応答待ちの可能性）。",
    cfg.grace_ms,
    remaining.len()
   );
   Ok(StopOutcome {
    closed_windows: closed,
    terminated_pids: 0,
   })
  },
  RunWithShutdownAction::LeaveRunning => unreachable!("先頭で早期 return 済み"),
 }
}

#[cfg(not(target_os = "windows"))]
pub async fn stop_entry_graceful(_pids: &[u32], _cfg: ShutdownCfg) -> crate::Result<StopOutcome> {
 Err(crate::Error::InternalError(anyhow::anyhow!(
  "ManagedApp の停止操作はこの OS では未対応です（Windows 専用）。"
 )))
}

/// Phase ε-1/ε-2: registry に載っている全 ManagedApp を graceful 停止する。
///
/// VAC 自身のシャットダウンフローから呼ぶ。`run_with` で立ち上げた子プロセス（CoeiroInk、
/// VOICEVOX、LibreTranslate など）が親プロセスの stdio ハンドルを握ったまま残ると、
/// Windows では親が `exit` できず「2 回目 Ctrl+C で STATUS_CONTROL_C_EXIT 強制終了」の
/// 原因になる。ここで事前に畳んでから actix / bridges の cleanup に進む。
///
/// Phase ε-2 から、各 entry の shutdown 設定（`ShutdownCfg`）を spec から読んで使う。
/// 共通の grace_ms 引数は廃止。conf 上で `close_only` 指定された entry は強制終了されない。
///
/// **Phase ε-2.5 で並列化**: entry ごとに `tokio::spawn` で独立タスクを立てて、全部まとめて
/// await する。逐次だと先頭 entry が `CloseOnly` で grace_ms いっぱいブロック（例: CoeiroInk の
/// 「終了しますか」ダイアログで停止）したときに後続 entry が SC_CLOSE を受け取れず、
/// OBS が実はもっと早く閉じられるのに VAC 側で待たせてしまう、という現象があった。
/// 並列化で各 entry が独立に動くため、OBS の SC_CLOSE は CoeiroInk の応答を待たずに送られる。
///
/// 戻り値は各 entry に対する `(id, StopOutcome)` のリスト（ログ出力用）。非 Windows では
/// `stop_entry_graceful` が Err を返すため、結果は空に近くなる（0 件エントリに寄せる）。
pub async fn stop_all_graceful(registry: &Arc<RwLock<ManagedAppRegistry>>) -> Vec<(String, StopOutcome)> {
 // 監視ループが先に止まっていて statuses が古い可能性があるので、ここで改めて probe_all する。
 // `run_with` 直後で monitor が一度も回っていない場合でもこの再 probe で現行 PID を掴める。
 let specs = {
  let r = registry.read().await;
  r.specs.clone()
 };
 if specs.is_empty() {
  return Vec::new();
 }
 let statuses = probe_all(&specs);

 // 各 entry を独立タスクで並列実行。`stop_entry_graceful` は内部で Win32 EnumWindows /
 // sysinfo スナップショットを自前で用意するため、並行に呼んでも共有可変状態は無い。
 let mut handles: Vec<tokio::task::JoinHandle<(String, crate::Result<StopOutcome>)>> = Vec::with_capacity(specs.len());
 for spec in &specs {
  let Some(st) = statuses.get(&spec.id) else {
   continue;
  };
  if st.pids.is_empty() {
   continue;
  }
  log::info!(
   "《Shutdown》 ManagedApp stop 開始 id={} action={:?} method={:?} grace_ms={} pids={:?}",
   spec.id,
   spec.shutdown.action,
   spec.shutdown.method,
   spec.shutdown.grace_ms,
   st.pids
  );
  let id = spec.id.clone();
  let cfg = spec.shutdown.clone();
  let pids = st.pids.clone();
  handles.push(tokio::spawn(async move {
   let r = stop_entry_graceful(&pids, cfg).await;
   (id, r)
  }));
 }

 let mut out = Vec::with_capacity(handles.len());
 for h in handles {
  match h.await {
   Ok((id, Ok(outcome))) => out.push((id, outcome)),
   Ok((id, Err(e))) => log::warn!("《Shutdown》 ManagedApp stop 失敗 id={}: {e}", id),
   Err(e) => log::warn!("《Shutdown》 ManagedApp stop タスクが panic しました: {e}"),
  }
 }
 out
}

#[derive(Debug, Clone, Serialize)]
pub struct StopOutcome {
 pub closed_windows: usize,
 pub terminated_pids: usize,
}

/// Windows: 指定 PID 集合の「表示可能なトップレベルウィンドウ」にクローズ通知を送る。
///
/// `method` に応じて以下を使い分ける:
///  - `Syscommand`: `SendMessageTimeoutW(WM_SYSCOMMAND, SC_CLOSE)` 同期送信（2 秒タイムアウト、
///    SMTO_ABORTIFHUNG）。×ボタン押下と同じ経路で、アプリ側は DefWindowProc で WM_CLOSE に変換する。
///  - `WmClose`   : `PostMessageW(WM_CLOSE)` 非同期送信。
///  - `Both`      : 両方送る。片方だけ hook しているアプリの保険。
///
/// 戻り値は「送信に成功した HWND 数」。同一 HWND に両方送った場合は 1 件としてカウント（= どちらか片方でも成功したら 1）。
#[cfg(target_os = "windows")]
fn send_close_to_pids(pids: &std::collections::HashSet<u32>, method: RunWithShutdownMethod) -> usize {
 use windows::core::BOOL;
 use windows::Win32::Foundation::{HWND, LPARAM, TRUE, WPARAM};
 use windows::Win32::UI::WindowsAndMessaging::{
  EnumWindows, GetWindow, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, SendMessageTimeoutW, GW_OWNER,
  SC_CLOSE, SMTO_ABORTIFHUNG, SMTO_NORMAL, WM_CLOSE, WM_SYSCOMMAND,
 };

 struct EnumCtx {
  pids: std::collections::HashSet<u32>,
  hwnds: Vec<HWND>,
 }

 unsafe extern "system" fn cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
  let ctx = unsafe { &mut *(lparam.0 as *mut EnumCtx) };
  let mut wpid = 0u32;
  unsafe { GetWindowThreadProcessId(hwnd, Some(&mut wpid as *mut u32)) };
  if !ctx.pids.contains(&wpid) {
   return TRUE;
  }
  // ツールウィンドウやオーナー付きは無視（親ウィンドウにだけ送る）
  if unsafe { GetWindow(hwnd, GW_OWNER) }.map(|h| !h.is_invalid()).unwrap_or(false) {
   return TRUE;
  }
  if !unsafe { IsWindowVisible(hwnd).as_bool() } {
   return TRUE;
  }
  ctx.hwnds.push(hwnd);
  TRUE
 }

 let mut ctx = EnumCtx { pids: pids.clone(), hwnds: Vec::new() };
 let ptr: *mut EnumCtx = &mut ctx;
 unsafe {
  let _ = EnumWindows(Some(cb), LPARAM(ptr as isize));
 }

 let mut ok = 0usize;
 for hwnd in ctx.hwnds {
  let use_syscmd = matches!(method, RunWithShutdownMethod::Syscommand | RunWithShutdownMethod::Both);
  let use_wmclose = matches!(method, RunWithShutdownMethod::WmClose | RunWithShutdownMethod::Both);

  let mut any_ok = false;

  if use_syscmd {
   // SendMessageTimeoutW は LRESULT を返し、タイムアウト/エラー時は 0。
   // SMTO_ABORTIFHUNG でハング窓を早期スキップし、最大 2 秒でブロックを解除する。
   let mut result: usize = 0;
   let lr = unsafe {
    SendMessageTimeoutW(
     hwnd,
     WM_SYSCOMMAND,
     WPARAM(SC_CLOSE as usize),
     LPARAM(0),
     SMTO_NORMAL | SMTO_ABORTIFHUNG,
     2000,
     Some(&mut result as *mut usize),
    )
   };
   if lr.0 != 0 {
    any_ok = true;
   } else {
    log::debug!(
     "《ManagedApp》 SendMessageTimeoutW(SC_CLOSE) が hwnd={:?} に対して 0 を返しました（hung or timeout）",
     hwnd.0
    );
   }
  }

  if use_wmclose {
   let r = unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) };
   if r.is_ok() {
    any_ok = true;
   }
  }

  if any_ok {
   ok += 1;
  }
 }
 ok
}

/// Windows: 指定 PID のうち、まだ生きているものだけを残して返す。
#[cfg(target_os = "windows")]
fn filter_alive_pids(pids: &std::collections::HashSet<u32>) -> Vec<u32> {
 use sysinfo::{Pid, ProcessRefreshKind, ProcessesToUpdate};
 let mut system = sysinfo::System::new();
 system.refresh_processes_specifics(
  ProcessesToUpdate::All,
  false,
  ProcessRefreshKind::everything().without_cpu(),
 );
 pids
  .iter()
  .copied()
  .filter(|pid| system.process(Pid::from_u32(*pid)).is_some())
  .collect()
}

/// Windows: TerminateProcess で強制終了。失敗（AccessDenied 等）は log::warn のみ。
#[cfg(target_os = "windows")]
fn terminate_process(pid: u32) -> bool {
 use windows::Win32::Foundation::CloseHandle;
 use windows::Win32::System::Threading::{OpenProcess, TerminateProcess, PROCESS_TERMINATE};

 unsafe {
  let handle = match OpenProcess(PROCESS_TERMINATE, false, pid) {
   Ok(h) => h,
   Err(e) => {
    log::warn!("《ManagedApp》 OpenProcess(pid={pid}) 失敗: {:?}", e);
    return false;
   },
  };
  let r = TerminateProcess(handle, 1);
  let _ = CloseHandle(handle);
  if let Err(e) = r {
   log::warn!("《ManagedApp》 TerminateProcess(pid={pid}) 失敗: {:?}", e);
   return false;
  }
  true
 }
}

/// Minimize: Windows 専用。既存 `conf::run_with_schedule_minimize_child_windows` に委譲。
#[cfg(target_os = "windows")]
pub fn minimize_pids(pids: &[u32]) -> usize {
 for &pid in pids {
  crate::conf::run_with_schedule_minimize_child_windows(pid);
 }
 pids.len()
}

#[cfg(not(target_os = "windows"))]
pub fn minimize_pids(_pids: &[u32]) -> usize {
 0
}

#[cfg(test)]
mod tests {
 use super::*;

 #[test]
 fn auto_id_when_not_specified() {
  let rws = vec![
   RunWith::Command("http://example.com".to_string()),
   RunWith::CommandIfProcessIsNotRunning {
    command: "obs64.exe".to_string(),
    if_not_running: Some("obs64".to_string()),
    run_as_admin: None,
    working_dir: None,
    minimized: None,
    id: None,
    label: None,
    shutdown: None,
   },
   RunWith::CommandIfProcessIsNotRunning {
    command: "coeiroink.exe".to_string(),
    if_not_running: Some("COEIROINKv2".to_string()),
    run_as_admin: None,
    working_dir: None,
    minimized: Some(true),
    id: Some("coeiroink".to_string()),
    label: Some("CoeiroInk (speech)".to_string()),
    shutdown: None,
   },
  ];
  let reg = ManagedAppRegistry::from_run_with(&rws);
  assert_eq!(reg.specs.len(), 3);
  assert_eq!(reg.specs[0].id, "run-with-1");
  assert!(!reg.specs[0].supports_status, "Command(..) は状態監視対象外");
  assert_eq!(reg.specs[1].id, "run-with-2");
  assert!(reg.specs[1].supports_status);
  assert_eq!(reg.specs[2].id, "coeiroink", "明示 id が優先される");
  assert_eq!(reg.specs[2].label, "CoeiroInk (speech)");
  assert!(reg.specs[2].minimized);
 }
}
