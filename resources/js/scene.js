/// シーン管理機能を追加します。
///
///  HTML に必要な要素は以下の通りです。(※ data- 属性が必須で、要素は div でなくてもお好みで構いません)
///
///  1. ステージング中のシーンを格納する親要素
///   <div data-vac-scene-stage></div>
///  2. ウィング(ステージの袖)としてステージング中でないシーンを格納する親要素
///   <button data-vac-scene-change="シーン名">シーン名</button>
///  3. シーン要素
///   <div data-vac-scene="シーン名"></div>
///   ※一番最初に定義されたシーンが自動的に最初のシーンとしてステージング(表示)されます。
///
///  シーン切り替えの仕方は以下の通りです。
///
///   1. シーンを定義する(HTML, CSS)
///   2. シーン管理機能を有効化する: <script type="module" src="このファイル.js"></script>
///   3. シーンの切り替えを仕込んで使う:
///    例: window.vac.scene.change('シーン名')   切り替え
///    例: window.vac.scene.overlap('シーン名')  オーバーラップ
///    例: window.vac.scene.back()              戻る
///
///   必要に応じて

export class VacScene
{
 constructor()
 {
  this.stage = document.querySelector('[data-vac-scene-stage]')
  this.wing = document.querySelector('[data-vac-scene-wing]')
  this.scenes = [...this.wing.querySelectorAll(':scope > [data-vac-scene]')]

  if (this.scenes.length == 0)
   return console.error('No scene found')

  this.stage.style.display = 'grid'
  this.stage.style.gridTemplateColumns = '1fr'
  this.stage.style.gridTemplateRows = '1fr'
  for (let scene of this.scenes)
  {
   scene.style.gridColumn = '1'
   scene.style.gridRow = '1'
  }
 }

 /// 最初のシーンを開始
 start(name)
 {
  let current_scene = this.stage.querySelector(':scope > [data-vac-scene]')
  if (current_scene)
   return console.error('Scene already started')

  let start_scene = name ? this.scenes.find(s => s.dataset.vacScene == name) : this.scenes[0]
  if (!start_scene)
   return console.error(`Scene ${name} not found`)

  this.stage.appendChild(start_scene)
  start_scene.classList.add('scene-staging')
  console.log('Scene started', name)
  return true
 }

 /// シーンを変更
 change(name)
 {
  let new_scene = this.scenes.find(s => s.dataset.vacScene == name)
  if (!new_scene)
   return console.error(`Scene ${name} not found`)

  let current_scene = this.stage.querySelector(':scope > [data-vac-scene]')
  if (!current_scene)
   return console.error('No scene in stage')
  else if (current_scene == new_scene)
   return console.error(`Scene ${name} is already in stage`)

  let old_scene = this.stage.querySelector(':scope > [data-vac-scene]')
  let replaced_scene = this.stage.replaceChild(new_scene, old_scene)
  replaced_scene.classList.remove('scene-staging')
  new_scene.classList.add('scene-staging')
  this.wing.appendChild(replaced_scene)
  console.log('Scene changed', name)
  return true
 }

 /// シーンを戻す
 back()
 {
  if (this.overlap_stack.length > 0)
  {
   let remove_scene = this.overlap_stack.pop()
   this.wing.appendChild(remove_scene)
  }
  else
  {
   if (this.stage.childElementCount == 0)
    return console.error('No scene in stage')
   if (this.wing.childElementCount == 0)
    return console.error('No scene in wing')

   let previous_scene = this.stage.querySelector(':scope > [data-vac-scene]')
   let current_scene = this.wing.querySelector(':scope > [data-vac-scene]')
   let replaced_scene = this.stage.replaceChild(previous_scene, current_scene)
   replaced_scene.classList.remove('scene-staging')
   previous_scene.classList.add('scene-staging')
   this.wing.appendChild(replaced_scene)
  }
  console.log('Scene back')
  return true
 }

 /// シーンを現在のシーンの上に重ねる
 /// back() を呼ぶと重なっているシーンが戻ります
 /// memo: BRB 画面などの一時的なオーバーレイシーンにどうぞ。
 overlap_stack = []
 overlap(name)
 {
  // name が現在オーバーラップ中なら back 動作を行う
  if (this.overlap_stack.length > 0 && this.overlap_stack[this.overlap_stack.length - 1].dataset.vacScene == name)
  {
   console.warn('Scene overlapped', name, 'is already overlapped, so back')
   return this.back()
  }

  let new_scene = this.scenes.find(s => s.dataset.vacScene == name)
  if (!new_scene)
   return console.error(`Scene ${name} not found`)

  this.overlap_stack.push(new_scene)
  new_scene.classList.add('scene-staging')

  this.stage.appendChild(new_scene)
  console.log('Scene overlapped', name)
  return true
 }

}

window.VacScene = VacScene
if (!window.vac)
 window.vac = {}
window.vac.scene = new VacScene()
console.log('Scene loaded')