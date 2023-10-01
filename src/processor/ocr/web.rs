use super::PathOrTempFileWithMime;
use crate::Result;
use async_tempfile::TempFile;
use tokio::io::AsyncWriteExt;

pub async fn get(url: &str) -> Result<PathOrTempFileWithMime> {
 log::debug!("画像をダウンロードします。 url = {:?}", url);
 let res = reqwest::get(url).await;

 if res.is_err() {
  log::error!("画像のダウンロードに失敗しました: {:?}", res);
 }

 let res = res?;

 // res -> mime
 let mime = res
  .headers()
  .get("content-type")
  .map(|h| h.to_str().unwrap_or_default().to_string())
  .unwrap_or_default();

 let bytes = res.bytes().await;

 if bytes.is_err() {
  log::error!("画像のダウンロードに失敗しました: {:?}", bytes);
 }

 let bytes = bytes?;
 let bytes = bytes.as_ref();
 let mut tempfile = TempFile::new()
  .await
  .map_err(|e| log::error!("一時ファイルの作成に失敗しました: {:?}", e))
  .unwrap();
 // tempfile に書き込み
 {
  tempfile
   .write_all(bytes)
   .await
   .map_err(|e| log::error!("一時ファイルへの書き込みに失敗しました: {:?}", e))
   .unwrap();
 }
 log::trace!("一時ファイルを作成しました: {:?}", tempfile.file_path());
 Ok(PathOrTempFileWithMime::TempFile(tempfile, mime))
}
