use super::PathOrTempFileWithMime;
use crate::Result;
use anyhow::Context;

pub async fn get(path: &str) -> Result<PathOrTempFileWithMime> {
 let mime = mime_guess::from_path(path);
 let mime = mime.first_raw().with_context(|| "MIME の推定に失敗しました。")?.to_string();
 Ok(PathOrTempFileWithMime::Path(path.to_string(), mime))
}
