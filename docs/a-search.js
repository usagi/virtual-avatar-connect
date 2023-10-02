export default class ASearch
{
 URL_BASE = 'https://duckduckgo.com/?q='
 TARGET = 'search'

 constructor(root_element = document)
 {
  this.init(root_element)
 }

 init(root_element = document)
 {
  let elements = root_element.querySelectorAll('a.search')
  for (let element of elements)
  {
   element.href = `${this.URL_BASE}${element.innerText}`
   element.target = this.TARGET
  }

 }
}

window.a_search = new ASearch()
