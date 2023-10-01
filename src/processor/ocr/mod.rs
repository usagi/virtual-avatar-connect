mod data_urls;
mod local;
mod web;

use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedChannelData, SharedState};
use anyhow::{bail, ensure, Result};
use async_tempfile::TempFile;
use async_trait::async_trait;
use std::path::Path;
use unic_langid::LanguageIdentifier;

#[derive(Debug, Clone)]
pub struct Ocr {
 conf: ProcessorConf,
 state: SharedState,
 channel_data: SharedChannelData,
 lang: String,
}

pub enum PathOrTempFileWithMime {
 Path(String, String),
 TempFile(TempFile, String),
}

#[async_trait]
impl Processor for Ocr {
 const FEATURE: &'static str = "ocr";

 async fn process(&self, id: u64) -> Result<CompletedAnd> {
  log::debug!("Ocr::process() が呼び出されました。");

  let conf = self.conf.clone();
  let state = self.state.clone();
  let channel_data = self.channel_data.clone();
  let channel_to = self.conf.channel_to.as_ref().unwrap().clone();
  let lang = self.lang.to_lowercase();
  let lines = self.conf.lines.unwrap_or_default();
  let auto_delete_processed_file = self.conf.auto_delete_processed_file.unwrap_or_default();

  tokio::spawn(async move {
   // conf と contents に応じて画像ファイルをローカルに生成ないしパスを保持
   // 処理後にファイルを削除する tmpfile の場合は remove に true をセット

   // 処理対象群を格納
   let mut targets = vec![];

   // conf.load_from から画像ファイルを取得して処理対象に追加
   let load_from_len = conf.load_from.len();
   for (index, url_or_path_pattern) in conf.load_from.iter().enumerate() {
    let target = match url_or_path_pattern.as_str() {
     web_url if web_url.starts_with("http://") || web_url.starts_with("https://") => web::get(web_url).await,
     file_url if file_url.starts_with("file:///") => local::get(&file_url[8..]).await,
     local_path => local::get(&local_path).await,
    };

    match target {
     Ok(target) => targets.push(target),
     Err(e) => {
      log::error!("画像ファイルの取得に失敗しました ({}/{}): {:?}", index + 1, load_from_len, e);
      continue;
     },
    }
   }

   // conf.load_from_contents が true なら contents から画像ファイルを取得して処理対象に追加
   if conf.load_from_content.unwrap_or_default() {
    // channel_data.read ロック -> ChannelDatum を取得
    let datum = {
     let data = channel_data.read().await;
     let datum = data.iter().rev().find(|cd| cd.get_id() == id);
     ensure!(datum.is_some(), "指定された id の ChannelDatum が見つかりませんでした: {}", id);
     datum.unwrap().clone()
    };

    // 確定入力の場合のみ処理
    if !datum.has_flag(ChannelDatum::FLAG_IS_FINAL) {
     log::trace!("未確定の入力なので、処理をスキップします。");
     return Ok(());
    }

    let content_list = match serde_json::from_str::<Vec<String>>(&datum.content) {
     Ok(content_list) => content_list,
     Err(e) => {
      log::error!("content を [\"path-or-dataurl\", ... ] 形式として読み込めませんでした: {:?}", e);
      bail!("content を [\"path-or-dataurl\", ... ] 形式として読み込めませんでした: {:?}", e);
     },
    };

    if content_list.is_empty() {
     log::warn!("contents は空でした。");
    }

    let content_list_len = content_list.len();
    for (index, target_maybe) in content_list.into_iter().enumerate() {
     match data_urls::get(target_maybe).await {
      Ok(target) => targets.push(target),
      Err(e) => {
       log::error!(
        "data_url から画像への変換に失敗しました ({}/{}): {:?}",
        index + 1,
        content_list_len,
        e
       );
       continue;
      },
     }
    }
   }

   // 処理対象がなければ終了
   if targets.is_empty() {
    log::warn!("処理対象がありませんでした。");
    return Ok(());
   }

   let targets_len = targets.len();
   for (index, target) in targets.into_iter().enumerate() {
    let display_index = index + 1;

    // ocr
    let output = match &target {
     PathOrTempFileWithMime::Path(path, mime) => ocr(path, mime, &lang, lines),
     PathOrTempFileWithMime::TempFile(tempfile, mime) => ocr(tempfile.file_path(), mime, &lang, lines),
    };

    let output = match output {
     Ok(output) => output,
     Err(e) => {
      log::error!("OCR に失敗しました ({}/{}): {:?}", display_index, targets_len, e);
      continue;
     },
    };

    if let Some(true) = conf.check_result_lang {
     log::trace!("OCR の結果から言語を推定します。");
     let info = whatlang::detect(&output);
     match info {
      Some(info) => {
       log::debug!("OCR の結果から言語を推定しました: info={:?}", info);
       let l2 = crate::utility::iso_639_lang_code_3_to_2(info.lang().code());
       log::trace!("OCR の結果から言語を推定しました: l2={:?}", l2);
       let ref_lang = crate::utility::iso_3166_to_lang_code(&lang);
       log::trace!("ref_lang = {:?}", ref_lang);
       match (l2, ref_lang) {
        (Ok(l2), Ok(ref_lang)) if l2 == ref_lang => log::trace!(
         "OCR の結果から推定した言語が設定された言語と一致しました: estimated={:?} lang={:?}",
         l2,
         ref_lang
        ),
        (Ok(l2), Ok(ref_lang)) => {
         log::error!(
          "OCR の結果から言語を推定しましたが、設定された言語と一致しませんでした: estimated={:?} lang={:?}",
          l2,
          ref_lang
         );
         continue;
        },
        (Ok(_), Err(e)) => {
         log::error!("推定された言語コードの変換に失敗しました: estimated_err={:?}", e);
         continue;
        },
        (Err(e), Ok(_)) => {
         log::error!("設定の lang からの言語部分の抽出に失敗しました: lang_err={:?}", e);
         continue;
        },
        (Err(e1), Err(e2)) => {
         log::error!(
          "推定された言語コードの変換および設定の lang からの言語部分の抽出に失敗しました: estimated_err={:?} lang_err={:?}",
          e1,
          e2
         );
         continue;
        },
       }
      },
      None => {
       log::warn!(
        "OCR の結果から言語を推定できなかったため処理をスキップします。 ({}/{}): {:?}",
        display_index,
        targets_len,
        output
       );
       continue;
      },
     }
    }

    // lock state -> push datum
    {
     let state = state.read().await;
     // push datum
     state
      .push_channel_datum(ChannelDatum::new(channel_to.clone(), output).with_flag(ChannelDatum::FLAG_IS_FINAL))
      .await;
    }

    // auto remove file
    if auto_delete_processed_file {
     match target {
      // 画像ファイルを削除
      PathOrTempFileWithMime::Path(path, _) => {
       if let Err(e) = tokio::fs::remove_file(&path).await {
        log::error!("画像ファイルの削除に失敗しました ({}/{}): {:?}", display_index, targets_len, e)
       }
      },
      PathOrTempFileWithMime::TempFile(tempfile, _) => drop(tempfile),
     }
    }
   }

   Ok(())
  });

  Ok(CompletedAnd::Next)
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  // TODO: Windows 以外のサポート
  #[cfg(not(target_os = "windows"))]
  {
   log::error!("OCR Processor はいまのところ Windows のみ対応です。");
   bail!("OCR Processor はいまのところ Windows のみ対応です。");
  }

  let lang: String = match pc.lang.as_ref().map(|s| s.parse::<LanguageIdentifier>()) {
   Some(Ok(lang)) => lang.to_string(),
   _ => bail!("lang が不正です: {:?}", pc.lang),
  };

  let mut p = Ocr {
   conf: pc.clone(),
   state: state.clone(),
   channel_data: state.read().await.channel_data.clone(),
   lang,
  };

  if !p.is_established().await {
   bail!("ocr が正常に設定されていません: {:?}", pc);
  }

  Ok(ProcessorKind::Ocr(p))
 }

 fn is_channel_from(&self, channel_from: &str) -> bool {
  self.conf.channel_from.as_ref().unwrap() == channel_from
 }

 async fn is_established(&mut self) -> bool {
  if self.conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }

  if self.conf.channel_to.is_none() {
   log::error!("channel_to が設定されていません。");
   return false;
  }

  if self.conf.channel_from == self.conf.channel_to {
   log::error!(
    "この Processor ({:?}) では不慮の短絡的な無限ループの発生を防ぐ目的と比較的重い処理時間で同期ブロックを行わないように工夫するため channel_from と channel_to は同一に設定できません。",
    Self::FEATURE
   );
   return false;
  }

  if self.conf.lang.is_none() {
   log::error!("lang が設定されていません。");
   return false;
  }

  log::info!(
   "Ocr は正常に設定されています: channel: {:?} -> {:?} lang: {:?}",
   self.conf.channel_from,
   self.conf.channel_to,
   self.lang
  );
  true
 }
}

// OCR; win-ocr だと lines が取れなかったのでとりあえずコンパクトに直実装しちゃう
fn ocr<P: AsRef<Path>, M: AsRef<str>>(path: P, mime: M, lang: &str, lines: bool) -> Result<String> {
 log::trace!(
  "ocr() が呼び出されました。 path = {:?}, lang = {:?}, lines = {:?}",
  path.as_ref(),
  lang,
  lines
 );

 use std::fs;
 use windows::{
  core::HSTRING,
  Globalization::Language,
  Graphics::Imaging::BitmapDecoder,
  Media::Ocr::OcrEngine,
  Storage::{FileAccessMode, StorageFile},
 };

 let path = path.as_ref();
 let path = fs::canonicalize(path);
 let path = match path {
  Ok(path) => path.to_string_lossy().replace("\\\\?\\", ""),
  Err(ref e) => anyhow::bail!("ファイルのパスが不正です: {:?} path={:?}", e, path),
 };

 // mime
 let mime = mime.as_ref();
 let id = match mime.to_lowercase().as_str() {
  "image/png" => BitmapDecoder::PngDecoderId(),
  "image/jpeg" => BitmapDecoder::JpegDecoderId(),
  "image/gif" => BitmapDecoder::GifDecoderId(),
  "image/bmp" | "image/x-ms-bmp" => BitmapDecoder::BmpDecoderId(),
  "image/tiff" => BitmapDecoder::TiffDecoderId(),
  "image/webp" => BitmapDecoder::WebpDecoderId(),
  "image/x-icon" | "image/vnc.microsoft.icon" => BitmapDecoder::IcoDecoderId(),
  "image/vnd.ms-photo" => BitmapDecoder::JpegXRDecoderId(),
  "image/heif" | "image/heic" => BitmapDecoder::HeifDecoderId(),
  _ => anyhow::bail!("サポートされていないファイル形式です: {:?}", mime),
 }?;

 let file = StorageFile::GetFileFromPathAsync(&HSTRING::from(path))?.get()?;

 let bmp = BitmapDecoder::CreateWithIdAsync(id, &file.OpenAsync(FileAccessMode::Read)?.get()?)?
  .get()?
  .GetSoftwareBitmapAsync()?
  .get()?;

 let langs = lang.clone();
 let lang = Language::CreateLanguage(&HSTRING::from(lang))?;
 let engine = OcrEngine::TryCreateFromLanguage(&lang)?;

 let result = engine.RecognizeAsync(&bmp)?.get()?;
 log::trace!("text = {:?}", result.Text()?);
 let output = if lines {
  result
   .Lines()?
   .into_iter()
   .map(|line| remove_whitespace_if_cjk(line.Text().unwrap().to_string_lossy(), &langs))
   .collect::<Vec<_>>()
   .join("\n")
 } else {
  remove_whitespace_if_cjk(result.Text()?.to_string_lossy(), &langs)
 };
 Ok(output)
}

/// CJK なら whitespace を削除
fn remove_whitespace_if_cjk(content: String, lang: &str) -> String {
 if lang.starts_with("ja") || lang.starts_with("ko") || lang.starts_with("zh") {
  return content.chars().filter(|c| !c.is_whitespace()).collect::<String>();
 } else {
  return content;
 }
}
