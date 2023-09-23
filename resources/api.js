class Api
{
 /// arg:                  default:                 note:
 ///  ws_message_event      undefined                ハンドラーを設定すると WebSocket 接続が有効化されイベントが流れます
 ///  ws_url                'ws://127.0.0.1:57000/'  WebSocket 接続先の URL
 ///  ws_error_event        undefined                ハンドラーを設定すると WebSocket 接続でエラーが発生したときにイベントが流れます
 ///  ws_close_event        undefined                ハンドラーを設定すると WebSocket 接続が切断されたときにイベントが流れます
 ///  ws_open_event         undefined                ハンドラーを設定すると WebSocket 接続が確立されたときにイベントが流れます
 ///  ws_reconnect_interval 2000                     WebSocket 接続が切断されたときに再接続を試みる間隔 (ms)
 ///  ws_reconnect_max      10                       WebSocket 接続が切断されたときに再接続を試みる最大回数
 ///  ws_reconnect_delay    100                      WebSocket 接続が切断されたときに再接続を試みるときに遅延させる時間 (ms)
 ///
 ///  rest_input_event      undefined                ハンドラーを設定すると REST API による入力が有効化されイベントが流れます
 ///  rest_input_url        '/input'                 REST API による入力先の URL
 ///  rest_input_method     'POST'                   REST API による入力の HTTP メソッド
 ///
 ///  rest_output_event     undefined                ハンドラーを設定すると REST API による出力が有効化されイベントが流れます
 ///  rest_output_url       '/output'                REST API による出力先の URL
 ///  rest_output_method    'POST'                   REST API による出力の HTTP メソッド
 ///  rest_output_interval  100                      REST API による出力の間隔 (ms)
 constructor(arg)
 {
  if (arg.ws_message_event)
  {
   let ws_message_event = arg.ws_message_event
   let ws_url = arg.ws_url || 'ws://127.0.0.1:57000/'
   let ws_error_event = arg.ws_error_event || (() => { })
   let ws_open_event = arg.ws_open_event || (() => { })
   let ws_close_event = arg.ws_close_event || (() => { })
   let ws_reconnect_interval = arg.ws_reconnect_interval || 2000
   let ws_reconnect_max = arg.ws_reconnect_max || 10
   let ws_reconnect_delay = arg.ws_reconnect_delay || 100
   this.init_ws(ws_message_event, ws_url, ws_error_event, ws_open_event, ws_close_event, ws_reconnect_interval, ws_reconnect_max, ws_reconnect_delay)
  }

  if (arg.rest_input_event)
  {
   let rest_input_event = arg.rest_input_event
   let rest_input_url = arg.rest_input_url || '/input'
   let rest_input_method = arg.rest_input_method || 'POST'
   this.init_rest_input(rest_input_event, rest_input_url, rest_input_method)
  }

  if (arg.rest_output_event)
  {
   let rest_output_event = arg.rest_output_event
   let rest_output_url = arg.rest_output_url || '/output'
   let rest_output_method = arg.rest_output_method || 'POST'
   let rest_output_interval = arg.rest_output_interval || 100
   this.init_rest_output(rest_output_event, rest_output_url, rest_output_method, rest_output_interval)
  }
 }

 init_ws(ws_message_event, ws_url, ws_error_event, ws_open_event, ws_close_event, ws_reconnect_interval, ws_reconnect_max, ws_reconnect_delay)
 {
  this.ws = new ReconnectingWebSocket(ws_url, null, { debug: true, reconnectInterval: 2000 })
  this.ws.onmessage = m => ws_message_event(JSON.parse(m.data))
  this.ws.onerror = ws_error_event
  this.ws.onclose = ws_close_event
  this.ws.onopen = ws_open_event
  this.ws.reconnectInterval = ws_reconnect_interval
  this.ws.reconnectMax = ws_reconnect_max
  this.ws.reconnectDelay = ws_reconnect_delay
 }

 ws_send(payload) { this.ws.send(JSON.stringify(payload)) }

 stop_ws() { this.ws.close() }

 init_rest_input(rest_input_event, rest_input_url, rest_input_method)
 {
  this.rest_input_callback = rest_input_event
  this.rest_input_url = rest_input_url
  this.rest_input_method = rest_input_method
 }

 /// rest_input を呼ぶと rest_input_event に結果が流れます
 async rest_input(payload)
 {
  let args = {
   method: this.rest_input_method,
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify(payload)
  }
  let r = fetch(this.rest_input_url, args)
  r = await r
  let t = await r.text()
  this.rest_input_callback(JSON.parse(t))
 }

 init_output(rest_output_event, rest_output_url, rest_output_method, rest_output_interval)
 {
  this.rest_output_callback = rest_output_event
  this.rest_output_url = rest_output_url
  this.rest_output_method = rest_output_method
  this.rest_output_interval = rest_output_interval
  this.rest_output_continuous = false
 }

 // continuous が true の間、rest_output_interval に従って rest_output を繰り返し、結果を rest_output_event に流す
 async rest_output(payload, continuous = false)
 {
  let args = {
   method: this.rest_output_method,
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify(payload)
  }
  let loop = async () =>
  {
   if (this.rest_output_continuous)
   {
    let r = fetch(this.rest_output_url, args)
    r = await r
    let t = await r.text()
    this.rest_output_callback(JSON.parse(t))
    setTimeout(loop, this.rest_output_interval)
   }
  }
 }

 stop_rest_output() { this.rest_output_continuous = false }
}
