//! Phase ε-3: CoeiroInk v2 の「終了の確認」TaskDialog を検出し、「終了」ボタン HWND に
//! `BM_CLICK` を送って正常終了させるハンドラ。
//!
//! ## 背景
//!
//! CoeiroInk v2 は ×ボタンや `WM_SYSCOMMAND(SC_CLOSE)` を受けると「アプリケーションを終了しますか？」
//! の確認 TaskDialog を出して応答待ちに入る。ユーザー応答がないとプロセスは残り続け、VAC 側で
//! `TerminateProcess` すると設定保存が飛ぶ（CoeiroInk は正常にクリーンアップ出来ていないので
//! 副次的な不整合も起きうる）。そこで VAC 側から「終了」ボタン相当の `BM_CLICK` を送って、
//! CoeiroInk 自身の正規経路で exit させる。
//!
//! ## 検出ルール（全部 AND）
//!
//! 1. トップレベル `#32770` ウィンドウ（Windows TaskDialog の共通クラス）
//! 2. オーナー CoeiroInk PID に一致
//! 3. 可視
//! 4. タイトルが `U+7D42 U+4E86`（= "終了"）で始まる
//!    - "終了の確認" と完全一致にすると将来の文言変更で壊れるので prefix 一致
//! 5. 子ウィンドウに `class = "Button"` かつ **タイトル完全一致で `U+7D42 U+4E86`** のものが存在
//!    - 「キャンセル」ではなく「終了」ボタンだけをクリック対象にするため完全一致
//!
//! 合致したボタンに `PostMessageW(BM_CLICK)` を非同期送信。呼び出しは冪等で、ダイアログが既に
//! 閉じていれば No-Op として扱う。
//!
//! ## 検証済み情報（PowerShell 調査結果）
//!
//! ```text
//! Dialog HWND=7277380  class="#32770"  title codepoints=[7D42 4E86 306E 78BA 8A8D]
//! Button[0] codepoints=[30AD 30E3 30F3 30BB 30EB]  (キャンセル)
//! Button[1] codepoints=[7D42 4E86]                 (終了)
//! → SendMessage(BM_CLICK) で全 PID がクリーン終了
//! ```

#![cfg(target_os = "windows")]

use std::collections::HashSet;

use windows::core::BOOL;
use windows::Win32::Foundation::{HWND, LPARAM, TRUE, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
	EnumChildWindows, EnumWindows, GetClassNameW, GetWindowTextW, GetWindowThreadProcessId, IsWindowVisible, PostMessageW, BM_CLICK,
};

use super::AppSpecificOutcome;

/// TaskDialog の Windows 標準クラス名。
const TASKDIALOG_CLASS: &str = "#32770";

/// 期待するダイアログタイトルの prefix（= "終了"）。
/// `[U+7D42, U+4E86]` を UTF-16 で直接書くことで、ソースを ASCII だけに保つ（ファイル
/// エンコーディング事故を避けるため）。
const TITLE_PREFIX_END: &[u16] = &[0x7D42, 0x4E86];

/// 期待するボタンタイトル（完全一致、= "終了"）。
const BUTTON_TITLE_END: &[u16] = &[0x7D42, 0x4E86];

/// 1 tick ぶんの呼び出し。冪等。
///
/// `pids` は CoeiroInk の子孫 PID 全部（`run_with_collect_process_tree_pids` 展開済）。
pub fn try_click_exit_confirm(pids: &HashSet<u32>) -> AppSpecificOutcome {
	let dialog_hwnds = find_dialog_hwnds(pids);
	if dialog_hwnds.is_empty() {
		return AppSpecificOutcome::NothingToDo;
	}

	let mut clicked = 0usize;
	for dlg in &dialog_hwnds {
		let buttons = find_end_buttons(*dlg);
		for btn in buttons {
			// PostMessageW は非同期。BM_CLICK を受け取った Button の WndProc が自力で
			// WM_LBUTTONDOWN/UP 相当の処理を走らせて、親へ WM_COMMAND(BN_CLICKED) を通知する。
			// 次の poll tick（200ms 後）にはダイアログが消えているはず。
			let r = unsafe { PostMessageW(Some(btn), BM_CLICK, WPARAM(0), LPARAM(0)) };
			if r.is_ok() {
				clicked += 1;
				log::info!(
     "《ManagedApp/app_specific:coeiroink》 終了の確認ダイアログの「終了」ボタンへ BM_CLICK を送信しました（dialog={:?} button={:?}）",
     dlg.0,
     btn.0
    );
			} else {
				log::warn!(
					"《ManagedApp/app_specific:coeiroink》 BM_CLICK 送信に失敗（dialog={:?} button={:?}）",
					dlg.0,
					btn.0
				);
			}
		}
	}

	if clicked > 0 {
		AppSpecificOutcome::Clicked {
			detail: format!("coeiroink: clicked {clicked} end button(s)"),
		}
	} else {
		AppSpecificOutcome::NothingToDo
	}
}

// --- 内部ヘルパ ---------------------------------------------------------------

struct DialogEnumCtx {
	pids: HashSet<u32>,
	out: Vec<HWND>,
}

unsafe extern "system" fn enum_dialogs_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
	let ctx = unsafe { &mut *(lparam.0 as *mut DialogEnumCtx) };
	let mut wpid = 0u32;
	unsafe { GetWindowThreadProcessId(hwnd, Some(&mut wpid as *mut u32)) };
	if !ctx.pids.contains(&wpid) {
		return TRUE;
	}
	if !unsafe { IsWindowVisible(hwnd).as_bool() } {
		return TRUE;
	}
	if !classname_equals(hwnd, TASKDIALOG_CLASS) {
		return TRUE;
	}
	if !title_starts_with(hwnd, TITLE_PREFIX_END) {
		return TRUE;
	}
	ctx.out.push(hwnd);
	TRUE
}

/// pids 内の PID に属し、かつ「終了の確認」条件を満たすトップレベル TaskDialog を列挙する。
fn find_dialog_hwnds(pids: &HashSet<u32>) -> Vec<HWND> {
	let mut ctx = DialogEnumCtx {
		pids: pids.clone(),
		out: Vec::new(),
	};
	let ptr: *mut DialogEnumCtx = &mut ctx;
	unsafe {
		let _ = EnumWindows(Some(enum_dialogs_cb), LPARAM(ptr as isize));
	}
	ctx.out
}

struct ButtonEnumCtx {
	out: Vec<HWND>,
}

unsafe extern "system" fn enum_buttons_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
	let ctx = unsafe { &mut *(lparam.0 as *mut ButtonEnumCtx) };
	if !unsafe { IsWindowVisible(hwnd).as_bool() } {
		return TRUE;
	}
	if !classname_equals(hwnd, "Button") {
		return TRUE;
	}
	if !title_equals(hwnd, BUTTON_TITLE_END) {
		return TRUE;
	}
	ctx.out.push(hwnd);
	TRUE
}

/// ダイアログ直下の「終了」ボタンを列挙する（通常 0 or 1 件）。
fn find_end_buttons(dialog: HWND) -> Vec<HWND> {
	let mut ctx = ButtonEnumCtx { out: Vec::new() };
	let ptr: *mut ButtonEnumCtx = &mut ctx;
	unsafe {
		let _ = EnumChildWindows(Some(dialog), Some(enum_buttons_cb), LPARAM(ptr as isize));
	}
	ctx.out
}

/// `GetClassNameW` の結果と期待値（ASCII）を比較する。
fn classname_equals(hwnd: HWND, expected: &str) -> bool {
	let mut buf = [0u16; 64];
	let n = unsafe { GetClassNameW(hwnd, &mut buf) } as usize;
	if n == 0 {
		return false;
	}
	let actual = String::from_utf16_lossy(&buf[..n]);
	actual == expected
}

/// `GetWindowTextW` の結果が指定 UTF-16 で **始まる** か。
fn title_starts_with(hwnd: HWND, prefix: &[u16]) -> bool {
	let mut buf = [0u16; 256];
	let n = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
	if n < prefix.len() {
		return false;
	}
	&buf[..prefix.len()] == prefix
}

/// `GetWindowTextW` の結果が指定 UTF-16 と **完全一致** か。
fn title_equals(hwnd: HWND, expected: &[u16]) -> bool {
	let mut buf = [0u16; 256];
	let n = unsafe { GetWindowTextW(hwnd, &mut buf) } as usize;
	if n != expected.len() {
		return false;
	}
	&buf[..n] == expected
}
