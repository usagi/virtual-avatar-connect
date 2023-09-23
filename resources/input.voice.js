document.addEventListener("DOMContentLoaded", () =>
{
 let channel_to = document.querySelector('.text .channel')

 let auto_input = document.querySelector('.voice .auto-input')
 let continuous = document.querySelector('.voice .continuous')
 let one_shot = document.querySelector('.voice .one-shot')
 let lang = document.querySelector('.voice .lang')

 let input = document.querySelector('.text input')

 let SpeechRecognition = window.SpeechRecognition || webkitSpeechRecognition
 let SpeechGrammarList = window.SpeechGrammarList || webkitSpeechGrammarList
 let r = new SpeechRecognition()
 r.grammers = new SpeechGrammarList()
 r.continuous = true
 r.lang = 'ja-JP'
 r.interimResults = true
 r.maxAlternatives = 1

 r.onend = () =>
 {
  if (continuous.checked)
  {
   r.start()
   return
  }
  if (one_shot.checked)
  {
   continuous.disabled = false
   one_shot.checked = false
   lang.disabled = false
  }
 }

 r.onresult = async e =>
 {
  let latest = e.results[e.results.length - 1]
  let content = latest[0].transcript
  if (content == '')
   return
  let is_final = latest.isFinal
  input.value = content
  if (auto_input.checked)
   vi.post(input, is_final)
 }

 let save = () =>
 {
  input.dataset.vacChannel = channel_to.value

  localStorage.setItem('settings', JSON.stringify({
   channel: channel_to.value,

   auto_input: auto_input.checked,
   continuous: continuous.checked,
   lang: lang.value
  }))
 }

 let load = () =>
 {
  let s = JSON.parse(localStorage.getItem('settings'))
  if (s)
  {
   channel_to.value = s.channel

   auto_input.checked = s.auto_input
   continuous.checked = s.continuous
   r.lang = lang.value = s.lang

   if (continuous.checked)
   {
    one_shot.disabled = true
    r.continuous = true
    r.start()
   }
  }
  else
   r.lang = lang.value = 'ja-JP'
 }

 // let ti = document.querySelector('.text input')
 input.addEventListener('keydown', e => e.key == 'Enter' && vi.post(input, true))
 document.querySelector('.text button').addEventListener('click', () => vi.post(input, true))

 auto_input.addEventListener('input', save)

 continuous.addEventListener('input', () =>
 {
  if (continuous.checked)
  {
   one_shot.disabled = true
   lang.disabled = true
   r.continuous = true
   r.start()
  }
  else
  {
   one_shot.disabled = false
   lang.disabled = false
   r.stop()
  }
  save()
 })

 one_shot.addEventListener('input', () =>
 {
  if (one_shot.checked)
  {
   continuous.disabled = true
   lang.disabled = true
   r.continuous = false
   r.start()
  }
  else
  {
   continuous.disabled = false
   lang.disabled = false
  }
 })

 let vl_changed = () =>
 {
  if (lang_region_codes.includes(lang.value.replace('_', '-')))
   lang.classList.remove('error')
  else
   lang.classList.add('error')
  save()
 }

 lang.addEventListener('input', vl_changed)
 lang.addEventListener('change', vl_changed)
 lang.addEventListener('keydown', vl_changed)

 channel_to.addEventListener('input', save)
 channel_to.addEventListener('change', save)
 channel_to.addEventListener('keydown', save)

 load()
})
