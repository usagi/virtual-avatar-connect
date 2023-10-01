import { escape_inner_text, escape_quotes } from '/resources/js/common/escape.js'

/// note: container 相当の親要素または set_height_limit で指定した高さを超えると、その高さになるまで自動的に縮小する。
/// 例えば OBS で 1920x450 でプロパティーを設定した場合、高さは 450 までに自動的にフォントサイズが調整される。
/// 但し、特にフォントサイズに transision を設定している場合は僅かな時間、上部の描画が切れてしまう。
/// この対策として、 set_height_limit で高さを指定できる。OBS側で少し高さに余裕を持たせておき、 set_height_limit で
/// 本来の表示領域の高さを指定する。
/// .js で制御したくない場合は、URL のフラグメント部の最後にに ,高さ として指定することもできる。
/// つまり、次のように実用することができる:
///  目標表示領域: 1920x450
///  OBS のプロパティー: 1920x675 URL のフラグメント部: #450
/// こうすると、常時高さ 450 となるよう調整され、かつ調整アニメーションの間も上部が切れれなくなる。
///
/// さて、実はこのクラスはそもそもフラグメント部で ch1, ch2 の2つのチャンネルのパラメーターも受け取る。
/// そういうわけで、設定可能なすべてのパラメーターを省略しなかった場合、
/// 最終的には次のような URL フラグメント部になる:
///
///  #ch1-name|red;ch1-suffix|blue;ch1-prefix|green`ch2-name|purple;ch2-suffix|yellow;ch2-prefix|aliceblue`450
///
///   ※制限事項: 色の指定は % 文字をそのまま使用できません。%を使わない表現にするか、 % を %25 と記述(URLエンコード)して指定してください。🙏
///
///  1. , <-- ch1 ` ch2 ` 高さ(省略可能) の区切り文字
///  2. ; <-- ch-name | ch-suffix(省略可能) | ch-prefix(省略可能) の区切り文字
///  3. | <-- part | color の区切り文字
///
/// この3種類の区切り文字があります。初期設計時にはここまでパラメーターが増えると思っていなかったのです。🙏
///
export default class VacOutputDisplayFor2ChMix
{
 chs = []
 suffixies = []
 prefixies = []
 parents = []
 childrens = []
 colors = { chs: [null, null], suffixies: [null, null], prefixies: [null, null] }

 constructor()
 {
  let [ch1conf, ch2conf, height_limit] = decodeURI(location.hash.slice(1)).replaceAll("'", "\\'").split('`')

  if (height_limit)
   this.set_height_limit(parseFloat(height_limit));

  [ch1conf, ch2conf]
   .map((conf, index) =>
   {
    // [ ch|col , suffix|col , prefix|col ] <= ch|col;suffix|col;prefix|col
    let [ch_conf, suffix_conf, prefix_conf] = conf.split(';')

    if (!ch_conf)
    {
     console.error('2-channel-subtitles: チャンネルが指定されていません。URLのフラグメント部に #ch1`ch2 のように設定します。')
     return
    }

    let [ch, ch_color] = ch_conf.split('|')
    this.chs.push(ch)
    if (ch_color)
     this.colors.chs[index] = ch_color

    if (suffix_conf)
    {
     let [suffix, suffix_color] = suffix_conf.split('|')
     this.suffixies.push(suffix)
     if (suffix_color)
      this.colors.suffixies[index] = suffix_color
    }

    if (prefix_conf)
    {
     let [prefix, prefix_color] = prefix_conf.split('|')
     this.prefixies.push(prefix)
     if (prefix_color)
      this.colors.prefixies[index] = prefix_color
    }
   })

  this.chs.map((ch, index) =>
  {
   let parent = document.querySelector(`.ch${index + 1}`)
   console.log('parent', parent)
   console.log('ch', ch)
   parent.classList.add(ch)
   this.parents.push(parent)

   let children = [...parent.querySelectorAll('[data-vac-output]')]
   this.childrens.push(children)
   children.map((child, index) =>
   {
    child.setAttribute('data-vac-output', ch)
   })

  })

  this.inner_text_update_observers = [this.childrens[0][0], this.childrens[1][0]].map(c0 =>
  {
   let observer = new MutationObserver(async (m, _) => { m.forEach(async m => m.type === 'childList' && await this.adjustFontSize()) })
   observer.observe(c0, { childList: true, subtree: false })
   return observer
  })

  this.set_colors()
  this.set_prefix()
  this.set_suffix()

  document.body.addEventListener('keydown', e =>
  {
   switch (e.key)
   {
    case '1': this.test1(); break
    case '2': this.test2(); break
   }
  })
  document.body.addEventListener('click', () => this.test2())

  for (let index of [0, 1])
  {
   console.log(`--- channel ${index+1} initialized ---`)
   console.log('name: ', this.chs[index])
   console.log('color: ', this.colors.chs[index] ?? '(default)')
   console.log('suffix: ', this.suffixies[index] ?? '(none)')
   console.log('suffix-color: ', this.colors.suffixies[index] ?? '(default)')
   console.log('prefix: ', this.prefixies[index] ?? '(none)')
   console.log('prefix-color: ', this.colors.prefixies[index] ?? '(default)')
  }
  console.log('--- height limit ---')
  console.log('height limit: ', this.height_limit)
 }

 set_colors()
 {
  for (let index of [0, 1])
  {
   let ch_color = this.colors.chs[index]
   if (ch_color)
    this.childrens[index][1].style.setProperty(`--ch${index + 1}-color`, ch_color)

   let suffix_color = this.colors.suffixies[index]
   if (suffix_color)
    this.parents[index].style.setProperty(`--ch${index + 1}-suffix-color`, suffix_color)

   let prefix_color = this.colors.prefixies[index]
   if (prefix_color)
    this.parents[index].style.setProperty(`--ch${index + 1}-prefix-color`, prefix_color)
  }
 }

 /// .ch1>*, .ch2>* それぞれに prefixes を ::before の content として設定
 /// style で var(--ch1-prefix), var(--ch2-prefix) として使える。
 set_prefix()
 {
  for (let index of [0, 1])
  {
   let prefix = this.prefixies[index]
   if (prefix)
    this.parents[index].style.setProperty(`--ch${index + 1}-prefix`, `'${prefix}'`)
  }
 }

 /// .ch1>*, .ch2>* それぞれに suffixies を ::after の content として設定
 /// style で var(--ch1-suffix), var(--ch2-suffix) として使える。
 set_suffix()
 {
  for (let index of [0, 1])
  {
   let suffix = this.suffixies[index]
   if (suffix)
    this.parents[index].style.setProperty(`--ch${index + 1}-suffix`, `'${suffix}'`)
  }
 }

 height_limit = Number.MAX_SAFE_INTEGER
 set_height_limit(h)
 {
  if (!isFinite(h))
  {
   console.error('2-channel-subtitles: 高さの指定が不正です。URLのフラグメント部に #ch1`ch2,高さ のように設定します。')
   return
  }
  this.height_limit = h
 }

 prev_factor = 1.0
 prev_total_char_count = 0
 is_transisioning = false
 async adjustFontSize()
 {
  if (this.is_transisioning)
   return

  // ch1, ch2 の文字数の合計
  let total_char_count = this.childrens[0][0].innerText.length + this.childrens[1][0].innerText.length
  let delta_char_count = total_char_count - this.prev_total_char_count
  if (delta_char_count === 0)
   return

  // parents の parent の高さ = このコンテンツ全体のコンテナーの高さ (factor = 1.0 相当の値で計算)
  let hc = this.parents[0].parentElement.clientHeight / this.prev_factor
  // parents の parent の parent の高さ = 親要素が本来収まるべき高さ
  let hl = Math.min(this.parents[0].parentElement.parentElement.clientHeight, this.height_limit)
  // 全体で必要な縮小率
  let factor = Math.min(hl / hc, 1.0)
  let delta_factor = factor - this.prev_factor
  console.log('delta_char_count', delta_char_count, 'delta_factor', delta_factor)
  // 怪現象を回避
  if (delta_char_count >= 0 && delta_factor >= 0)
   factor = this.prev_factor

  // ch1 の高さ
  let h0 = this.parents[0].clientHeight
  // ch2 の高さ
  let h1 = this.parents[1].clientHeight
  // ch1 と ch2 の高さの構成比
  let r0 = h0 / hc
  let r1 = h1 / hc

  // factor の ch1, ch2 への逆比率での分配
  let f0 = factor * r1
  let f1 = factor * r0

  // ch1, ch2 に高さファクターを適用
  this.parents[0].style.fontSize = `${factor * 100}%`
  this.parents[1].style.fontSize = `${factor * 100}%`

  if (factor !== this.prev_factor)
  {
   this.is_transisioning = true
   this.parents[0].addEventListener('transitionend', () => this.is_transisioning = false, { once: true })
  }

  this.prev_factor = factor
  this.prev_total_char_count = total_char_count
 }

 TEST_TEXT = [
  '医の神アポローン、アスクレーピオス、ヒュギエイア、パナケイア、および全ての神々よ。私自身の能力と判断に従って、この誓約を守ることを誓う。この誓いを守り続ける限り、私は人生と医術とを享受し、全ての人から尊敬されるであろう！しかし、万が一、この誓いを破る時、私はその反対の運命を賜るだろう。',
  'ὅρκον μὲν οὖν μοι τόνδε ἐπιτελέα ποιέοντι, καὶ μὴ συγχέοντι, εἴη ἐπαύρασθαι καὶ βίου καὶ τέχνης δοξαζομένῳ παρὰ πᾶσιν ἀνθρώποις ἐς τὸν αἰεὶ χρόνον: παραβαίνοντι δὲ καὶ ἐπιορκέοντι, τἀναντία τούτων.'
 ]
 /// 1秒で20文字ずつ多く表示してゆくテスト。
 /// 開始から10秒後に200文字表示されるが、最後の1文字は … となる。
 /// 開始から20秒後にすべての文字を消して終了。
 /// テストで出力される文字列は TEST_TEXT を ch1, ch2 でそれぞれ使用する。
 test_alredy_running = false
 async test1()
 {
  if (this.test_alredy_running)
  {
   console.log('test already running')
   return
  }
  this.test_alredy_running = true
  console.log('test start')
  // 0 .. 20secs
  for (let time = 0; time < 20; time++)
  {
   console.log('test step time 0..20secs', time)
   for (let index of [0, 1])
   {
    let text = this.TEST_TEXT[index]
    let length = Math.floor(text.length * time / 20)
    let output = text.slice(0, length)
    if (output.length >= 200)
     output = output.slice(0, 199) + '…'
    this.childrens[index].map(child => child.innerText = output)
   }
   await new Promise(resolve => setTimeout(resolve, 1000))
  }
  // 20 .. 30secs
  console.log('test waiting 30secs)')
  await new Promise(resolve => setTimeout(resolve, 10000))
  for (let index of [0, 1])
   this.childrens[index].map(child => child.innerText = '')
  this.test_alredy_running = false
 }

 /// 呼ばれる度に ch1, ch2 に TEST_TEXT [0] [1] から 1 文字追加する。
 test_2_cursor = 0
 test_2_prev_char = [null, null]
 async test2()
 {
  console.log('test2 cursor', this.test_2_cursor)
  for (let index of [0, 1])
  {
   let cursor = this.test_2_cursor % this.TEST_TEXT[index].length
   let c = this.TEST_TEXT[index][cursor]
   let p = this.test_2_prev_char[index] ?? ''
   if (this.childrens[index][0].innerText[this.childrens[index][0].innerText.length - 1] === p)
    p = ''
   this.childrens[index].map(child => child.innerText = `${child.innerText}${p}${c}`)
   this.test_2_prev_char[index] = c
   console.log('test2', index, cursor, c, p)
  }
  ++this.test_2_cursor
 }
}

window.VacOutputDisplayFor2ChMix = VacOutputDisplayFor2ChMix
if (!window.vac)
 window.vac = {}
window.vac.output_display_for_2_ch_mix = new VacOutputDisplayFor2ChMix()
console.log('VacOutputDisplayFor2ChMix loaded')