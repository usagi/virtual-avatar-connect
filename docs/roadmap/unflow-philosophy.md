# UN Flow Philosophy

> Status: non-normative vision note.
> 本書は `.unflow` の確定仕様ではない。構文、拡張子、UNVAC / UN Flow の命名、WASM module、capability、型システム、GUI 連携は今後の設計で変更されうる。
> ただし、VAC Flowgraph をどの方向へ伸ばすかを判断するための思想的な北極星として扱う。

## 1. 自己定義

UN Flow は、自らを「何でもできる汎用プログラミング言語」とは名乗らない。
目指す自己定義は次のもの。

> UN Flow は、UNVAC および USAGI.NETWORK 系インタラクティブシステムのための、静的型付き・副作用管理つき・イベント駆動データフロー言語である。

これは逃げではない。
領域を限定し、何が流れ、何が作用し、何が安全に接続されるのかを明示するための抑制である。

## 2. 三層構造の夢

`.unflow` は人間が書く表層言語。
`.flowgraph.toml` は安定 IR。
Runtime は型検査済みの Typed Runtime Graph を実行する。

```text
.unflow
  ↓ compile
.flowgraph.toml
  ↓ load
Typed Runtime Graph
  ↓ run
UN Flowgraph Runtime
```

GUI はこの IR の View / Editor であり、テキスト言語と敵対しない。
テキスト編集、GUI 編集、LSP、formatter、静的解析、capability 検査が、同じ IR を中心に共存することが理想形。

## 3. 圧縮された哲学

```text
Program is Graph.
Event is Cause.
Data is Lazy.
Effect is Explicit.
Capability is Permission.
Node is Boundary.
WASM is Extension.
TOML is IR.
GUI is View.
```

日本語ではこう読む。

```text
記述対象はグラフである。
イベントは原因である。
データは必要になるまで評価されない。
副作用は明示される。
capability は許可である。
ノードは境界である。
WASM は拡張である。
TOML は IR である。
GUI は View である。
```

## 4. Rust から取り入れるもの

UN Flow は Rust の構文を真似る必要はない。
取り入れるべきは、事故を防ぐ思想である。

- `Result` / `Option`: 失敗と欠損を型で表す
- `enum` / `match`: mode、event kind、command kind を文字列より強い形で表す
- capability ownership: OBS connection や process handle などの外部リソースを所有権的に扱う
- `unsafe` marker: 配信事故、OS 操作、不可逆操作を明示する

ここでいう `unsafe` はメモリ安全性ではない。
外界に作用し、ユーザーの配信や PC 状態に影響する操作の危険性を表示するための印である。

## 5. Haskell から取り入れるもの

Haskell から取り入れるべきものは、純粋性、合成性、型推論、代数的データ型である。

- pure component は lazy、cache、test、GUI preview と相性がよい
- effect component は explicit trigger と capability なしに発火しない
- pipeline は人間が意味を書き、compiler が Flowgraph IR へ落とすための表現になる
- LLM などの高コスト処理は、pure と effect の中間分類も検討する

## 6. 実行モデル

UN Flow は逐次実行言語ではない。
基本実行モデルは次の流れ。

```text
外部イベントが入る
  → 条件が評価される
  → 必要な値が pull される
  → 型と capability が検査される
  → effect node が発火する
  → 結果が別のノードへ流れる
```

data edge と exec edge は分ける。

```text
chat.exec_out => speaker.exec_in
chat.content  -> speaker.text
```

- `->`: data edge
- `=>`: exec edge
- `|>`: pipeline composition

副作用は、exec edge または event trigger なしに発火してはならない。

## 7. 構文の方向性

将来 `.unflow` を検討するとき、中心語彙はこの程度で十分だと考える。

- `module`
- `requires`
- `graph`
- `component`
- `node`
- `const`
- `let`
- `on`
- `when`
- `gate`
- `enum`
- `match`
- `pure`
- `effect`
- `resource`
- `unsafe`
- `use`

慎重に扱うもの。

- `while`
- 無制限再帰
- 暗黙のグローバル可変状態
- 魔術的マクロ
- 過剰な高階抽象
- 表層ライフタイム構文

UN Flow は研究言語ではなく、配信、アバター、AI、外部連携を安全に構成するための実用言語である。

## 8. Capability

Capability は UN Flow の安全性の核である。

```unflow
requires {
  twitch.chat.read
  twitch.chat.write
  obs.scene.write
  tts.speak
  network.http
}
```

危険操作は明示的な capability なしに実行できない。
Twitch moderation、OBS 操作、ローカルプロセス、OS ウィンドウ操作、ファイル I/O、外部 HTTP、LLM API 呼び出しは、それぞれ別 capability として扱う。

## 9. WASM as Node

WASM は重要な拡張点だが、主役ではない。
あくまで node の一種であり、Flowgraph の型検査、effect 管理、capability 管理の中に置かれる。

```unflow
node detector: wasm("./motion_filter.wasm") {
  input yaw: Angle
  input pitch: Angle
  output gaze: Vec2
}
```

WASM は計算能力と配布性を広げる。
しかし主語は「WASM で何でもできる」ではない。
主語は常に、型検査され、副作用が管理され、capability で制御される Flowgraph である。

## 10. 設計原則

1. すべての副作用は型または capability に現れる
2. pure component は lazy・cache 可能・test 可能である
3. effect component は明示的な trigger なしに実行されない
4. 外部リソースは move-only として扱える
5. 失敗は `Result` で表す
6. ない値は `Option` で表す
7. `enum` と `match` を第一級にする
8. ローカルは型推論、公開境界は明示型にする
9. `.unflow` は表層言語、`.flowgraph.toml` は IR である
10. GUI とテキストが同じ Flowgraph を扱えるようにする
11. WASM は拡張であり、言語の主役ではない
12. 逐次処理より、イベントと流れを中心に置く
13. 便利さより、事故りにくさを優先する
14. 「何でもできる」より、「何が起きるか分かる」を優先する

## 11. 標語

```text
Effects are explicit.
Events are first-class.
Flow is the subject.
```

```text
副作用は明示される。
イベントは第一級である。
流れそのものが記述対象である。
```

## 12. 現行計画との関係

この文書は以下の計画に接続する。

- [`flowgraph-language-roadmap.md`](flowgraph-language-roadmap.md): Flowgraph を言語として扱う上位計画
- [`flowgraph-language-foundation-roadmap.md`](flowgraph-language-foundation-roadmap.md): Schema / Capability / Testing / WASM module target
- [`flowgraph-stdlib-roadmap.md`](flowgraph-stdlib-roadmap.md): Ranges / LINQ / Iterator、データ構造、乱数、分布
- [`glossary-rename-roadmap.md`](glossary-rename-roadmap.md): Glossary / Dictionary / `.map` の語彙整理
- [`resident-io-roadmap.md`](resident-io-roadmap.md): File I/O、通知、OBS template、SQLite、外部データ源

`.unflow` 設計は、これらの基礎が固まった後に扱う。
今は夢でよい。

