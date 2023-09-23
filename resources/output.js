// condition が false の場合に message を console.error しつつ throw する
function ensure(condition, message)
{
 if (!condition)
 {
  console.error(message)
  throw message
 }
}

class VacOutput
{
 /// API URL
 ///  note: 'ws://' または 'wss://' で始まる場合は websocket として扱われ、それ以外の場合は HTTP fetch による処理となります
 ///  default: 'ws://127.0.0.1:57000/' for websocket
 ///  optinn : '/output'               for POST
 api_url = 'ws://127.0.0.1:57000/'

 /// API Method
 ///  note: api_url が 'ws://' または 'ws://' で始まる場合は無視されます
 api_method = 'POST'

 /// データを更新する間隔
 fps = 10
 /// 表示を消すまでの時間
 too_old_threshold_in_ms = 40000
 /// channel -> { retrieved_id: Number elements: Set<Element> }
 request_conf = new Map()
 /// VAC から取得したデータのキャッシュ
 data_cache = null

 /// run の継続フラグ
 /// note: 通常はユーザーが直接操作する必要はないが、必要に応じて false にすると run が終了する
 continuous = true

 /// [data-vac-output="channel"] を持つ要素を探して更新対象として追加する
 retrieve_elements(root_element)
 {
  ensure(root_element instanceof Element || root_element instanceof Document, 'retrieve_elements: root_element が Element ではありません')

  for (let element of root_element.querySelectorAll('[data-vac-output]'))
  {
   let channel = element.dataset.vacOutput
   if (channel)
    this.set_channel_and_element(channel, element)
  }
  return this
 }

 /// channel と element を追加する
 /// note: 通常は retrieve_elements を使えば楽なのでユーザーは凝ったことをしない限り使う必要はない
 set_channel_and_element(channel, element)
 {
  ensure(typeof channel == 'string', 'set_channel_and_element: channel が string ではありません')
  ensure(element instanceof Element, 'set_channel_and_element: element が Element ではありません')

  if (!this.request_conf.has(channel))
   this.request_conf.set(channel, { retrieved_id: 0, elements: new Set([element]) })
  else
   this.request_conf.get(channel).elements.add(element)

  return this
 }

 /// FPS を設定する
 set_ftps(fps)
 {
  this.fps = fps
  return this
 }

 /// 表示を消すまでの時間を設定する
 set_too_old_threshold_in_ms(too_old_threshold_in_ms)
 {
  this.too_old_threshold_in_ms = too_old_threshold_in_ms
  return this
 }

 /// API URL を設定する
 set_api_url(api_url)
 {
  this.api_url = api_url
  return this
 }

 /// API Method を設定する
 set_api_method(api_method)
 {
  this.api_method = api_method
  return this
 }

 /// VAC からデータを取得する
 /// note: 通常は run を使えば楽なのでユーザーは凝ったことをしない限り使う必要はない
 async get_channel_data()
 {
  let request_payload = { channels: [] }
  for (let [channel, conf] of this.request_conf)
   request_payload.channels.push({ name: channel, retrieved_id: conf.retrieved_id, count: 1 })
  let return_value = null;
  try
  {
   let response_payload = await fetch(this.api_url, {
    method: this.api_method,
    headers: { 'Content-Type': 'application/json' },
    body: JSON.stringify(request_payload)
   })
   return_value = await response_payload.json()
  } catch (e) { throw e }
  this.data_cache = return_value
  return this
 }

 update_element(datum)
 {
  console.log(datum)
  ensure(datum instanceof Object, 'update_element: datum が Object ではありません')
  ensure(typeof datum.channel == 'string', 'update_element: datum.channel が string ではありません')
  ensure(typeof datum.content == 'string', 'update_element: datum.content が string ではありません')
  ensure(datum.flags instanceof Array, 'update_element: datum.flags が Array ではありません')

  let conf = this.request_conf.get(datum.channel)

  if (!conf)
   return

  let text = datum.content
  for (let element of conf.elements)
  {
   if (datum.flags.includes('is_final'))
    element.classList.add('is_final')
   else
   {
    text = `（…${datum.content}…💭）`
    element.classList.remove('is_final')
   }
   element.classList.remove('too_old')
   if (element.too_old_timeout_id)
    clearTimeout(element.too_old_timeout_id)
   element.too_old_timeout_id = setTimeout(() => element.classList.add('too_old'), this.too_old_threshold_in_ms)
   element.innerText = text
  }

  if (conf)
   conf.retrieved_id = Math.max(conf.retrieved_id, datum.id)
 }

 /// 要素の内容を更新する
 /// note: 通常は run を使えば楽なのでユーザーは凝ったことをしない限り使う必要はない
 update_elements()
 {
  for (let [channel, conf] of this.request_conf)
  {
   let channel_data = this.data_cache.channel_data[channel]
   if (channel_data && channel_data.length > 0)
   {
    let latest = channel_data[0]
    this.request_conf.get(channel).retrieved_id = latest.id
    let text = latest.content
    this.update_element(latest, conf.elements)
   }
  }
 }

 /// データを取得して要素の内容を更新する
 /// note: 通常は run を使えば楽なのでユーザーは凝ったことをしない限り使う必要はない
 async step()
 {
  await this.get_channel_data()
  await this.update_elements()
 }

 register_ws()
 {
  this.api = new Api({
   ws_message_event: e =>
   {
    let payload = JSON.parse(e.data)
    if (payload.channel_datum)
     this.update_element(payload.channel_datum)
    else if (payload.channel_data)
     for (let datum of payload.channel_data)
      this.update_element(datum)
   }
  })
 }

 /// これを呼ぶと自動的に処理が開始される
 run()
 {
  // チャンネルか要素が無い場合は retrieve_elements で探す
  if (this.request_conf.size == 0)
   this.retrieve_elements(document)

  // それでも無い場合は throw する
  ensure(this.request_conf.size > 0, 'run: チャンネルか要素が設定されておらず、自動的に探すこともできませんでした。')

  if (this.api_url.startsWith('ws://') || this.api_url.startsWith('wss://'))
   this.register_ws()
  else if (this.api_method == 'POST')
   // fps に合わせて this.step を繰り返す
   requestAnimationFrame(async () =>
   {
    this.continuous = true
    while (this.continuous)
     try
     {
      await this.step_rest()
      await new Promise(resolve => setTimeout(resolve, 1000 / this.fps))
     }
     catch (e)
     {
      console.error(`接続にエラーが発生しました。2秒後に自動的にリトライします: ${e}`)
      await new Promise(resolve => setTimeout(resolve, 2000))
     }
   })()
  else
   console.error('run: WebSocket でも POST でもない API は未対応です。')
 }

 stop()
 {
  if (this.ws)
  {
   this.ws.close()
   this.ws = null
  }
  this.continuous = false
 }

}

document.addEventListener('DOMContentLoaded', () => new VacOutput().run())