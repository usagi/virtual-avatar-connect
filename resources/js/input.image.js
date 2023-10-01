import VacApi from './api.js'
import { ISO639_ALPHA3 } from './common/lang_code.js'
import { unorm_to_percent } from './common/number.js'
import { extract_colors } from './common/image.js'

// v: value of color element, d: delta of color element
let c = (v, d) => [v, v, v, d]

export const ARKNIGHTS_SENARIO_TEXT = { colors: [c(255, 2), c(146, 2)], min_length: 2, confidence_filter: 80 }

export default class VacInputImage
{
 settings_key = 'settings.image'

 constructor(root_element)
 {
  this.init_elements(root_element)
  this.load()
 }

 init_elements(root_element = document)
 {
  this.elements = {}
  this.elements.img = root_element.querySelector('img')
  this.elements.auto_input = root_element.querySelector('.image .auto-input')
  this.elements.continuous = root_element.querySelector('.image .continuous')
  this.elements.one_shot = root_element.querySelector('.image .one-shot')
  this.elements.lang = root_element.querySelector('.image .lang')
  this.elements.result = root_element.querySelector('.image-result input')
  this.elements.channel_to = root_element.querySelector('.image-result .channel')
  this.elements.result_post = root_element.querySelector('.image-result button')
  this.elements.preprocessor = root_element.querySelector('.image select')

  this.elements.auto_input.addEventListener('input', () => this.save())
  this.elements.continuous.addEventListener('input', () => this.save())
  this.elements.one_shot.addEventListener('input', () => this.save())

  this.elements.lang.addEventListener('input', () => this.lang_changed())
  this.elements.lang.addEventListener('change', () => this.lang_changed())
  this.elements.lang.addEventListener('keydown', () => this.lang_changed())

  // paste したら画像を読み込み
  document.onpaste = e =>
  {
   let items = [...(e.clipboardData || e.originalEvent.clipboardData).items]
   let blob = items.find(item => item.kind === 'file' && item.type.startsWith('image'))
   if (blob)
    this.image_loader(blob.getAsFile())
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

  this.elements.channel_to.addEventListener('input', () => this.save())
  this.elements.channel_to.addEventListener('change', () => this.save())
  this.elements.channel_to.addEventListener('keydown', () => this.save())

  this.elements.result.addEventListener('keydown', e => e.key == 'Enter' && window.vac.input.post(this.elements.result, true))
  this.elements.result_post.addEventListener('click', () => window.vac.input.post(this.elements.result, true))

  this.elements.continuous.addEventListener('input', () =>
  {
   if ((this.elements.one_shot.disabled = this.elements.continuous.checked) && this.elements.img.src.startsWith('data:'))
   {
    this.elements.img.recognized = false
    this.image_recognizer()
   }
  })

  this.elements.one_shot.addEventListener('input', () =>
  {
   if ((this.elements.continuous.disabled = this.elements.one_shot.checked) && this.elements.img.src.startsWith('data:'))
   {
    this.elements.img.recognized = false
    this.image_recognizer()
   }
  })

  this.elements.preprocessor.addEventListener('input', () => this.save())
 }

 lang_changed()
 {
  if (ISO639_ALPHA3.includes(lang.value))
   this.elements.lang.classList.remove('error')
  else
   this.elements.lang.classList.add('error')

  this.save()
 }

 save = () =>
 {
  this.elements.result.dataset.vacInput = this.elements.channel_to.value
  localStorage.setItem(this.settings_key, JSON.stringify({
   channel: this.elements.channel_to.value,

   auto_input: this.elements.auto_input.checked,
   continuous: this.elements.continuous.checked,
   lang: this.elements.lang.value,

   preprocessor: this.elements.preprocessor.value
  }))
 }

 load = () =>
 {
  let s = JSON.parse(localStorage.getItem(this.settings_key))
  if (s)
  {
   this.elements.result.dataset.vacInput = this.elements.channel_to.value = s.channel

   this.elements.auto_input.checked = s.auto_input
   this.elements.continuous.checked = s.continuous
   this.elements.lang.value = s.lang

   if (s.preprocessor)
    this.elements.preprocessor.value = s.preprocessor
  }
 }

 async image_recognizer()
 {
  this.elements.continuous.disabled = true
  this.elements.one_shot.disabled = true
  this.elements.lang.disabled = true
  this.elements.result.disabled = true
  this.elements.channel_to.disabled = true
  this.elements.result_post.disabled = true

  let l = this.elements.lang.value

  // pre process
  switch (this.elements.preprocessor.value.toLowerCase())
  {
   case 'arknights-story-text': extract_colors(this.elements.img, ARKNIGHTS_SENARIO_TEXT.colors); break
  }

  // recognize
  let r = await Tesseract.recognize(
   this.elements.img,
   l,
   { logger: l => this.elements.result.value = `( ... recoganizing ... ${unorm_to_percent(l.progress)}) : ${l.status}` }
  )

  // post process
  this.elements.result.value = l === 'jpn'
   ? r.data.text
    .replace(/([^\x00-\x7F])\s+([^\x00-\x7F])\s*/g, '$1$2')
    .replace('Mon③tr', 'Mon3tr')
    .replace(/[①-⑳]/g, c => String.fromCodePoint(c.charCodeAt(0) - ('①'.charCodeAt(0) - '1'.charCodeAt(0))))
    .replace(/[㉑-㉟]/g, c => String.fromCodePoint(c.charCodeAt(0) - ('㉑'.charCodeAt(0) - '㉟'.charCodeAt(0))))
    .replace(/[㊱-㊿]/g, c => String.fromCodePoint(c.charCodeAt(0) - ('㊱'.charCodeAt(0) - '㊿'.charCodeAt(0))))
   : r.data.text.trim().replace(/[\n\r]/g, '')

  this.elements.img.recognized = true

  this.elements.continuous.disabled = !this.elements.continuous.checked
  this.elements.one_shot.disabled = !this.elements.one_shot.checked
  this.elements.lang.disabled = false
  this.elements.result.disabled = false
  this.elements.channel_to.disabled = false
  this.elements.result_post.disabled = false

  if (this.elements.one_shot.checked)
  {
   this.elements.continuous.disabled = false
   this.elements.one_shot.checked = false
  }

  if (this.elements.auto_input.checked)
   window.vac.input.post(this.elements.result, true)
 }

 image_loader = async file =>
 {
  let reader = new FileReader()
  reader.onload = e =>
  {
   this.elements.img.src = e.target.result
   // 1frame 待たないと img.naturalWidth が 0 のままだったり、 src が取れなかったりするので 1frame 待つ
   requestAnimationFrame(() =>
   {
    // console.log(img.width, img.height, img.naturalWidth, img.naturalHeight)
    this.elements.img.recognized = false
    if (this.elements.continuous.checked || this.elements.one_shot.checked)
     this.image_recognizer()
   })
  }

  reader.readAsDataURL(file)
 }
}

window.VacInputImage = VacInputImage
if (!window.vac)
 window.vac = {}
window.vac.input_image = new VacInputImage()
console.log('VacInputImage loaded')
