let post = async (channel, input_element, is_final, result_element) =>
{
 if (input_element.value == '')
  return

 let content = input_element.value

 let payload = { channel, content, is_final }
 let args = {
  method: 'POST',
  headers: { 'Content-Type': 'application/json' },
  body: JSON.stringify(payload)
 }

 console.log(payload)

 try
 {
  let r = fetch('/input', args)
  if (is_final)
   input_element.value = ''
  r = await r
  let t = await r.text()
  try
  {
   result_element.value = JSON.stringify(JSON.parse(t), null, 1)
   result_element.classList.remove('error')
  }
  catch (_)
  {
   result_element.value = t
   result_element.classList.add('error')
  }
 }
 catch (e)
 {
  result_element.value = e
  result_element.classList.add('error')
 }
}
