// 関連 crate の性能評価メモ

// 汎用性は高いけど、ウィンドウタイトルからのキャプチャーができない
// 1. https://crates.io/crates/screenshots (Windows, Linux, MacOS)
//   1. capture display(UHD)     :   653ms
//        save                   :  3351ms
//   2. capture 100x100          :    33ms
//        save                   :    82ms

// Windows 専用だけど、ウィンドウタイトルからのキャプチャーもできる
// 2. https://crates.io/crates/win-screenshot (Windows only)
//   1. capture (UHDx3+1024x600) :   766ms
//        save                   : 12917ms
//   2. capture title(1024x600)  :    33ms
//        save                   :   309ms
//   3. capture bitblt(100x100)  :     0ms (但し映らない場合もある)
//        save                   :     4ms (但し写ってない場合)
//   4. capture print(100x100)   :     4ms
//        save                   :    10ms

// memo: win-screenshot を使いたいところだったけど、キャプチャー時点でもう少し器用にクロップを指定したいので参考にしつつ独自実装へ

mod ops;
#[cfg(target_os = "windows")]
mod windows;

use super::{CompletedAnd, Processor};
use crate::{ChannelDatum, ProcessorConf, ProcessorKind, SharedProcessorConf, SharedState};
use anyhow::{bail, Result};
use async_trait::async_trait;
use base64::{engine::general_purpose, Engine as _};
use core::fmt::Debug;
use image::{ImageFormat, RgbaImage};
use regex::Regex;
use std::io::Cursor;
use std::path::Path;

// linux, macos は screenshots を使用
#[cfg(not(target_os = "windows"))]
use screenshots::Screen;

#[derive(Clone)]
pub struct Screenshot {
 conf: SharedProcessorConf,
 state: SharedState,

 #[cfg(target_os = "windows")]
 target: Target,

 #[cfg(target_os = "windows")]
 using: windows::Using,

 #[cfg(target_os = "windows")]
 area: windows::Area,
}

impl Debug for Screenshot {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  write!(
   f,
   "Screenshot {{ conf: {:?}, target: {:?}, using: {:?}, area: {:?} }}",
   self.conf,
   self.target,
   match self.using {
    windows::Using::BitBlt => "BitBlt",
    windows::Using::PrintWindow => "PrintWindow",
   },
   match self.area {
    windows::Area::Full => "Full",
    windows::Area::ClientOnly => "ClientOnly",
   },
  )
 }
}

#[derive(Clone)]
enum Target {
 Desktop,
 Title(String),
 Regex(Regex),
}

impl Debug for Target {
 fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
  match self {
   Self::Desktop => write!(f, "Desktop"),
   Self::Title(t) => write!(f, "Title({:?})", t),
   Self::Regex(r) => write!(f, "Regex({:?})", r),
  }
 }
}

#[async_trait]
impl Processor for Screenshot {
 const FEATURE: &'static str = "screenshot";

 async fn process(&self, _id: u64) -> Result<CompletedAnd> {
  log::debug!("Screenshot::process() が呼び出されました。");

  let conf = self.conf.read().await.clone();
  let state = self.state.clone();
  let channel_to = conf.channel_to.as_ref().cloned();
  let paths = conf.paths.clone();
  let using = self.using.clone();
  let area = self.area.clone();

  // 4要素ではない子要素があれば4要素になるまで None で埋めつつ複製を製造
  let crops = conf
   .crops
   .iter()
   .map(|v| {
    let mut v = v.clone();
    while v.len() < 4 {
     v.push(None);
    }
    v
   })
   .collect::<Vec<_>>();

  #[cfg(target_os = "windows")]
  let target = self.target.clone();

  tokio::spawn(async move {
   // 撮影 -> 撮影用バッファー
   #[cfg(target_os = "windows")]
   let (capture_buffers, _title_captured) = match target {
    Target::Title(title) => win_screenshot::utils::find_window(&title)
     .map(|hwnd| (take_screenshots(hwnd, using, area, &crops), title.clone()))
     .map_err(|e| log::error!("指定されたタイトル {:?} のウィンドウを発見できませんでした: {:?}", title, e))
     .unwrap(),
    Target::Regex(regex) => {
     let window_list = win_screenshot::utils::window_list()
      .map_err(|e| log::error!("ウィンドウリストの取得に失敗しました: {:?}", e))
      .unwrap();
     window_list
      .into_iter()
      .find_map(|w| {
       regex
        .is_match(&w.window_name)
        .then(|| (take_screenshots(w.hwnd, using, area, &crops), w.window_name.clone()))
      })
      .ok_or_else(|| log::error!("指定されたタイトルのウィンドウの正規表現検索に失敗しました: {:?}", regex))
      .unwrap()
    },
    Target::Desktop => (vec![windows::capture_display().unwrap()], "".to_string()),
   };

   // ファイル保存 も チャンネル送信 も無い場合は何も起こらないので処理をスキップ
   if paths.is_empty() && channel_to.is_none() {
    return;
   }

   // 撮影用バッファーから画像データを生成
   let mut image_with_paths = vec![];
   let capture_buffer_len = capture_buffers.len();
   for (index, capture_buffer) in capture_buffers.into_iter().enumerate() {
    let image = RgbaImage::from_raw(capture_buffer.width, capture_buffer.height, capture_buffer.pixels).unwrap();

    // メモリー上に PNG 画像データを生成
    let mut png = Vec::new();
    let mut cur = Cursor::new(&mut png);
    image.write_to(&mut cur, ImageFormat::Png).unwrap();
    log::debug!(
     "Screenshot が撮影した画像データを生成しました。 ({}/{})",
     index + 1,
     capture_buffer_len
    );

    // ファイル保存 => 保存したパスを contents 用に保持
    let path = match paths.get(index).or_else(|| paths.get(0)) {
     Some(path) => path,
     None => {
      log::error!("paths が設定されていません。");
      return;
     },
    };

    let path = match path.contains("{T}") {
     true => {
      let t = chrono::Utc::now().to_rfc3339().replace(":", "").replace("-", "");
      let path = Path::new(&path.replace("{T}", &t)).to_path_buf();
      path
     },
     false => Path::new(path).to_path_buf(),
    };

    // ディレクトリが存在しない場合は作成
    if let Some(parent) = path.parent() {
     if !parent.exists() {
      log::warn!("設定された保存先のフォルダーが存在しないため作成します。 path = {:?}", parent);
      std::fs::create_dir_all(parent).unwrap();
     }
    }

    log::debug!("Screenshot が撮影した画像データを {} に保存します。", path.display());
    // image を path へ保存
    std::fs::write(&path, &png).unwrap();
    log::trace!("Screenshot が撮影した画像データを保存しました。");

    // channel_to 送信用に保持
    image_with_paths.push((png, path.to_string_lossy().to_string()));
   }

   // チャンネル送信
   let output = image_with_paths
    .into_iter()
    .map(|(png, path)| match conf.to_data_urls {
     Some(true) => {
      let base64_encoded_data = general_purpose::STANDARD.encode(&png);
      format!("data:image/png;base64,{}", base64_encoded_data)
     },
     _ => path,
    })
    .collect::<Vec<_>>();

   if let Some(channel_to) = channel_to {
    log::debug!(
     "{} 件の撮影情報(Path or Data URLs)を JSON 配列で {} へ送信します。",
     output.len(),
     channel_to
    );

    let output_contents_json = serde_json::to_string(&output).unwrap();
    // チャンネル送信
    let channel_datum = ChannelDatum::new(channel_to.clone(), output_contents_json).with_flag(ChannelDatum::FLAG_IS_FINAL);
    {
     let state = state.read().await;
     state.push_channel_datum(channel_datum).await;
    }
   }
  });

  Ok(CompletedAnd::Next)
 }

 fn conf(&self) -> SharedProcessorConf {
  self.conf.clone()
 }

 async fn new(pc: &ProcessorConf, state: &SharedState) -> Result<ProcessorKind> {
  #[cfg(target_os = "windows")]
  let target = match (&pc.title, &pc.title_regex) {
   (Some(title), _) => Target::Title(title.clone()),
   (_, Some(title_regex)) => Target::Regex(Regex::new(&title_regex)?),
   _ => Target::Desktop,
  };

  #[cfg(target_os = "windows")]
  let area = if pc.client_only {
   windows::Area::ClientOnly
  } else {
   windows::Area::Full
  };

  #[cfg(target_os = "windows")]
  let using = if pc.bitblt {
   windows::Using::BitBlt
  } else {
   windows::Using::PrintWindow
  };

  let mut p = Screenshot {
   conf: pc.as_shared(),
   state: state.clone(),

   #[cfg(target_os = "windows")]
   target,

   #[cfg(target_os = "windows")]
   using,

   #[cfg(target_os = "windows")]
   area,
  };

  if !p.is_established().await {
   bail!("Screenshot が正常に設定されていません: {:?}", pc);
  }

  Ok(ProcessorKind::Screenshot(p))
 }

 async fn is_channel_from(&self, channel_from: &str) -> bool {
  let conf = self.conf.read().await;
  conf.channel_from.as_ref().unwrap() == channel_from
 }

 async fn is_established(&mut self) -> bool {
  let conf = self.conf.read().await;

  if conf.channel_from.is_none() {
   log::error!("channel_from が設定されていません。");
   return false;
  }

  let m_channel_to = match &conf.channel_to {
   Some(v) => format!(" channel_to={:?}", v),
   None => String::new(),
  };

  #[cfg(target_os = "windows")]
  let m_target = match (&conf.title, &conf.title_regex) {
   (Some(t), _) => format!("{:?} (ウィンドウタイトル完全一致)", t),
   (_, Some(r)) => format!("{:?} (ウィンドウタイトル正規表現)", r),
   _ => "(デスクトップ全体)".to_string(),
  };
  #[cfg(not(target_os = "windows"))]
  {
   if t.is_some() || r.is_some() {
    log::warn!("title, title_regex は Windows でのみ有効です。");
   }
   let m_target = "(デスクトップ全体)".to_string();
  }

  // client_only が true の場合はウィンドウの枠を除いたクライアント領域のみが対象となる
  #[cfg(target_os = "windows")]
  let m_client_only = match conf.client_only {
   true => "有効",
   _ => "無効",
  }
  .to_string();

  #[cfg(not(target_os = "windows"))]
  if conf.client_only.is_some() {
   log::warn!("client_only は Windows でのみ有効です。無視されます。");
  }

  #[cfg(target_os = "windows")]
  let m_bitblt = match conf.bitblt {
   true => {
    log::warn!("BitBlt モードが設定されています。対象のウィンドウによっては BitBlt モードではキャプチャーできない場合があります。キャプチャーに失敗する場合は bitblt = false を設定してください。");
    "有効"
   },
   _ => "無効",
  }.to_string();
  #[cfg(not(target_os = "windows"))]
  if conf.bitblt.is_some() {
   log::warn!("bitblt は Windows でのみ有効です。");
  }

  #[cfg(target_os = "windows")]
  log::info!(
   "Screenshot は正常に設定されています: channel_from={:?}{} target={} client_only={} bitblt={} crops={:?} paths={:?}",
   conf.channel_from,
   m_channel_to,
   m_target,
   m_client_only,
   m_bitblt,
   conf.crops,
   conf.paths
  );
  #[cfg(not(target_os = "windows"))]
  log::info!(
   "Screenshot は正常に設定されています: channel_from={:?}{} target={} crops={:?} paths={:?}",
   conf.channel_from,
   m_channel_to,
   m_target,
   conf.crops,
   conf.paths
  );

  true
 }
}

/// hwnd -> (buffers, title)
#[cfg(target_os = "windows")]
fn take_screenshots(hwnd: isize, using: windows::Using, area: windows::Area, crops: &Vec<Vec<Option<i32>>>) -> Vec<windows::RgbBuf> {
 if crops.is_empty() {
  vec![windows::capture_window_ex(hwnd, using, area, None, None, None, None).unwrap()]
 } else {
  crops
   .iter()
   .map(|crop| {
    let (crop_x, crop_y, crop_w, crop_h) = (crop[0], crop[1], crop[2], crop[3]);
    windows::capture_window_ex(hwnd, using, area, crop_x, crop_y, crop_w, crop_h).unwrap()
   })
   .collect::<Vec<_>>()
 }
}
