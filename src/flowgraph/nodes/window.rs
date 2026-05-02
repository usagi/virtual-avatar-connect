//! Phase tau: window control ノード。
//!
//! Windows を第一級 target とする。非 Windows では明示的に on_error / 空結果を返す。

use crate::flowgraph::node::{
	get_optional_bool, get_optional_int, get_optional_string, get_required_int, EffectfulNode, ExecCtx, ExecFireSet, InputMap,
	NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{FlowResult, SocketType, SocketValue};
use crate::flowgraph::table::{ColumnSpec, Row, Table, TableSchema};
use async_trait::async_trait;
use serde_json::{json, Value as JsonValue};

pub struct WindowEnumNode;
pub struct WindowMoveNode;
pub struct WindowResizeNode;
pub struct WindowMinimizeNode;
pub struct WindowMaximizeNode;
pub struct WindowRestoreNode;
pub struct WindowCloseNode;
pub struct WindowForegroundNode;
pub struct WindowPseudoFullscreenNode;
pub struct WindowPseudoFullscreenExitNode;

impl NodeDescriptor for WindowEnumNode {
	fn describe(&self) -> NodeSpec {
		NodeSpec {
			feature: "flowgraph.window.enum".into(),
			title: "Window: Enum".into(),
			category: "window".into(),
			description: Some("トップレベルウィンドウ一覧を Table として取得する。Windows first。".into()),
			inputs: vec![
				PortSpec::exec_input("exec_in", "Exec"),
				PortSpec::input("pid", "PID", SocketType::Int).with_default(SocketValue::Int(0)),
				PortSpec::input("title_filter", "Title Filter", SocketType::String).with_default(SocketValue::String(String::new())),
				PortSpec::input("exact", "Exact Title", SocketType::Bool).with_default(SocketValue::Bool(false)),
				PortSpec::input("visible_only", "Visible Only", SocketType::Bool).with_default(SocketValue::Bool(true)),
			],
			outputs: vec![
				PortSpec::exec_output("exec_out", "Exec Out"),
				PortSpec::output("windows", "Windows", SocketType::Table),
				PortSpec::output("count", "Count", SocketType::Int),
				PortSpec::output("error", "Error", SocketType::String),
				PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Table))),
			],
			properties: vec![],
		}
	}
}

#[async_trait]
impl EffectfulNode for WindowEnumNode {
	async fn execute(
		&self,
		ctx: &mut ExecCtx,
		_props: &InputMap,
		inputs: &InputMap,
		fired_exec: &ExecFireSet,
	) -> Result<NodeOutput, NodeExecError> {
		if !fired_exec.contains("exec_in") {
			return Ok(NodeOutput::new());
		}
		let query = WindowQuery::from_inputs(inputs)?;
		match platform_enum_windows(&query) {
			Ok(windows) => {
				let count = windows.len() as i64;
				ctx.log(format!("window.enum: {count} windows"));
				Ok(enum_output(windows, String::new()).fire_exec("exec_out"))
			}
			Err(e) => Ok(enum_output(Vec::new(), e).fire_exec("exec_out")),
		}
	}
}

macro_rules! action_node {
	($ty:ident, $feature:literal, $title:literal, $description:literal, $action:expr, $extra_inputs:expr) => {
		impl NodeDescriptor for $ty {
			fn describe(&self) -> NodeSpec {
				let mut inputs = vec![
					PortSpec::exec_input("exec_in", "Exec"),
					PortSpec::input("hwnd", "HWND", SocketType::Int).with_default(SocketValue::Int(0)),
					PortSpec::input("pid", "PID", SocketType::Int).with_default(SocketValue::Int(0)),
					PortSpec::input("title_filter", "Title Filter", SocketType::String).with_default(SocketValue::String(String::new())),
					PortSpec::input("exact", "Exact Title", SocketType::Bool).with_default(SocketValue::Bool(false)),
				];
				inputs.extend($extra_inputs);
				NodeSpec {
					feature: $feature.into(),
					title: $title.into(),
					category: "window".into(),
					description: Some($description.into()),
					inputs,
					outputs: vec![
						PortSpec::exec_output("on_success", "On Success"),
						PortSpec::exec_output("on_error", "On Error"),
						PortSpec::output("affected_count", "Affected Count", SocketType::Int),
						PortSpec::output("windows", "Windows", SocketType::Table),
						PortSpec::output("error", "Error", SocketType::String),
						PortSpec::output("result", "Result", SocketType::Result(Box::new(SocketType::Int))),
					],
					properties: vec![],
				}
			}
		}

		#[async_trait]
		impl EffectfulNode for $ty {
			async fn execute(
				&self,
				ctx: &mut ExecCtx,
				_props: &InputMap,
				inputs: &InputMap,
				fired_exec: &ExecFireSet,
			) -> Result<NodeOutput, NodeExecError> {
				if !fired_exec.contains("exec_in") {
					return Ok(NodeOutput::new());
				}
				let target = WindowTarget::from_inputs(inputs)?;
				match platform_apply_action(&target, $action, inputs) {
					Ok(result) => {
						ctx.log(format!("{}: {} windows", $feature, result.affected_count));
						let out = action_output(result.affected_count, result.windows, "");
						Ok(if result.affected_count > 0 {
							out.fire_exec("on_success")
						} else {
							out.fire_exec("on_error")
						})
					}
					Err(e) => Ok(action_output(0, Vec::new(), e).fire_exec("on_error")),
				}
			}
		}
	};
}

action_node!(
	WindowMoveNode,
	"flowgraph.window.move",
	"Window: Move",
	"対象ウィンドウの左上座標を変更する。",
	WindowAction::Move,
	vec![
		PortSpec::input("x", "X", SocketType::Int),
		PortSpec::input("y", "Y", SocketType::Int)
	]
);
action_node!(
	WindowResizeNode,
	"flowgraph.window.resize",
	"Window: Resize",
	"対象ウィンドウのサイズを変更する。",
	WindowAction::Resize,
	vec![
		PortSpec::input("width", "Width", SocketType::Int),
		PortSpec::input("height", "Height", SocketType::Int)
	]
);
action_node!(
	WindowMinimizeNode,
	"flowgraph.window.minimize",
	"Window: Minimize",
	"対象ウィンドウを最小化する。",
	WindowAction::Minimize,
	Vec::new()
);
action_node!(
	WindowMaximizeNode,
	"flowgraph.window.maximize",
	"Window: Maximize",
	"対象ウィンドウを最大化する。",
	WindowAction::Maximize,
	Vec::new()
);
action_node!(
	WindowRestoreNode,
	"flowgraph.window.restore",
	"Window: Restore",
	"対象ウィンドウを通常表示へ戻す。",
	WindowAction::Restore,
	Vec::new()
);
action_node!(
	WindowCloseNode,
	"flowgraph.window.close",
	"Window: Close",
	"対象ウィンドウへ WM_CLOSE を送る。",
	WindowAction::Close,
	Vec::new()
);
action_node!(
	WindowForegroundNode,
	"flowgraph.window.foreground",
	"Window: Foreground",
	"対象ウィンドウを前面化する。",
	WindowAction::Foreground,
	Vec::new()
);
action_node!(
	WindowPseudoFullscreenNode,
	"flowgraph.window.pseudo_fullscreen",
	"Window: Pseudo Fullscreen",
	"対象ウィンドウの表示状態を保存して最大化する first slice。",
	WindowAction::PseudoFullscreen,
	Vec::new()
);
action_node!(
	WindowPseudoFullscreenExitNode,
	"flowgraph.window.pseudo_fullscreen_exit",
	"Window: Pseudo Fullscreen Exit",
	"pseudo_fullscreen 前の表示状態を復元する。",
	WindowAction::PseudoFullscreenExit,
	Vec::new()
);

#[derive(Debug, Clone)]
struct WindowQuery {
	pid: u32,
	title_filter: String,
	exact: bool,
	visible_only: bool,
}

impl WindowQuery {
	fn from_inputs(inputs: &InputMap) -> Result<Self, NodeExecError> {
		Ok(Self {
			pid: i64_to_u32(get_optional_int(inputs, "pid", 0)?)?,
			title_filter: get_optional_string(inputs, "title_filter", "")?,
			exact: get_optional_bool(inputs, "exact", false)?,
			visible_only: get_optional_bool(inputs, "visible_only", true)?,
		})
	}
}

#[derive(Debug, Clone)]
struct WindowTarget {
	hwnd: i64,
	pid: u32,
	title_filter: String,
	exact: bool,
}

impl WindowTarget {
	fn from_inputs(inputs: &InputMap) -> Result<Self, NodeExecError> {
		Ok(Self {
			hwnd: get_optional_int(inputs, "hwnd", 0)?,
			pid: i64_to_u32(get_optional_int(inputs, "pid", 0)?)?,
			title_filter: get_optional_string(inputs, "title_filter", "")?,
			exact: get_optional_bool(inputs, "exact", false)?,
		})
	}

	fn to_query(&self) -> WindowQuery {
		WindowQuery {
			pid: self.pid,
			title_filter: self.title_filter.clone(),
			exact: self.exact,
			visible_only: true,
		}
	}
}

#[derive(Debug, Clone)]
struct WindowInfo {
	hwnd: i64,
	pid: u32,
	title: String,
	x: i32,
	y: i32,
	width: i32,
	height: i32,
	visible: bool,
	minimized: bool,
}

#[derive(Debug, Clone, Copy)]
enum WindowAction {
	Move,
	Resize,
	Minimize,
	Maximize,
	Restore,
	Close,
	Foreground,
	PseudoFullscreen,
	PseudoFullscreenExit,
}

#[derive(Debug, Clone)]
struct WindowActionResult {
	affected_count: i64,
	windows: Vec<WindowInfo>,
}

fn windows_to_table(windows: &[WindowInfo]) -> Table {
	let schema = TableSchema::new(vec![
		ColumnSpec::new("hwnd", SocketType::Int),
		ColumnSpec::new("pid", SocketType::Int),
		ColumnSpec::new("title", SocketType::String),
		ColumnSpec::new("x", SocketType::Int),
		ColumnSpec::new("y", SocketType::Int),
		ColumnSpec::new("width", SocketType::Int),
		ColumnSpec::new("height", SocketType::Int),
		ColumnSpec::new("visible", SocketType::Bool),
		ColumnSpec::new("minimized", SocketType::Bool),
	]);
	let rows = windows
		.iter()
		.map(|w| {
			Row::new(vec![
				json!(w.hwnd),
				json!(w.pid),
				JsonValue::String(w.title.clone()),
				json!(w.x),
				json!(w.y),
				json!(w.width),
				json!(w.height),
				json!(w.visible),
				json!(w.minimized),
			])
		})
		.collect();
	Table::new(schema, rows)
}

fn enum_output(windows: Vec<WindowInfo>, error: impl Into<String>) -> NodeOutput {
	let error = error.into();
	let table = windows_to_table(&windows);
	let result = if error.is_empty() {
		FlowResult::ok(SocketValue::Table(table.clone()))
	} else {
		FlowResult::err(error.clone()).with_code("window.enum")
	};
	NodeOutput::new()
		.set_data("windows", SocketValue::Table(table))
		.set_data("count", SocketValue::Int(windows.len() as i64))
		.set_data("error", SocketValue::String(error))
		.set_data("result", SocketValue::Result(result))
}

fn action_output(affected_count: i64, windows: Vec<WindowInfo>, error: impl Into<String>) -> NodeOutput {
	let error = error.into();
	let result = if error.is_empty() && affected_count > 0 {
		FlowResult::ok(SocketValue::Int(affected_count))
	} else {
		let msg = if error.is_empty() {
			"no window affected".to_string()
		} else {
			error.clone()
		};
		FlowResult::err(msg).with_code("window.action")
	};
	NodeOutput::new()
		.set_data("affected_count", SocketValue::Int(affected_count))
		.set_data("windows", SocketValue::Table(windows_to_table(&windows)))
		.set_data("error", SocketValue::String(error))
		.set_data("result", SocketValue::Result(result))
}

fn i64_to_u32(v: i64) -> Result<u32, NodeExecError> {
	if v < 0 || v > i64::from(u32::MAX) {
		return Err(NodeExecError::Generic(anyhow::anyhow!("value out of u32 range: {v}")));
	}
	Ok(v as u32)
}

#[cfg(not(target_os = "windows"))]
fn platform_enum_windows(_query: &WindowQuery) -> Result<Vec<WindowInfo>, String> {
	Err("flowgraph.window.* is only implemented on Windows".into())
}

#[cfg(not(target_os = "windows"))]
fn platform_apply_action(_target: &WindowTarget, _action: WindowAction, _inputs: &InputMap) -> Result<WindowActionResult, String> {
	Err("flowgraph.window.* is only implemented on Windows".into())
}

#[cfg(target_os = "windows")]
mod windows_impl {
	use super::*;
	use std::collections::HashMap;
	use std::sync::{LazyLock, Mutex};
	use windows::core::BOOL;
	use windows::Win32::Foundation::{HWND, LPARAM, RECT, TRUE};
	use windows::Win32::UI::WindowsAndMessaging::{
		EnumWindows, GetWindow, GetWindowPlacement, GetWindowRect, GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindowVisible,
		PostMessageW, SetForegroundWindow, SetWindowPlacement, SetWindowPos, ShowWindow, GW_OWNER, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
		SW_MAXIMIZE, SW_MINIMIZE, SW_RESTORE, WINDOWPLACEMENT, WM_CLOSE, WNDENUMPROC,
	};

	static SAVED_PLACEMENTS: LazyLock<Mutex<HashMap<i64, WINDOWPLACEMENT>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

	struct EnumCtx {
		query: WindowQuery,
		windows: Vec<WindowInfo>,
	}

	pub(super) fn enum_windows(query: &WindowQuery) -> Result<Vec<WindowInfo>, String> {
		let mut ctx = EnumCtx {
			query: query.clone(),
			windows: Vec::new(),
		};
		let ptr: *mut EnumCtx = &mut ctx;
		unsafe {
			let enum_fn: WNDENUMPROC = Some(enum_cb);
			EnumWindows(enum_fn, LPARAM(ptr as isize)).map_err(|e| format!("EnumWindows: {e}"))?;
		}
		ctx.windows.sort_by(|a, b| a.title.cmp(&b.title).then(a.hwnd.cmp(&b.hwnd)));
		Ok(ctx.windows)
	}

	unsafe extern "system" fn enum_cb(hwnd: HWND, lparam: LPARAM) -> BOOL {
		let ctx = unsafe { &mut *(lparam.0 as *mut EnumCtx) };
		if let Some(info) = window_info(hwnd) {
			if matches_query(&info, &ctx.query) {
				ctx.windows.push(info);
			}
		}
		TRUE
	}

	pub(super) fn apply_action(target: &WindowTarget, action: WindowAction, inputs: &InputMap) -> Result<WindowActionResult, String> {
		let hwnds = target_hwnds(target)?;
		if hwnds.is_empty() {
			return Ok(WindowActionResult {
				affected_count: 0,
				windows: Vec::new(),
			});
		}
		let mut affected = 0i64;
		for hwnd in &hwnds {
			if apply_one(*hwnd, action, inputs)? {
				affected += 1;
			}
		}
		let windows = hwnds.into_iter().filter_map(window_info).collect();
		Ok(WindowActionResult {
			affected_count: affected,
			windows,
		})
	}

	fn target_hwnds(target: &WindowTarget) -> Result<Vec<HWND>, String> {
		if target.hwnd != 0 {
			return Ok(vec![hwnd_from_i64(target.hwnd)]);
		}
		if target.pid == 0 && target.title_filter.trim().is_empty() {
			return Err("hwnd, pid, or title_filter is required".into());
		}
		Ok(enum_windows(&target.to_query())?
			.into_iter()
			.map(|w| hwnd_from_i64(w.hwnd))
			.collect())
	}

	fn apply_one(hwnd: HWND, action: WindowAction, inputs: &InputMap) -> Result<bool, String> {
		match action {
			WindowAction::Move => {
				let x = i64_to_i32(get_required_int(inputs, "x").map_err(|e| e.to_string())?)?;
				let y = i64_to_i32(get_required_int(inputs, "y").map_err(|e| e.to_string())?)?;
				unsafe { SetWindowPos(hwnd, None, x, y, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE) }
					.map(|_| true)
					.map_err(|e| format!("SetWindowPos(move): {e}"))
			}
			WindowAction::Resize => {
				let width = i64_to_i32(get_required_int(inputs, "width").map_err(|e| e.to_string())?)?.max(1);
				let height = i64_to_i32(get_required_int(inputs, "height").map_err(|e| e.to_string())?)?.max(1);
				unsafe { SetWindowPos(hwnd, None, 0, 0, width, height, SWP_NOMOVE | SWP_NOACTIVATE) }
					.map(|_| true)
					.map_err(|e| format!("SetWindowPos(resize): {e}"))
			}
			WindowAction::Minimize => Ok(unsafe { ShowWindow(hwnd, SW_MINIMIZE).as_bool() }),
			WindowAction::Maximize => Ok(unsafe { ShowWindow(hwnd, SW_MAXIMIZE).as_bool() }),
			WindowAction::Restore => Ok(unsafe { ShowWindow(hwnd, SW_RESTORE).as_bool() }),
			WindowAction::Close => unsafe { PostMessageW(Some(hwnd), WM_CLOSE, Default::default(), Default::default()) }
				.map(|_| true)
				.map_err(|e| format!("PostMessageW(WM_CLOSE): {e}")),
			WindowAction::Foreground => Ok(unsafe { SetForegroundWindow(hwnd).as_bool() }),
			WindowAction::PseudoFullscreen => {
				let mut placement = WINDOWPLACEMENT::default();
				placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
				unsafe { GetWindowPlacement(hwnd, &mut placement) }.map_err(|e| format!("GetWindowPlacement: {e}"))?;
				SAVED_PLACEMENTS
					.lock()
					.map_err(|_| "placement lock poisoned".to_string())?
					.insert(hwnd_to_i64(hwnd), placement);
				Ok(unsafe { ShowWindow(hwnd, SW_MAXIMIZE).as_bool() })
			}
			WindowAction::PseudoFullscreenExit => {
				let placement = SAVED_PLACEMENTS
					.lock()
					.map_err(|_| "placement lock poisoned".to_string())?
					.remove(&hwnd_to_i64(hwnd));
				if let Some(mut placement) = placement {
					placement.length = std::mem::size_of::<WINDOWPLACEMENT>() as u32;
					unsafe { SetWindowPlacement(hwnd, &placement) }
						.map(|_| true)
						.map_err(|e| format!("SetWindowPlacement: {e}"))
				} else {
					Ok(unsafe { ShowWindow(hwnd, SW_RESTORE).as_bool() })
				}
			}
		}
	}

	fn matches_query(info: &WindowInfo, query: &WindowQuery) -> bool {
		if query.visible_only && !info.visible {
			return false;
		}
		if query.pid > 0 && info.pid != query.pid {
			return false;
		}
		let filter = query.title_filter.trim().to_lowercase();
		if filter.is_empty() {
			return true;
		}
		let title = info.title.to_lowercase();
		if query.exact {
			title == filter
		} else {
			title.contains(&filter)
		}
	}

	fn window_info(hwnd: HWND) -> Option<WindowInfo> {
		if unsafe { GetWindow(hwnd, GW_OWNER) }.map(|h| !h.is_invalid()).unwrap_or(false) {
			return None;
		}
		let mut pid = 0u32;
		unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid as *mut u32)) };
		if pid == 0 {
			return None;
		}
		let mut rect = RECT::default();
		if unsafe { GetWindowRect(hwnd, &mut rect) }.is_err() {
			return None;
		}
		let title = window_title(hwnd);
		let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
		Some(WindowInfo {
			hwnd: hwnd_to_i64(hwnd),
			pid,
			title,
			x: rect.left,
			y: rect.top,
			width: (rect.right - rect.left).max(0),
			height: (rect.bottom - rect.top).max(0),
			visible,
			minimized: unsafe { IsIconic(hwnd).as_bool() },
		})
	}

	fn window_title(hwnd: HWND) -> String {
		let mut buf = [0u16; 512];
		let n = unsafe { GetWindowTextW(hwnd, &mut buf) };
		String::from_utf16_lossy(&buf[..n as usize])
	}

	fn hwnd_to_i64(hwnd: HWND) -> i64 {
		hwnd.0 as isize as i64
	}

	fn hwnd_from_i64(hwnd: i64) -> HWND {
		HWND(hwnd as isize as *mut core::ffi::c_void)
	}

	fn i64_to_i32(v: i64) -> Result<i32, String> {
		i32::try_from(v).map_err(|_| format!("value out of i32 range: {v}"))
	}
}

#[cfg(target_os = "windows")]
fn platform_enum_windows(query: &WindowQuery) -> Result<Vec<WindowInfo>, String> {
	windows_impl::enum_windows(query)
}

#[cfg(target_os = "windows")]
fn platform_apply_action(target: &WindowTarget, action: WindowAction, inputs: &InputMap) -> Result<WindowActionResult, String> {
	windows_impl::apply_action(target, action, inputs)
}

#[cfg(test)]
mod tests {
	use super::*;

	fn exec_in_fire() -> ExecFireSet {
		let mut fired = ExecFireSet::new();
		fired.insert("exec_in");
		fired
	}

	#[tokio::test]
	async fn window_nodes_no_fire_are_noop() {
		let mut ctx = ExecCtx::default();
		let nodes: Vec<Box<dyn EffectfulNode>> = vec![
			Box::new(WindowEnumNode),
			Box::new(WindowMoveNode),
			Box::new(WindowResizeNode),
			Box::new(WindowMinimizeNode),
			Box::new(WindowMaximizeNode),
			Box::new(WindowRestoreNode),
			Box::new(WindowCloseNode),
			Box::new(WindowForegroundNode),
			Box::new(WindowPseudoFullscreenNode),
			Box::new(WindowPseudoFullscreenExitNode),
		];
		for node in nodes {
			let out = node
				.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
				.await
				.unwrap();
			assert!(out.fired_exec.is_empty());
			assert!(out.data.is_empty());
		}
	}

	#[test]
	fn windows_to_table_keeps_row_count() {
		let table = windows_to_table(&[WindowInfo {
			hwnd: 1,
			pid: 2,
			title: "VAC".into(),
			x: 10,
			y: 20,
			width: 640,
			height: 480,
			visible: true,
			minimized: false,
		}]);
		assert_eq!(table.len(), 1);
		assert_eq!(table.schema().len(), 9);
	}

	#[tokio::test]
	async fn enum_returns_table_or_platform_error() {
		let mut ctx = ExecCtx::default();
		let out = WindowEnumNode
			.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &exec_in_fire())
			.await
			.unwrap();
		assert!(out.fired_exec.contains("exec_out"));
		assert!(matches!(out.data.get("count"), Some(SocketValue::Int(_))));
		match out.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				#[cfg(target_os = "windows")]
				assert!(result.ok);
				#[cfg(not(target_os = "windows"))]
				{
					assert!(!result.ok);
					assert_eq!(result.code.as_deref(), Some("window.enum"));
				}
			}
			other => panic!("expected result, got {other:?}"),
		}
	}

	#[test]
	fn action_output_includes_result() {
		let ok = action_output(2, Vec::new(), "");
		match ok.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(result.ok);
				assert_eq!(result.value.as_deref(), Some(&SocketValue::Int(2)));
			}
			other => panic!("expected result, got {other:?}"),
		}

		let err = action_output(0, Vec::new(), "boom");
		match err.data.get("result").unwrap() {
			SocketValue::Result(result) => {
				assert!(!result.ok);
				assert_eq!(result.code.as_deref(), Some("window.action"));
			}
			other => panic!("expected result, got {other:?}"),
		}
	}
}
