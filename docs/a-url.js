export default class ASearch
{
 TARGET = '_blank'

 constructor(root_element = document)
 {
  this.init(root_element)
 }

 init(root_element = document)
 {
  let elements = root_element.querySelectorAll('a.url')
  for (let element of elements)
  {
   element.href = element.innerText
   element.target = this.TARGET
  }

 }
}

window.a_search = new ASearch()
