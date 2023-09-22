document.addEventListener("DOMContentLoaded", () =>
{
 let settings_key = 'settings.image'

 let img = document.querySelector('img')
 let auto_input = document.querySelector('.image .auto-input')
 let continuous = document.querySelector('.image .continuous')
 let one_shot = document.querySelector('.image .one-shot')
 let lang = document.querySelector('.image .lang')
 let result = document.querySelector('.image-result input')
 let channel_to = document.querySelector('.image-result .channel')
 let result_post = document.querySelector('.image-result button')
 let textarea = document.querySelector('textarea')

 let save = () =>
 {
  localStorage.setItem(settings_key, JSON.stringify({
   channel: channel_to.value,

   auto_input: auto_input.checked,
   continuous: continuous.checked,
   lang: lang.value
  }))
 }

 let load = () =>
 {
  let s = JSON.parse(localStorage.getItem(settings_key))
  if (s)
  {
   channel_to.value = s.channel

   auto_input.checked = s.auto_input
   continuous.checked = s.continuous
   lang.value = s.lang
  }
 }

 load()

 auto_input.addEventListener('input', save)
 continuous.addEventListener('input', save)
 one_shot.addEventListener('input', save)

 let lang_changed = () =>
 {
  if (lang_codes.includes(lang.value))
   lang.classList.remove('error')
  else
   lang.classList.add('error')

  save()
 }

 lang.addEventListener('input', lang_changed)
 lang.addEventListener('change', lang_changed)
 lang.addEventListener('keydown', lang_changed)

 // img -> text
 image_recognizer = async () =>
 {
  continuous.disabled = true
  one_shot.disabled = true
  lang.disabled = true
  result.disabled = true
  channel_to.disabled = true
  result_post.disabled = true

  let unorm_to_percent = v =>
  {
   let p = (v * 100).toFixed(2);
   let length = p.length;
   let padding_length = 7 - length;
   return ' '.repeat(padding_length) + p + '%';
  }

  let l = lang.value
  let r = await Tesseract.recognize(img, l, { logger: l => result.value = `( ... recoganizing ... ${unorm_to_percent(l.progress)}) : ${l.status}` })

  result.value = l === 'jpn'
   ? r.data.text
    .replace(/([^\x00-\x7F])\s+([^\x00-\x7F])\s*/g, '$1$2')
    .replace('Mon③tr', 'Mon3tr')
    .replace(/[①-⑳]/g, c => String.fromCodePoint(c.charCodeAt(0) - ('①'.charCodeAt(0) - '1'.charCodeAt(0))))
    .replace(/[㉑-㉟]/g, c => String.fromCodePoint(c.charCodeAt(0) - ('㉑'.charCodeAt(0) - '㉟'.charCodeAt(0))))
    .replace(/[㊱-㊿]/g, c => String.fromCodePoint(c.charCodeAt(0) - ('㊱'.charCodeAt(0) - '㊿'.charCodeAt(0))))

   : r.data.text.trim().replace(/[\n\r]/g, '')

  if (one_shot.checked)
   one_shot.checked = false

  continuous.disabled = false
  one_shot.disabled = false
  lang.disabled = false
  result.disabled = false
  channel_to.disabled = false
  result_post.disabled = false

  if (auto_input.checked)
   await post(channel_to.value, result, true, textarea)
 }

 // file -> img
 image_loader = async file =>
 {
  let reader = new FileReader()
  reader.onload = e =>
  {
   img.src = e.target.result
   if (continuous.checked || one_shot.checked)
    image_recognizer()
  }
  reader.readAsDataURL(file)
 }

 // paste したら画像を読み込み
 document.onpaste = e =>
 {
  let items = [...(e.clipboardData || e.originalEvent.clipboardData).items]
  let blob = items.find(item => item.kind === 'file' && item.type.startsWith('image'))
  if (blob)
   image_loader(blob.getAsFile())
 }

 // drop したら画像を読み込み
 document.ondrop = e =>
 {
  e.preventDefault()
  let file = [...e.dataTransfer.files].find(file => file.type.startsWith('image'))
  if (file)
   image_loader(file)
 }
 // これもしないとブラウザーがファイルを開いてしまう
 document.ondragover = e => e.preventDefault()

 channel_to.addEventListener('input', save)
 channel_to.addEventListener('change', save)
 channel_to.addEventListener('keydown', save)

 result_post.addEventListener('click', async () => await post(channel_to.value, result, true, textarea))

 continuous.addEventListener('input', () => one_shot.disabled = continuous.checked)
 one_shot.addEventListener('input', () => continuous.disabled = one_shot.checked)

})
