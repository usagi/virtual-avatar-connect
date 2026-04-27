# VAC GUI Visual Style Guide

> Status: visual baseline + initial theme engine draft. VAC GUI は「常駐ランタイムの管制卓」として、わかりやすく、使いやすく、さり気なくかっこいい見た目を目指す。派手なスキンやブランド演出へ進む前に、まず情報密度・色・余白・状態表現の基準を揃える。

---

## 1. 目標

VAC v2 は、配信時だけ開く支援ツールではなく、ユーザーのPC上に常駐するデータフローアプリである。GUI は「今VACが何をしているか」「何に注意すべきか」「どこまで安全に変更できるか」を即座に示す必要がある。

ルック・アンド・フィールは、次の印象を目標にする。

- 知的: 状態、根拠、操作結果が整理されている。
- クール: 無駄な装飾を避け、線・余白・状態色で引き締める。
- 工学的: ログ、ID、mode、Flowgraph、外部アプリの関係が読み解ける。
- VStreamer向け: 配信環境に置いても見栄えが悪くなく、使っていること自体が少し気分を上げる。
- 拡張可能: 将来の skin / theme で「かっこいい」「かわいい」方向へ分岐できる。

Dr.USAGI の印象としては、知的でクールな工学系ドクターの道具であることを基準にする。これは個人ブランドを前面に出すという意味ではなく、VAC の設計判断に「落ち着いた専門性」「精密さ」「使っていてダサくないこと」を通すという意味である。

---

## 2. 基本原則

### 2.1 Operational First

画面は「操作説明」よりも「現在状態」を優先する。Now / Modes / Resources / Observability / Settings / Flowgraph Studio は、それぞれ次の問いに答える。

- Now: 今どう動いているか。
- Modes: どの生活・配信状態として動くか。
- Resources: 外部アプリや認証は使える状態か。
- Observability: 何が起きたか、何を根拠に判断できるか。
- Settings: 永続設定や危険操作をどこで扱うか。
- Flowgraph Studio: VAC の挙動をどう編集・debugするか。

### 2.2 Dense but Calm

常駐アプリのGUIは、情報を薄く広げすぎない。カードを大きく飾るより、比較・確認・操作に必要な情報密度を保つ。

- 余白は落ち着きを作るために使う。
- 巨大な hero / marketing 風レイアウトは使わない。
- 1画面に収めるべき情報を、装飾カードで無駄に分断しない。
- ただし詰め込みすぎて読めない状態にはしない。

### 2.3 Cool without Noise

かっこよさは、色数や発光やグラデーションで盛ることではない。VAC では、整列、コントラスト、状態色、控えめな奥行き、専門的な語彙の自然な混在で表現する。

避けるもの:

- 紫・青紫の強いグラデーション一辺倒
- 丸すぎるカードや pill の多用
- 装飾用の orb / blob / bokeh
- やたら大きい見出し
- マーケティングサイト風の hero
- 意味のないアイコンや絵文字

許容するもの:

- 薄い境界線
- 控えめな状態色
- 必要最小限の shadow
- 小さな icon button
- 等幅フォントによる ID / path / PID / event 表示
- Flowgraph や runtime 状態を連想させる line / grid / panel 構成

---

## 3. 色

### 3.1 ベース

既定 theme は OS theme に従う dark / light を基準にする。独自 theme は `data-vac-theme` と localStorage による軽い theme engine で切り替える。

- dark: 長時間表示しても疲れにくい低輝度。真っ黒ではなく、surface の階層で分ける。
- light: 開発・設定作業で読みやすい白背景。境界線と状態色を強めすぎない。
- accent: primary は操作可能性を示すために使い、装飾色として濫用しない。

### 3.2 状態色

状態色は意味を固定する。

| 状態 | 役割 |
| --- | --- |
| success | 稼働中、接続済み、完了 |
| warning | 要確認、遷移中、部分成功 |
| error | 失敗、切断、破壊的操作 |
| primary | 主操作、選択中、現在 focus |
| surface | 通常状態、非アクティブ、補助情報 |

色だけに依存しない。ラベル、アイコン、数値、説明文も併用する。

---

## 4. タイポグラフィ

- 日本語本文はOS標準の読みやすさを優先する。
- 見出しは小さめ・太めでよい。hero-scale は使わない。
- 数値、ID、path、PID、event kind、Flowgraph node ID は等幅フォントを使う。
- letter spacing は原則 0。uppercase の補助ラベルだけ既存Tailwindの範囲で使う。
- ボタン内テキストは短くし、長い説明は tooltip / helper text へ逃がす。

---

## 5. レイアウト

### 5.1 Shell

上部ヘッダ、左 rail、下部 status bar は「管制卓」の骨格である。派手にせず、常に現在状態と主要操作へ戻れる構造を保つ。

- header: app identity、接続状態、global action
- rail: Now / Modes / Flowgraph Studio / Resources / Observability / Settings
- status bar: runtime heartbeat、mode、簡易状態

### 5.2 Panels

カードは「繰り返し項目」「状態単位」「操作単位」に使う。ページ全体をカードの入れ子にしない。

- section は原則 full-width band または unframed layout。
- panel は境界線で軽く分ける。
- dashboard card は数値と状態を短く見せる。
- 危険操作は同じ場所に固め、普段の状態確認画面から少し距離を置く。

### 5.3 Flowgraph Studio

Flowgraph Studio は compact IDE として扱う。

- file tree / canvas / inspector / problems の関係を崩さない。
- editor 操作は icon + tooltip を優先し、必要なものだけ text button にする。
- node / edge / diagnostics は「構造が読める」ことを最優先する。
- 将来の group / subgraph / trace 表示を前提に、余白と階層を使い切らない。

---

## 6. Skin / Theme への備え

初期実装は1つの堅実な visual baseline に集中する。将来 skin を持つ場合は、次の方向を想定する。

- `dr-usagi-default`: 既定。Dr.USAGI 通常仕様をイメージした intelligence & engineering。知的、硬質、低ノイズ。
- `dark-crimson`: 赤 / 黒 / 金を主軸にした cool & dark。配信画面に置いて映えるが、警告色との衝突に注意する。
- `light-silver`: 白 / 青 / 銀を主軸にした cool & light。明るい作業環境でも読みやすく、清潔で工学的な印象を保つ。
- `soft-cute`: かわいい custom 路線。角丸・色味・柔らかさは増やすが、操作性と情報密度は維持する。

skin は見た目の差し替えであり、情報設計や操作語彙を変えない。初期 theme engine は CSS token の差し替えに留め、画面構造や文言は theme ごとに分岐しない。

初期テーマエンジンで用意するのは上記4系統までで十分。テーマ数を増やすより、token 設計、状態色の意味、dark/light の視認性、Flowgraph Studio の可読性を優先する。

テーマの初期検証は Playwright の screenshot smoke に留める。固定 PNG baseline による visual regression は、フォント、密度、theme token が安定してから導入する。

---

## 7. 実装順

1. Visual baseline を文書化する。
2. 現行CSSの token / utility 方針を整理する。
3. Now / Modes / Resources / Observability / Settings の panel density と見出し階層を揃える。
4. Flowgraph Studio の toolbar / inspector / problems の密度と状態表現を揃える。
5. Theme token と `data-vac-theme` による初期 theme engine を追加する。
6. Theme ごとの細部調整は、既定 baseline と Playwright screenshot 確認を前提に段階投入する。

最初のPRでは、文書化とごく小さいCSS補助に留める。見た目の大改修は、Playwright screenshot と実機確認を前提に段階投入する。
