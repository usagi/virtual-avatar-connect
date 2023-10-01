/// RgbaImage から特定の RGB 値の色を許容誤差 d 以内で抽出して white にして、 white or black な2値化した画像として返す
pub fn _binarize(source: image::RgbaImage, r: u8, g: u8, b: u8, d: u8) -> image::GrayImage {
 let (r, g, b, d) = (r as i16, g as i16, b as i16, d as i16);
 let mut target = image::GrayImage::new(source.width(), source.height());

 // 残す色を最大化するための係数
 // let factor = 255f32 / ((r + d + g + d + b + d) / 3i16) as f32;
 let factor = 1.0f32;

 for (x, y, pixel) in source.enumerate_pixels() {
  if (pixel[0] as i16 - r).abs() <= d && (pixel[1] as i16 - g).abs() <= d && (pixel[2] as i16 - b).abs() <= d {
   let gray = (pixel[0] as f32 + pixel[1] as f32 + pixel[2] as f32) / 3f32 * factor;
   target.put_pixel(x, y, image::Luma([gray as u8]));
  } else {
   target.put_pixel(x, y, image::Luma([0]));
  }
 }

 target
}
