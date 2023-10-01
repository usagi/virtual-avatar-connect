import { ISO3166_ALPHA2 } from './common/lang_code.js'
import VacInput from './input.js'

export default class VacInputVoice
{
 constructor(root_element)
 {
  this.init_speech_recognition()
  console.log(this)
  this.init_elements(root_element)
 }

 init_speech_recognition()
 {
  this.web_api = {
   SpeechRecognition: window.SpeechRecognition || webkitSpeechRecognition,
   SpeechGrammarList: window.SpeechGrammarList || webkitSpeechGrammarList
  }

  let r = this.speech_recognition = new this.web_api.SpeechRecognition()

  r.grammers = new this.web_api.SpeechGrammarList()
  r.continuous = true
  r.lang = 'ja-JP'
  r.interimResults = true
  r.maxAlternatives = 1

  r.onend = () =>
  {
   if (this.elements.continuous.checked)
   {
    r.start()
    return
   }
   if (this.elements.one_shot.checked)
   {
    this.elements.continuous.disabled = false
    this.elements.one_shot.checked = false
    this.elements.lang.disabled = false
   }
  }

  r.onresult = async e =>
  {
   let latest = e.results[e.results.length - 1]
   let content = latest[0].transcript
   if (content == '')
    return
   let is_final = latest.isFinal
   this.elements.input.value = content
   if (this.elements.auto_input.checked)
    window.vac.input.post(this.elements.input, is_final)
  }
 }

 init_elements(root_element = document)
 {
  this.elements = {}

  this.elements.channel_to = root_element.querySelector('.text .channel')

  this.elements.auto_input = root_element.querySelector('.voice .auto-input')
  this.elements.continuous = root_element.querySelector('.voice .continuous')
  this.elements.one_shot = root_element.querySelector('.voice .one-shot')
  this.elements.lang = root_element.querySelector('.voice .lang')

  this.elements.input = root_element.querySelector('.text input')

  this.elements.input.addEventListener('keydown', e => e.key == 'Enter' && window.vac.input.post(this.elements.input, true))
  root_element.querySelector('.text button').addEventListener('click', () => window.vac.input.post(this.elements.input, true))

  this.elements.auto_input.addEventListener('input', () => this.save())

  this.elements.continuous.addEventListener('input', () =>
  {
   if (this.elements.continuous.checked)
   {
    this.elements.one_shot.disabled = true
    this.elements.lang.disabled = true
    this.speech_recognition.continuous = true
    this.speech_recognition.start()
   }
   else
   {
    this.elements.one_shot.disabled = false
    this.elements.lang.disabled = false
    this.speech_recognition.stop()
   }
   this.save()
  })

  this.elements.one_shot.addEventListener('input', () =>
  {
   if (this.elements.one_shot.checked)
   {
    this.elements.continuous.disabled = true
    this.elements.lang.disabled = true
    this.speech_recognition.continuous = false
    this.speech_recognition.start()
   }
   else
   {
    this.elements.continuous.disabled = false
    this.elements.lang.disabled = false
   }
  })

  let vl_changed = () =>
  {
   if (ISO3166_ALPHA2.includes(this.elements.lang.value.replace('_', '-')))
    this.elements.lang.classList.remove('error')
   else
    this.elements.lang.classList.add('error')
   this.save()
  }

  this.elements.lang.addEventListener('input', () => this.vl_changed())
  this.elements.lang.addEventListener('change', () => this.vl_changed())
  this.elements.lang.addEventListener('keydown', () => this.vl_changed())

  this.elements.channel_to.addEventListener('input', () => this.save())
  this.elements.channel_to.addEventListener('change', () => this.save())
  this.elements.channel_to.addEventListener('keydown', () => this.save())

  this.load()
  this.elements.input.focus()
 }

 save()
 {
  this.elements.input.dataset.vacInput = this.elements.channel_to.value

  localStorage.setItem('settings', JSON.stringify({
   channel: this.elements.channel_to.value,

   auto_input: this.elements.auto_input.checked,
   continuous: this.elements.continuous.checked,
   lang: this.elements.lang.value
  }))
 }

 load()
 {
  let s = JSON.parse(localStorage.getItem('settings'))
  if (s)
  {
   this.elements.input.dataset.vacInput = this.elements.channel_to.value = s.channel

   this.elements.auto_input.checked = s.auto_input
   this.elements.continuous.checked = s.continuous

   this.speech_recognition.lang = this.elements.lang.value = s.lang

   if (this.elements.continuous.checked)
   {
    this.elements.one_shot.disabled = true
    this.speech_recognition.continuous = true
    this.speech_recognition.start()
   }
  }
  else
   this.speech_recognition.lang = this.elements.lang.value = 'ja-JP'
 }
}

window.VacInputVoice = VacInputVoice
if (!window.vac)
 window.vac = {}
window.vac.input_voice = new VacInputVoice()
console.log('VacInputVoice loaded')
