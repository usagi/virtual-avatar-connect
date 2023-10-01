export function escape_inner_text(text)
{
 var map = {
  '&': '&amp;',
  '<': '&lt;',
  '>': '&gt;',
  '"': '&quot;',
  "'": '&#039;'
 };

 return text.replace(/[&<>"']/g, function (m) { return map[m]; });
}

export function escape_quotes(text)
{
 return text.replace(/'/g, "\\'");
}