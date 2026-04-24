//! `flowgraph.screenshot.capture`: スクリーンショット取得 EffectfulNode。
//!
//! V1 `Screenshot` の Flowgraph 版。1 実行 = 1 キャプチャ（単一画像）で、多重クロップは
//! ユーザが `list.for_each` 相当で下流に展開する方針（Flowgraph 流のシンプルさを優先）。
//!
//! ## プラットフォーム
//!
//! - **Windows**: `crate::processor::screenshot::windows` を再利用して実装。
//! - **Non-Windows**: スタブ。常に `on_error` を発火し `error = "this platform is not supported"`。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec)
//!   - `target_title` (String, default `""`): ウィンドウタイトル完全一致。空文字ならデスクトップ全体。
//!   - `target_title_regex` (String, default `""`): ウィンドウタイトル正規表現。`target_title` より優先。
//!   - `client_only` (Bool, default `false`): クライアント領域のみキャプチャ
//!   - `use_bitblt` (Bool, default `false`): BitBlt 方式（デフォルトは PrintWindow）
//!   - `crop` (Json, default `null`): `null` = クロップなし、`{"x", "y", "w", "h"}` を int で指定
//!   - `save_path` (String, default `""`): ファイル保存先。空なら保存しない。`{T}` は ISO-8601 時刻
//! - 出力:
//!   - `on_success` (Exec) / `on_error` (Exec)
//!   - `data_url` (String): `data:image/png;base64,...`
//!   - `saved_path` (String): `save_path` 解決後のパス（保存時のみ）
//!   - `width` (Int) / `height` (Int)
//!   - `error` (String)

use crate::flowgraph::node::{
 EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct ScreenshotCaptureNode;

impl NodeDescriptor for ScreenshotCaptureNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.screenshot.capture".into(),
   title: "Screenshot Capture".into(),
   category: "screenshot".into(),
   description: Some("ウィンドウ/デスクトップのスクリーンショットを PNG data URL として取得".into()),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("target_title", "Target Title", SocketType::String)
     .with_default(SocketValue::String(String::new())),
    PortSpec::input("target_title_regex", "Target Title Regex", SocketType::String)
     .with_default(SocketValue::String(String::new())),
    PortSpec::input("client_only", "Client Only", SocketType::Bool).with_default(SocketValue::Bool(false)),
    PortSpec::input("use_bitblt", "Use BitBlt", SocketType::Bool).with_default(SocketValue::Bool(false)),
    PortSpec::input("crop", "Crop", SocketType::Json).with_default(SocketValue::Json(serde_json::Value::Null)),
    PortSpec::input("save_path", "Save Path", SocketType::String)
     .with_default(SocketValue::String(String::new())),
   ],
   outputs: vec![
    PortSpec::exec_output("on_success", "On Success"),
    PortSpec::exec_output("on_error", "On Error"),
    PortSpec::output("data_url", "Data URL", SocketType::String),
    PortSpec::output("saved_path", "Saved Path", SocketType::String),
    PortSpec::output("width", "Width", SocketType::Int),
    PortSpec::output("height", "Height", SocketType::Int),
    PortSpec::output("error", "Error", SocketType::String),
   ],
   properties: vec![],
  }
 }
}

fn err_output(msg: impl Into<String>) -> NodeOutput {
 NodeOutput::new()
  .set_data("data_url", SocketValue::String(String::new()))
  .set_data("saved_path", SocketValue::String(String::new()))
  .set_data("width", SocketValue::Int(0))
  .set_data("height", SocketValue::Int(0))
  .set_data("error", SocketValue::String(msg.into()))
  .fire_exec("on_error")
}

/// クロップ指定は 4 軸それぞれ Option（省略時は元画像の端まで）。V1 互換。
type Crop = (Option<i32>, Option<i32>, Option<i32>, Option<i32>);

/// `{T}` を ISO-8601 時刻（`:` `-` を削除）に置換。V1 互換挙動。
fn resolve_save_path(template: &str) -> String {
 if template.contains("{T}") {
  let t = jiff::Timestamp::now().to_string().replace([':', '-'], "");
  template.replace("{T}", &t)
 } else {
  template.to_string()
 }
}

fn extract_crop(v: &SocketValue) -> Option<Crop> {
 let j = match v {
  SocketValue::Json(j) => j,
  _ => return None,
 };
 let obj = j.as_object()?;
 let get_i32 = |k: &str| -> Option<i32> { obj.get(k).and_then(|v| v.as_i64()).map(|n| n as i32) };
 Some((get_i32("x"), get_i32("y"), get_i32("w"), get_i32("h")))
}

#[async_trait]
impl EffectfulNode for ScreenshotCaptureNode {
 async fn execute(
  &self,
  _ctx: &mut ExecCtx,
  _props: &InputMap,
  inputs: &InputMap,
  fired_exec: &ExecFireSet,
 ) -> Result<NodeOutput, NodeExecError> {
  if !fired_exec.contains("exec_in") {
   return Ok(NodeOutput::new());
  }

  #[cfg(not(target_os = "windows"))]
  {
   let _ = inputs; // supress unused warning on non-Windows
   return Ok(err_output("screenshot.capture is only supported on Windows"));
  }

  #[cfg(target_os = "windows")]
  {
   use crate::flowgraph::node::{get_optional_bool, get_optional_string};
   use base64::{engine::general_purpose, Engine as _};
   use image::{ImageFormat, RgbaImage};
   use std::io::Cursor;

   let target_title = get_optional_string(inputs, "target_title", "")?;
   let target_title_regex = get_optional_string(inputs, "target_title_regex", "")?;
   let client_only = get_optional_bool(inputs, "client_only", false)?;
   let use_bitblt = get_optional_bool(inputs, "use_bitblt", false)?;
   let save_path_tpl = get_optional_string(inputs, "save_path", "")?;

   let crop = inputs.get("crop").and_then(extract_crop);

   use crate::processor::screenshot::windows as winss;
   let using = if use_bitblt { winss::Using::BitBlt } else { winss::Using::PrintWindow };
   let area = if client_only { winss::Area::ClientOnly } else { winss::Area::Full };

   // ウィンドウ特定 → キャプチャ
   let capture_result: Result<winss::RgbBuf, String> = if !target_title_regex.is_empty() {
    match regex::Regex::new(&target_title_regex) {
     Ok(re) => match win_screenshot::utils::window_list() {
      Ok(list) => list
       .into_iter()
       .find(|w| re.is_match(&w.window_name))
       .map(|w| capture_hwnd(w.hwnd, using, area, crop))
       .unwrap_or_else(|| Err(format!("target_title_regex {target_title_regex:?} にマッチするウィンドウがありません"))),
      Err(e) => Err(format!("ウィンドウ一覧取得失敗: {e:?}")),
     },
     Err(e) => Err(format!("target_title_regex が無効: {e}")),
    }
   } else if !target_title.is_empty() {
    match win_screenshot::utils::find_window(&target_title) {
     Ok(hwnd) => capture_hwnd(hwnd, using, area, crop),
     Err(e) => Err(format!("target_title {target_title:?} ウィンドウが見つかりません: {e:?}")),
    }
   } else {
    winss::capture_display().map_err(|e| format!("デスクトップ キャプチャ失敗: {e:?}"))
   };

   let buf = match capture_result {
    Ok(b) => b,
    Err(e) => return Ok(err_output(e)),
   };

   let Some(img) = RgbaImage::from_raw(buf.width, buf.height, buf.pixels) else {
    return Ok(err_output("RgbaImage::from_raw に失敗（不正なバッファサイズ）"));
   };

   let mut png = Vec::new();
   if let Err(e) = img.write_to(&mut Cursor::new(&mut png), ImageFormat::Png) {
    return Ok(err_output(format!("PNG エンコード失敗: {e}")));
   }

   let saved_path = if save_path_tpl.is_empty() {
    String::new()
   } else {
    let resolved = resolve_save_path(&save_path_tpl);
    let p = std::path::Path::new(&resolved);
    if let Some(parent) = p.parent() {
     if !parent.as_os_str().is_empty() && !parent.exists() {
      if let Err(e) = std::fs::create_dir_all(parent) {
       return Ok(err_output(format!("保存先 {parent:?} 作成失敗: {e}")));
      }
     }
    }
    if let Err(e) = std::fs::write(p, &png) {
     return Ok(err_output(format!("保存失敗 {resolved}: {e}")));
    }
    resolved
   };

   let data_url = format!("data:image/png;base64,{}", general_purpose::STANDARD.encode(&png));

   Ok(
    NodeOutput::new()
     .set_data("data_url", SocketValue::String(data_url))
     .set_data("saved_path", SocketValue::String(saved_path))
     .set_data("width", SocketValue::Int(buf.width as i64))
     .set_data("height", SocketValue::Int(buf.height as i64))
     .set_data("error", SocketValue::String(String::new()))
     .fire_exec("on_success"),
   )
  }
 }
}

#[cfg(target_os = "windows")]
fn capture_hwnd(
 hwnd: isize,
 using: crate::processor::screenshot::windows::Using,
 area: crate::processor::screenshot::windows::Area,
 crop: Option<Crop>,
) -> Result<crate::processor::screenshot::windows::RgbBuf, String> {
 use crate::processor::screenshot::windows as winss;
 let (cx, cy, cw, ch) = crop.unwrap_or((None, None, None, None));
 winss::capture_window_ex(hwnd, using, area, cx, cy, cw, ch).map_err(|e| format!("キャプチャ失敗: {e:?}"))
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 async fn no_fire_is_noop() {
  let node = ScreenshotCaptureNode;
  let mut ctx = ExecCtx::default();
  let out = node
   .execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
   .await
   .unwrap();
  assert!(out.fired_exec.is_empty());
 }

 #[test]
 fn resolve_save_path_substitutes_template() {
  let a = resolve_save_path("shots/no_template.png");
  assert_eq!(a, "shots/no_template.png");
  let b = resolve_save_path("shots/snap_{T}.png");
  assert!(b.starts_with("shots/snap_"));
  assert!(b.ends_with(".png"));
  assert!(!b.contains("{T}"));
 }

 #[test]
 fn extract_crop_from_json() {
  let v = SocketValue::Json(serde_json::json!({"x": 10, "y": 20, "w": 100, "h": 200}));
  let (x, y, w, h) = extract_crop(&v).unwrap();
  assert_eq!((x, y, w, h), (Some(10), Some(20), Some(100), Some(200)));
  let null = SocketValue::Json(serde_json::Value::Null);
  assert!(extract_crop(&null).is_none());
 }

 /// 非 Windows では常に on_error が返る。
 #[cfg(not(target_os = "windows"))]
 #[tokio::test]
 async fn non_windows_returns_on_error() {
  let node = ScreenshotCaptureNode;
  let mut ctx = ExecCtx::default();
  let mut fired = ExecFireSet::new();
  fired.insert("exec_in");
  let out = node.execute(&mut ctx, &InputMap::new(), &InputMap::new(), &fired).await.unwrap();
  assert!(out.fired_exec.contains("on_error"));
 }
}
