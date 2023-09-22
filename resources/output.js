let fps = 10
let target_channels = ['user', 'ai', 'user-en', 'ai-fr']
let too_old_threshold_in_ms = 40000

let retrieved_ids = new Map()
for (let channel_name of target_channels)
 retrieved_ids.set(channel_name, 0)

let parent_elements = new Map()

let for_each_inner_text = (p, t) =>
{
 for (let e of p.querySelectorAll(':scope > *'))
  e.innerText = t
}

let update_inner_texts = (channel_name, response) =>
{
 let channel = response.channel_data[channel_name]

 if (channel && channel.length > 0)
 {
  let latest = channel[0]
  retrieved_ids.set(channel_name, latest.id)
  let text = latest.content
  let parent_element = parent_elements.get(channel_name)
  if (latest.flags.includes('is_final'))
   parent_element.classList.add('is_final')
  else
  {
   text = `（…${latest.content}…💭）`
   parent_element.classList.remove('is_final')
  }
  parent_element.classList.remove('too_old')
  if (parent_element.too_old_timeout_id)
   clearTimeout(parent_element.too_old_timeout_id)
  parent_element.too_old_timeout_id = setTimeout(() => parent_element.classList.add('too_old'), too_old_threshold_in_ms)
  for_each_inner_text(parent_element, text)
 }
}

let loop_with_fps = async (functor, fps) =>
{
 let secs_per_frame = 1 / fps
 let functor_with_fps = async () =>
 {
  let t0 = performance.now()
  await functor()
  let dt = performance.now() - t0
  let delay = Math.max(0, secs_per_frame - dt)
  setTimeout(functor_with_fps, delay * 1000)
 }
 await functor_with_fps()
}

let get_response = async () =>
{
 let request_payload = { channels: [] }
 for (let channel_name of target_channels)
  request_payload.channels.push({ name: channel_name, retrieved_id: retrieved_ids.get(channel_name), count: 1 })

 let return_value = null;

 try
 {
  let response_payload = await fetch('/output', {
   method: 'POST',
   headers: { 'Content-Type': 'application/json' },
   body: JSON.stringify(request_payload)
  })
  return_value = await response_payload.json()
 }
 catch (e)
 {
  console.error(e)
  throw e
 }

 return return_value
}

document.addEventListener("DOMContentLoaded", async () =>
{
 for (let channel_name of target_channels)
  parent_elements.set(channel_name, document.querySelector(`[data-${channel_name}]`))

 let main = async () =>
 {
  try
  {
   let response = await get_response()
   if (response)
    for (let channel_name of target_channels)
     update_inner_texts(channel_name, response)
  }
  catch (e)
  {
   console.error(`VAC からのデータ取得に失敗しました。2秒後に自動的に再開を試みます: ${e}`)
   await new Promise(resolve => setTimeout(resolve, 2000))
  }
 }

 await loop_with_fps(main, fps);
})
