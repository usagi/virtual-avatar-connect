let elements = document.querySelectorAll('[data-scroll-to]')
for (let element of elements)
{
 element.addEventListener('click', () =>
 {
  let target = document.querySelector(element.dataset.scrollTo)
  if (target)
   target.scrollIntoView({ behavior: 'smooth' })
  else
   window.scrollTo({ top: 0, behavior: 'smooth' });
 })
}
