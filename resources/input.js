class VacInput
{
 static TAG_ERROR = 'error'

 constructor(root_element)
 {
  root_element = root_element || document
  this.status_element = root_element.querySelector('[data-vac-status]')
  this.init_ws()
 }

 init_ws()
 {
  this.ws = new Api({
   ws_message_event: payload =>
   {
    let data = payload.channel_data || [payload.channel_datum]
    let datum = data[data.length - 1]
    if (datum)
     this.update_status_element(datum)
   }
  })
 }

 post(element, is_final)
 {
  if (this.ws)
   this.ws_post(element, is_final)
  else
   return this.rest_post(element, is_final)

  if (is_final)
   element.value = ''
 }

 to_payload(element, is_final)
 {
  let channel = element.dataset.vacInput
  console.log(element)
  let content = element.value
  let payload = { channel_datum: { channel, content } }
  if (is_final)
   payload.channel_datum.flags = ['is_final']
  return payload
 }

 ws_post(element, is_final)
 {
  let payload = this.to_payload(element, is_final)
  this.ws.ws_send(payload)
 }

 async rest_post(element, is_final)
 {
  if (input_element.value == '')
   return

  let payload = this.to_payload(element, is_final)
  let args = {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify(payload)
  }

  try
  {
   let r = await fetch('/input', args)
   let t = await r.text()
   try { this.update_status_element(JSON.stringify(JSON.parse(t), null, 1)) }
   catch (_) { this.update_status_element(t) }
  }
  catch (e) { this.update_status_element(e, true) }
 }

 update_status_element(v, is_error)
 {
  this.status_element.value = JSON.stringify(v, null, 1)
  if (is_error)
   this.status_element.classList.add(VacInput.TAG_ERROR)
  else
   this.status_element.classList.remove(VacInput.TAG_ERROR)
 }

}

document.addEventListener('DOMContentLoaded', () => window.vi = new VacInput())