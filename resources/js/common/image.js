// canvas の特定の色だけを残して他を透明にする
//  img: <img>
//  colors: [[r,g,b], ...]]
export function trim_colors(canvas, colors)
{
 let ctx = canvas.getContext('2d')
 let image_data = ctx.getImageData(0, 0, canvas.width, canvas.height)
 let data = image_data.data;

 for (let n = 0; n < data.length; n += 4)
  if (colors.some(color => data[n] === color[0]
   || data[n + 1] === color[1]
   || data[n + 2] === color[2]
  ))
   data[n] = data[n + 1] = data[n + 2] = data[n + 3] = 0

 ctx.putImageData(image_data, 0, 0);
}

// img を編集する:
//  - 効果1: colors で与えられた色のピクセルだけを残して、他のピクセルを透明にする
//  - 効果2: colors で与えられた色のピクセルの範囲を検出して、その範囲でクロップする
// args:
//  img   : <img>
//  colors: [[r,g,b,d], ...]]
export function extract_colors(img, colors = [[255, 255, 255]])
{
 let canvas = document.createElement('canvas')
 canvas.width = img.naturalWidth
 canvas.height = img.naturalHeight

 let ctx = canvas.getContext('2d')
 ctx.drawImage(img, 0, 0)

 let width = img.naturalWidth
 let height = img.naturalHeight

 let imageData = ctx.getImageData(0, 0, width, height)
 let data = imageData.data

 let top = 0
 let bottom = height
 let left = 0
 let right = width

 let like_color = (data_index, color) =>
  Math.abs(data[data_index] - color[0]) <= color[3]
  && Math.abs(data[data_index + 1] - color[1]) <= color[3]
  && Math.abs(data[data_index + 2] - color[2]) <= color[3]
 // && data[data_index + 3] === 255

 // 上から下へ検索
 for (let y = 0; y < height; y++)
 {
  let rowIsEmpty = true;
  for (let x = 0; x < width; x++)
  {
   let index = (y * width + x) * 4
   if (colors.some(color => like_color(index, color)))
   {
    rowIsEmpty = false
    break
   }
  }
  if (!rowIsEmpty)
  {
   top = y
   break
  }
 }

 // 下から上へ検索
 for (let y = height - 1; y >= 0; y--)
 {
  let rowIsEmpty = true;
  for (let x = 0; x < width; x++)
  {
   let index = (y * width + x) * 4
   if (colors.some(color => like_color(index, color)))
   {
    rowIsEmpty = false
    break
   }
  }
  if (!rowIsEmpty)
  {
   bottom = y + 1
   break
  }
 }

 // 左から右へ検索
 for (let x = 0; x < width; x++)
 {
  let colIsEmpty = true;
  for (let y = top; y < bottom; y++)
  {
   let index = (y * width + x) * 4;
   if (colors.some(color => like_color(index, color)))
   {
    colIsEmpty = false
    break
   }
  }
  if (!colIsEmpty)
  {
   left = x
   break
  }
 }

 // 右から左へ検索
 for (let x = width - 1; x >= 0; x--)
 {
  let colIsEmpty = true;
  for (let y = top; y < bottom; y++)
  {
   let index = (y * width + x) * 4;
   if (colors.some(color => like_color(index, color)))
   {
    colIsEmpty = false
    break
   }
  }
  if (!colIsEmpty)
  {
   right = x + 1
   break;
  }
 }

 // 残りの範囲:
 //  1. colors の要素のどれとも一致しないピクセルを透明にする
 //  2. colors の要素のどれかと一致するピクセルは 255 にする
 for (let y = top; y < bottom; y++)
  for (let x = left; x < right; x++)
  {
   let index = (y * width + x) * 4
   data[index] = data[index + 1] = data[index + 2] = //data[index + 3] =
    (!colors.some(color => like_color(index, color)))
     ? 0
     : 255
  }

 ctx.putImageData(imageData, 0, 0)

 // 特定された範囲をクロップして新しいキャンバスに描画
 let croppedWidth = right - left;
 let croppedHeight = bottom - top;
 let croppedImageData = ctx.getImageData(left, top, croppedWidth, croppedHeight);

 // キャンバスのサイズをクロップ後のサイズに設定
 canvas.width = croppedWidth;
 canvas.height = croppedHeight;

 // クロップされたデータを描画
 ctx.putImageData(croppedImageData, 0, 0);

 // img に書き戻す
 img.src = canvas.toDataURL();
}
