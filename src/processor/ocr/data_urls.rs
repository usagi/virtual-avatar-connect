use super::PathOrTempFileWithMime;
use anyhow::{bail, Context, Result};
use async_tempfile::TempFile;
use base64::{engine::general_purpose, Engine as _};
use tokio::io::AsyncWriteExt;

// path_or_data_url
//  Data URLs -> tempfile + mime(decoded)
//  path      -> path     + mime(guessed)
pub async fn get(path_or_data_url: String) -> Result<PathOrTempFileWithMime> {
 let v = if path_or_data_url.starts_with("data:") {
  let (mime, blob) = match decode_data_url(&path_or_data_url) {
   Some((mime, blob)) => (mime, blob),
   None => {
    log::error!("data: URL のデコードに失敗しました。");
    bail!("data: URL のデコードに失敗しました。");
   },
  };

  // 一時ファイルを作成して blob を書き込み
  let mut tempfile = TempFile::new()
   .await
   .map_err(|e| log::error!("一時ファイルの作成に失敗しました: {:?}", e))
   .unwrap();

  tempfile
   .write_all(&blob)
   .await
   .map_err(|e| log::error!("一時ファイルへの書き込みに失敗しました: {:?}", e))
   .unwrap();

  PathOrTempFileWithMime::TempFile(tempfile, mime)
 } else {
  let mime = mime_guess::from_path(&path_or_data_url);
  let mime = mime.first_raw().with_context(|| "MIME の推定に失敗しました。")?.to_string();
  PathOrTempFileWithMime::Path(path_or_data_url, mime)
 };

 Ok(v)
}

/// Data URLs をデコード -> (MIME, Blob)
fn decode_data_url(data_url: &str) -> Option<(String, Vec<u8>)> {
 // "data:" の部分をチェック
 if !data_url.starts_with("data:") {
  return None;
 }

 // , を探して MIME タイプとデータ部分を分離
 let comma_index = match data_url.find(',') {
  Some(index) => index,
  None => return None,
 };

 // MIME タイプを取得
 let mime_type = data_url[5..comma_index]
  .split_once(';')
  .map(|(mime, _)| mime)
  .unwrap_or_default()
  .to_string();

 // Base64 エンコードされたデータを取得
 let base64_data = &data_url[comma_index + 1..];

 // Base64 デコード
 if let Ok(decoded_data) = general_purpose::STANDARD.decode(base64_data) {
  Some((mime_type, decoded_data))
 } else {
  None
 }
}
