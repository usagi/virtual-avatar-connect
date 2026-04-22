//! `flowgraph.ocr.recognize`: 画像から文字列を抽出する EffectfulNode。
//!
//! V1 `Ocr` の Flowgraph 版。
//!
//! ## プラットフォーム
//!
//! - **Windows** のみ実装（Windows Media OCR を利用）。
//! - その他プラットフォームでは `on_error` を即返す。
//!
//! ## ポート
//!
//! - 入力:
//!   - `exec_in` (Exec)
//!   - `source` (String): ローカルパス / `file:///...` / `http(s)://...` / `data:image/...` のいずれか
//!   - `lang` (String): BCP-47（例: `"ja-JP"`, `"en-US"`）
//!   - `lines` (Bool, default `false`): true なら `Lines()` 単位、false なら `Text()` 全文
//!   - `check_result_lang` (Bool, default `false`): 認識結果に whatlang をかけ `lang` と一致しなければ error
//! - 出力:
//!   - `on_success` / `on_error` (Exec)
//!   - `text` (String)
//!   - `error` (String)
//!
//! ## マルチ入力について
//!
//! V1 の `load_from` (配列) は Flowgraph では **ユーザ側の for_each 相当で展開する** 方針。
//! このノードは 1 回発火 = 1 画像の仕様で、発火側で loop を組む。

use crate::flowgraph::node::{
 EffectfulNode, ExecCtx, ExecFireSet, InputMap, NodeDescriptor, NodeExecError, NodeOutput, NodeSpec, PortSpec,
};
use crate::flowgraph::socket::{SocketType, SocketValue};
use async_trait::async_trait;

pub struct OcrRecognizeNode;

impl NodeDescriptor for OcrRecognizeNode {
 fn describe(&self) -> NodeSpec {
  NodeSpec {
   feature: "flowgraph.ocr.recognize".into(),
   title: "OCR Recognize".into(),
   category: "ocr".into(),
   description: Some("画像ソースから文字列を OCR 抽出する（Windows のみ）".into()),
   inputs: vec![
    PortSpec::exec_input("exec_in", "Exec"),
    PortSpec::input("source", "Source", SocketType::String),
    PortSpec::input("lang", "Lang (BCP-47)", SocketType::String),
    PortSpec::input("lines", "Per Lines", SocketType::Bool).with_default(SocketValue::Bool(false)),
    PortSpec::input("check_result_lang", "Check Result Lang", SocketType::Bool)
     .with_default(SocketValue::Bool(false)),
   ],
   outputs: vec![
    PortSpec::exec_output("on_success", "On Success"),
    PortSpec::exec_output("on_error", "On Error"),
    PortSpec::output("text", "Text", SocketType::String),
    PortSpec::output("error", "Error", SocketType::String),
   ],
   properties: vec![],
  }
 }
}

fn err_output(msg: impl Into<String>) -> NodeOutput {
 NodeOutput::new()
  .set_data("text", SocketValue::String(String::new()))
  .set_data("error", SocketValue::String(msg.into()))
  .fire_exec("on_error")
}

#[async_trait]
impl EffectfulNode for OcrRecognizeNode {
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
   let _ = inputs; // suppress unused warning
   return Ok(err_output("ocr.recognize is only supported on Windows"));
  }

  #[cfg(target_os = "windows")]
  {
   use crate::flowgraph::node::{get_optional_bool, get_required_string};
   use crate::processor::ocr::{data_urls, local, web, PathOrTempFileWithMime};

   let source = get_required_string(inputs, "source")?;
   let lang = get_required_string(inputs, "lang")?;
   let lines = get_optional_bool(inputs, "lines", false)?;
   let check_result_lang = get_optional_bool(inputs, "check_result_lang", false)?;

   if source.is_empty() {
    return Ok(err_output("source が空です"));
   }
   if lang.is_empty() {
    return Ok(err_output("lang が空です"));
   }

   // URL / local path / data URL を PathOrTempFileWithMime へ解決
   let resolved: Result<PathOrTempFileWithMime, String> = if source.starts_with("http://") || source.starts_with("https://") {
    web::get(&source).await.map_err(|e| format!("web::get: {e:?}"))
   } else if let Some(stripped) = source.strip_prefix("file:///") {
    local::get(stripped).await.map_err(|e| format!("local::get: {e:?}"))
   } else if source.starts_with("data:") {
    data_urls::get(source.clone()).await.map_err(|e| format!("data_urls::get: {e:?}"))
   } else {
    local::get(&source).await.map_err(|e| format!("local::get: {e:?}"))
   };

   let target = match resolved {
    Ok(t) => t,
    Err(e) => return Ok(err_output(e)),
   };

   let text_result = match &target {
    PathOrTempFileWithMime::Path(p, m) => crate::processor::ocr::recognize(p, m, &lang, lines),
    PathOrTempFileWithMime::TempFile(tf, m) => crate::processor::ocr::recognize(tf.file_path(), m, &lang, lines),
   };

   let text = match text_result {
    Ok(t) => t,
    Err(e) => return Ok(err_output(format!("OCR 失敗: {e}"))),
   };

   if check_result_lang {
    if let Some(info) = whatlang::detect(&text) {
     let estimated_l3 = info.lang().code();
     let estimated_l2 = isolang::Language::from_639_3(estimated_l3)
      .and_then(|l| l.to_639_1())
      .map(|s| s.to_string());
     // lang は "ja-JP" 等なので `-` 前を取り出して比較
     let ref_l2 = lang.split(['-', '_']).next().unwrap_or("").to_lowercase();
     match estimated_l2 {
      Some(l2) if l2 == ref_l2 => {}
      Some(l2) => {
       return Ok(err_output(format!(
        "OCR 結果の言語推定が不一致: estimated={l2} ref={ref_l2}"
       )));
      }
      None => {
       return Ok(err_output(format!(
        "OCR 結果の推定言語コード変換に失敗: {estimated_l3}"
       )));
      }
     }
    } else {
     return Ok(err_output("OCR 結果から言語を推定できませんでした"));
    }
   }

   Ok(
    NodeOutput::new()
     .set_data("text", SocketValue::String(text))
     .set_data("error", SocketValue::String(String::new()))
     .fire_exec("on_success"),
   )
  }
 }
}

#[cfg(test)]
mod tests {
 use super::*;

 #[tokio::test]
 async fn no_fire_is_noop() {
  let node = OcrRecognizeNode;
  let mut ctx = ExecCtx::default();
  let out = node
   .execute(&mut ctx, &InputMap::new(), &InputMap::new(), &ExecFireSet::new())
   .await
   .unwrap();
  assert!(out.fired_exec.is_empty());
 }

 /// 非 Windows では常に on_error を返す。
 #[cfg(not(target_os = "windows"))]
 #[tokio::test]
 async fn non_windows_returns_on_error() {
  let node = OcrRecognizeNode;
  let mut ctx = ExecCtx::default();
  let mut fired = ExecFireSet::new();
  fired.insert("exec_in");
  let inputs: InputMap = [
   ("source".into(), SocketValue::String("/tmp/x.png".into())),
   ("lang".into(), SocketValue::String("en-US".into())),
  ]
  .into_iter()
  .collect();
  let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
  assert!(out.fired_exec.contains("on_error"));
 }

 /// Windows でも存在しないファイルを指定すると on_error が返る。
 #[cfg(target_os = "windows")]
 #[tokio::test]
 async fn missing_file_returns_on_error() {
  let node = OcrRecognizeNode;
  let mut ctx = ExecCtx::default();
  let mut fired = ExecFireSet::new();
  fired.insert("exec_in");
  let inputs: InputMap = [
   ("source".into(), SocketValue::String("C:/__nonexistent__/xx.png".into())),
   ("lang".into(), SocketValue::String("en-US".into())),
  ]
  .into_iter()
  .collect();
  let out = node.execute(&mut ctx, &InputMap::new(), &inputs, &fired).await.unwrap();
  assert!(out.fired_exec.contains("on_error"));
 }
}
