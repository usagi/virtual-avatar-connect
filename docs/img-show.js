export default class ImgShow
{
 constructor(root_element = document)
 {
  this.init(root_element)
 }

 init(root_element = document)
 {
  this.init_container(root_element)
  this.init_sources(root_element)
  this.init_scrollbar_width()
 }

 init_container(root_element = document)
 {
  this.showcase_container = root_element.querySelector('.img-show-case')
  this.showcase_img = this.showcase_container.querySelector('img')
  this.showcase_container.addEventListener('click', () => this.hide())
  this.showcase_container.addEventListener('wheel', event => event.stopPropagation())
  this.showcase_container.addEventListener('touchmove', event => event.stopPropagation())
 }

 init_sources(root_element = document)
 {
  this.sources = root_element.querySelectorAll('.img-show')
  for (let source of this.sources)
   source.addEventListener('click', () => this.show(source))
 }

 init_scrollbar_width()
 {
  let scrollbar_width = this.get_scrollbar_width()
  document.documentElement.style.setProperty('--img-show-scrollbar-width', `${scrollbar_width}px`)
 }

 show(img)
 {
  this.showcase_img.src = img.src
  this.showcase_img.alt = img.alt
  this.showcase_container.classList.add('show')
  this.showcase_container.style.visibility = 'visible'
 }

 hide()
 {
  this.showcase_container.classList.remove('show')
  let duration_in_secs = parseFloat(getComputedStyle(this.showcase_container).getPropertyValue('--img-show-transition-duration'))
  setTimeout(() => this.showcase_container.style.visibility = 'hidden', duration_in_secs * 1000)
 }

 get_scrollbar_width()
 {
  // スクロールバーを含むダミーの要素を作成
  const scrollDiv = document.createElement("div");
  scrollDiv.style.cssText = "width: 100px; height: 100px; overflow: scroll; position: absolute; top: -9999px;";
  document.body.appendChild(scrollDiv);

  // スクロールバーの幅を計算
  const scrollbarWidth = scrollDiv.offsetWidth - scrollDiv.clientWidth;

  // ダミーの要素を削除
  document.body.removeChild(scrollDiv);

  return scrollbarWidth;
 }
}

window.img_show = new ImgShow()
