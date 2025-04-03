// 参考: https://crates.io/crates/win-screenshot
// crop_x, crop_y, crop_w, crop_h 可能にしたかったのでとりあえず。

use std::mem::size_of;
use windows::Win32::Foundation::{ERROR_INVALID_PARAMETER, E_FAIL, HWND};
use windows::Win32::Graphics::Gdi::{
 BitBlt, CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, GetDIBits, ReleaseDC, SelectObject, StretchBlt,
 BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, SRCCOPY,
};
use windows::Win32::Storage::Xps::{PrintWindow, PRINT_WINDOW_FLAGS, PW_CLIENTONLY};
use windows::Win32::UI::HiDpi::{SetProcessDpiAwareness, PROCESS_PER_MONITOR_DPI_AWARE};
use windows::Win32::UI::WindowsAndMessaging::{
 GetSystemMetrics, PW_RENDERFULLCONTENT, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
};

use wrappers::{CreatedHdc, Hbitmap, Hdc, Rect};
mod wrappers {
 use windows::{
  core::Error,
  Win32::{
   Foundation::{HWND, RECT},
   Graphics::Gdi::{CreateCompatibleBitmap, CreateCompatibleDC, DeleteDC, DeleteObject, GetDC, ReleaseDC, HBITMAP, HDC},
   UI::WindowsAndMessaging::{GetClientRect, GetWindowRect},
  },
 };

 #[derive(Clone)]
 pub(crate) struct Hdc {
  pub(crate) hdc: HDC,
 }

 impl Hdc {
  pub(crate) fn get_dc<P0>(hwnd: P0) -> Result<Hdc, Error>
  where
   P0: Into<HWND>,
  {
   unsafe {
    match GetDC(Some(hwnd.into())) {
     e if e.is_invalid() => Err(Error::from_win32()),
     hdc => Ok(Hdc { hdc }),
    }
   }
  }
 }

 impl Drop for Hdc {
  fn drop(&mut self) {
   unsafe {
    ReleaseDC(Some(HWND::default()), self.hdc);
   }
  }
 }

 impl From<&Hdc> for HDC {
  fn from(item: &Hdc) -> Self {
   item.hdc
  }
 }

 impl From<Hdc> for HDC {
  fn from(item: Hdc) -> Self {
   item.hdc
  }
 }

 #[allow(dead_code)]
 #[derive(Debug)]
 pub(crate) struct Rect {
  //pub(crate) rect: RECT,
  pub(crate) left: i32,
  pub(crate) top: i32,
  pub(crate) right: i32,
  pub(crate) bottom: i32,
  pub(crate) width: i32,
  pub(crate) height: i32,
 }

 impl From<RECT> for Rect {
  fn from(rect: RECT) -> Self {
   Rect {
    left: rect.left,
    top: rect.top,
    right: rect.right,
    bottom: rect.bottom,
    width: rect.right - rect.left,
    height: rect.bottom - rect.top,
   }
  }
 }

 impl Rect {
  pub(crate) fn get_window_rect<P0>(hwnd: P0) -> Result<Rect, Error>
  where
   P0: Into<HWND>,
  {
   let mut rect = RECT::default();
   unsafe {
    match GetWindowRect(hwnd.into(), &mut rect) {
     Ok(_) => Ok(Rect::from(rect)),
     Err(e) => Err(e),
    }
   }
  }
  pub(crate) fn get_client_rect<P0>(hwnd: P0) -> Result<Rect, Error>
  where
   P0: Into<HWND>,
  {
   let mut rect = RECT::default();
   unsafe {
    match GetClientRect(hwnd.into(), &mut rect) {
     Ok(_) => Ok(Rect::from(rect)),
     Err(e) => Err(e),
    }
   }
  }
 }

 pub(crate) struct CreatedHdc {
  pub(crate) hdc: HDC,
 }

 impl CreatedHdc {
  pub(crate) fn create_compatible_dc(hdc: Option<HDC>) -> Result<CreatedHdc, Error> {
   unsafe {
    match CreateCompatibleDC(hdc) {
     e if e.is_invalid() => Err(Error::from_win32()),
     hdc => Ok(CreatedHdc { hdc }),
    }
   }
  }
 }

 impl From<&CreatedHdc> for HDC {
  fn from(item: &CreatedHdc) -> Self {
   HDC(item.hdc.0)
  }
 }

 impl From<CreatedHdc> for HDC {
  fn from(item: CreatedHdc) -> Self {
   HDC(item.hdc.0)
  }
 }

 impl Drop for CreatedHdc {
  fn drop(&mut self) {
   unsafe {
    let _ = DeleteDC(self.hdc);
   }
  }
 }

 pub(crate) struct Hbitmap {
  pub(crate) hbitmap: HBITMAP,
 }

 impl Hbitmap {
  pub(crate) fn create_compatible_bitmap(hdc: HDC, w: i32, h: i32) -> Result<Hbitmap, Error> {
   unsafe {
    match CreateCompatibleBitmap(hdc, w, h) {
     e if e.is_invalid() => Err(Error::from_win32()),
     hbitmap => Ok(Hbitmap { hbitmap }),
    }
   }
  }
 }

 impl Drop for Hbitmap {
  fn drop(&mut self) {
   unsafe {
    let _ = DeleteObject(self.hbitmap.into());
   }
  }
 }

 impl From<Hbitmap> for HBITMAP {
  fn from(item: Hbitmap) -> Self {
   item.hbitmap
  }
 }
}

#[derive(Debug)]
pub enum WSError {
 GetDCIsNull,
 // GetClientRectIsZero,
 CreateCompatibleDCIsNull,
 CreateCompatibleBitmapIsNull,
 SelectObjectError,
 // PrintWindowIsZero,
 GetDIBitsError,
 // GetSystemMetricsIsZero,
 StretchBltIsZero,
 // BitBltError,
}

#[derive(Clone, Copy)]
pub enum Area {
 Full,
 ClientOnly,
}

#[derive(PartialEq, Clone, Copy)]
pub enum Using {
 BitBlt,
 PrintWindow,
}

#[derive(Debug)]
pub struct RgbBuf {
 pub pixels: Vec<u8>,
 pub width: u32,
 pub height: u32,
}

pub fn capture_window_ex(
 hwnd: isize,
 using: Using,
 area: Area,
 crop_x: Option<i32>,
 crop_y: Option<i32>,
 crop_w: Option<i32>,
 crop_h: Option<i32>,
) -> Result<RgbBuf, windows::core::Error> {
 let hwnd = HWND(hwnd as *mut core::ffi::c_void);

 unsafe {
  #[allow(unused_must_use)]
  {
   SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
  }

  let hdc_screen = Hdc::get_dc(hwnd)?;

  // BitBlt support only ClientOnly
  let rect = match (using, area) {
   (Using::PrintWindow, Area::Full) => Rect::get_window_rect(hwnd),
   (Using::BitBlt, _) | (Using::PrintWindow, Area::ClientOnly) => Rect::get_client_rect(hwnd),
  }?;

  let cx = match crop_x {
   Some(v) if v > 0 => v,
   Some(v) if v < 0 => rect.width + v,
   _ => 0,
  };
  let cy = match crop_y {
   Some(v) if v > 0 => v,
   Some(v) if v < 0 => rect.height + v,
   _ => 0,
  };
  let cw = match crop_w {
   Some(v) if v > 0 => v,
   Some(v) if v < 0 => rect.width - cx + v,
   _ => rect.width - cx,
  };
  let ch = match crop_h {
   Some(v) if v > 0 => v,
   Some(v) if v < 0 => rect.height - cy + v,
   _ => rect.height - cy,
  };

  let crop = crop_x.is_some() || crop_y.is_some() || crop_w.is_some() || crop_h.is_some();

  let hdc = CreatedHdc::create_compatible_dc(Some(hdc_screen.hdc))?;

  let hbmp = match (crop, using) {
   (true, Using::BitBlt) => Hbitmap::create_compatible_bitmap(hdc_screen.hdc, cw, ch),
   (false, Using::BitBlt) | (_, Using::PrintWindow) => Hbitmap::create_compatible_bitmap(hdc_screen.hdc, rect.width, rect.height),
  }?;

  if SelectObject(hdc.hdc, hbmp.hbitmap.into()).is_invalid() {
   return Err(windows::core::Error::from_win32());
  }

  let flags = PRINT_WINDOW_FLAGS(match area {
   Area::Full => PW_RENDERFULLCONTENT,
   Area::ClientOnly => PW_CLIENTONLY.0 | PW_RENDERFULLCONTENT,
  });

  match using {
   Using::BitBlt => {
    BitBlt(hdc.hdc, 0, 0, cw, ch, Some(hdc_screen.hdc), cx, cy, SRCCOPY)?;
   },
   Using::PrintWindow => {
    if PrintWindow(hwnd, hdc.hdc, flags) == false {
     return Err(windows::core::Error::from_win32());
    }
   },
  }

  let (w, h, hdc, hbmp) = match (crop, using) {
   (true, Using::PrintWindow) => {
    let hdc2 = CreatedHdc::create_compatible_dc(Some(hdc.hdc))?;
    let hbmp2 = Hbitmap::create_compatible_bitmap(hdc.hdc, cw, ch)?;
    let so = SelectObject(hdc2.hdc, hbmp2.hbitmap.into());
    if so.is_invalid() {
     return Err(windows::core::Error::from_win32());
    }
    BitBlt(hdc2.hdc, 0, 0, cw, ch, Some(hdc.hdc), cx, cy, SRCCOPY)?;
    if SelectObject(hdc2.hdc, so).is_invalid() {
     return Err(windows::core::Error::from_win32());
    }
    (cw, ch, hdc2, hbmp2)
   },
   (true, Using::BitBlt) => (cw, ch, hdc, hbmp),
   (false, _) => (rect.width, rect.height, hdc, hbmp),
  };

  let bmih = BITMAPINFOHEADER {
   biSize: size_of::<BITMAPINFOHEADER>() as u32,
   biPlanes: 1,
   biBitCount: 32,
   biWidth: w,
   biHeight: -h,
   biCompression: BI_RGB.0 as u32,
   ..Default::default()
  };
  let mut bmi = BITMAPINFO {
   bmiHeader: bmih,
   ..Default::default()
  };
  let mut buf: Vec<u8> = vec![0; (4 * w * h) as usize];
  let gdb = GetDIBits(
   hdc.hdc,
   hbmp.hbitmap,
   0,
   h as u32,
   Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
   &mut bmi,
   DIB_RGB_COLORS,
  );
  if gdb == 0 || gdb == ERROR_INVALID_PARAMETER.0 as i32 {
   return Err(windows::core::Error::new(E_FAIL, "GetDIBits error"));
  }
  buf.chunks_exact_mut(4).for_each(|c| c.swap(0, 2));
  Ok(RgbBuf {
   pixels: buf,
   width: w as u32,
   height: h as u32,
  })
 }
}

pub fn capture_display() -> Result<RgbBuf, WSError> {
 unsafe {
  // win 8.1 temporary DPI aware
  #[allow(unused_must_use)]
  {
   SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE);
  }
  // for win 10
  //SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
  let hdc_screen = GetDC(Some(HWND::default()));
  if hdc_screen.is_invalid() {
   return Err(WSError::GetDCIsNull);
  }

  let hdc = CreateCompatibleDC(Some(hdc_screen));
  if hdc.is_invalid() {
   ReleaseDC(Some(HWND::default()), hdc_screen);
   return Err(WSError::CreateCompatibleDCIsNull);
  }

  let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
  let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
  let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
  let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

  let hbmp = CreateCompatibleBitmap(hdc_screen, width, height);
  if hbmp.is_invalid() {
   let _ = DeleteDC(hdc);
   ReleaseDC(Some(HWND::default()), hdc_screen);
   return Err(WSError::CreateCompatibleBitmapIsNull);
  }

  let so = SelectObject(hdc, hbmp.into());
  if so.is_invalid() {
   let _ = DeleteDC(hdc);
   let _ = DeleteObject(hbmp.into());
   ReleaseDC(Some(HWND::default()), hdc_screen);
   return Err(WSError::SelectObjectError);
  }

  let sb = StretchBlt(hdc, 0, 0, width, height, Some(hdc_screen), x, y, width, height, SRCCOPY);
  if sb == false {
   let _ = DeleteDC(hdc);
   let _ = DeleteObject(hbmp.into());
   ReleaseDC(Some(HWND::default()), hdc_screen);
   return Err(WSError::StretchBltIsZero);
  }

  let bmih = BITMAPINFOHEADER {
   biSize: size_of::<BITMAPINFOHEADER>() as u32,
   biPlanes: 1,
   biBitCount: 32,
   biWidth: width,
   biHeight: -height,
   biCompression: BI_RGB.0 as u32,
   ..Default::default()
  };

  let mut bmi = BITMAPINFO {
   bmiHeader: bmih,
   ..Default::default()
  };

  let mut buf: Vec<u8> = vec![0; (4 * width * height) as usize];

  let gdb = GetDIBits(
   hdc,
   hbmp,
   0,
   height as u32,
   Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
   &mut bmi,
   DIB_RGB_COLORS,
  );
  if gdb == 0 || gdb == ERROR_INVALID_PARAMETER.0 as i32 {
   let _ = DeleteDC(hdc);
   let _ = DeleteObject(hbmp.into());
   ReleaseDC(Some(HWND::default()), hdc_screen);
   return Err(WSError::GetDIBitsError);
  }

  buf.chunks_exact_mut(4).for_each(|c| c.swap(0, 2));

  let _ = DeleteDC(hdc);
  let _ = DeleteObject(hbmp.into());
  ReleaseDC(Some(HWND::default()), hdc_screen);

  Ok(RgbBuf {
   pixels: buf,
   width: width as u32,
   height: height as u32,
  })
 }
}
