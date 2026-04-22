//! Phase ε-3: **アプリ固有のシャットダウン後処理**。
//!
//! `ShutdownCfg.app_specific` に識別キーが設定されている entry については、
//! `stop_entry_graceful` のポーリングループの毎 tick で [`try_run`] が呼ばれる。
//!
//! 各ハンドラは次の前提で書かれる:
//!
//! - **冪等**: 同じ状態で複数回呼んでも副作用が増えない（「既にクリック済み」なら No-Op）。
//! - **失敗に寛容**: ダイアログ未出現 / ボタン未発見 / API 呼び出し失敗はいずれも `NothingToDo` or
//!   エラーログに留める。ポーリング継続中に状態が変わる前提なので Err 返しはしない。
//! - **非破壊**: `TerminateProcess` 等の強硬手段は使わない。ハンドラはあくまで「アプリ側の正規の
//!   終了経路を補助する」役割。
//!
//! 現状サポート:
//!
//! - `"coeiroink"` — CoeiroInk v2 の「終了の確認」TaskDialog（`#32770`）の「終了」ボタンへ
//!   `BM_CLICK` を送る。詳細は [`coeiroink`]。
//!
//! 将来追加するなら [`coeiroink`] と同じパターンで submodule を足して、
//! [`SUPPORTED_KINDS`] と [`try_run`] の match に登録する。

#[cfg(target_os = "windows")]
pub mod coeiroink;

/// ホワイトリスト: conf パース時にここに載ってないキーは warn & 無視。
pub const SUPPORTED_KINDS: &[&str] = &["coeiroink"];

/// 1 tick ぶんのアプリ固有ハンドラを実行した結果。ログ用途のみ。
#[derive(Debug, Clone)]
pub enum AppSpecificOutcome {
 /// ダイアログや API 呼び出しで実際にアクションを実行した（今回の tick で何かした）。
 Clicked {
  detail: String,
 },
 /// 対象状態ではない（ダイアログが出ていない等）。次 tick を待つ。
 NothingToDo,
 /// `kind` が未知か、この OS ではサポート外。毎 tick 出してもうるさいだけなので、
 /// 呼び出し側で初回だけログするのが想定。
 Unsupported,
}

/// `ShutdownCfg.app_specific` に応じたハンドラを呼ぶ dispatcher。冪等性は各ハンドラ側の責任。
///
/// `pids` は `run_with_collect_process_tree_pids` で展開済みの全 PID。ハンドラはここから
/// 関連ウィンドウを同定する。
#[cfg(target_os = "windows")]
pub async fn try_run(kind: &str, pids: &std::collections::HashSet<u32>) -> AppSpecificOutcome {
 match kind {
  "coeiroink" => coeiroink::try_click_exit_confirm(pids),
  _ => AppSpecificOutcome::Unsupported,
 }
}

#[cfg(not(target_os = "windows"))]
pub async fn try_run(_kind: &str, _pids: &std::collections::HashSet<u32>) -> AppSpecificOutcome {
 // 非 Windows は Win32 に依存するハンドラが動かないので全件未対応。
 AppSpecificOutcome::Unsupported
}
