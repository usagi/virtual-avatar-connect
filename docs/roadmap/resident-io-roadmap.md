# Resident I/O Roadmap

常駐型 VAC が外部データ源を Flowgraph に取り込み、OS や OBS へ出力するための次期機能波。
Flowgraph を汎用データフロー処理エンジンへ近づけるため、入力は可能な限り `Table` / JSON へ正規化し、出力は Runtime Mode の状態と通知ポリシーに従う。

## 1. 方針

- **Table 優先**: `.xlsx` / Google Sheets / RSS は、最初に Flowgraph の `Table` へ入れる。個別 API の都合を下流ノードへ漏らさない。
- **公開データ優先**: Google Sheets は OAuth なしで使える公開 URL / ID 対応から始める。認可つき読み取りは後続フェーズ。
- **常駐状態を尊重**: OS 通知と OBS 出力は、常駐アプリの現在モードに従う。睡眠モードや仕事モードで不要な通知・演出を出さない。
- **OBS テンプレート優先**: OBS は細かい部品ノードを大量に増やす前に、Browser Source 1 つで試せる完成形テンプレート出力を優先する。
- **小さく合成可能なノード**: 抽選、定数、変換は Flowgraph 標準ライブラリー候補として、小さく合成可能なノードにする。

## 2. 実装順序

### RI-1 `.xlsx -> Table`

最初の実装対象。ローカルファイルを `Table` 化できると、配信企画表、抽選表、辞書、設定表を Flowgraph に接続しやすくなる。

- Node: `flowgraph.table.load_xlsx`
- 入力: `path`, `sheet`, `range`, `header_mode`
- 出力: `table`, `sheet_name`, `row_count`, `column_count`
- sheet 省略時: 最初の表示 sheet を読む
- range 省略時: sheet の使用範囲を読む
- header: 既定は 1 行目を column name とみなす。空名や重複名は安定した suffix で正規化する
- 候補 crate: `calamine`

### RI-2 Google Sheets public URL / ID -> Table

`.xlsx` 読み込みと同じ Table 仕様へ正規化する。最初は認可なしで、公開共有・公開 CSV export に限る。

- Node: `flowgraph.table.load_google_sheet_public`
- 入力: `spreadsheet`, `sheet`, `range`, `header_mode`
- `spreadsheet` は ID だけでも URL でも許容する
- 公開されていない sheet は明確なエラーにする
- OAuth / private sheet は RI-8 へ回す

### RI-3 Table draw node

Table から行を抽選するノード。配信企画、コメント抽選、ランダム台詞、ゲーム内お題決定に直結する。

- Node: `flowgraph.table.draw`
- 入力: `table`, `count`, `weight_column`, `seed`, `replacement`
- 出力: `rows`, `row`, `indices`
- `count=1` では単一行 `row` も出す
- `replacement=false` の場合は重複なし
- 重み列は数値変換できない値を 0 扱いにし、全 0 なら通常抽選へフォールバックする

### RI-4 RSS polling / WebSub bridge

常駐 VAC が外部更新を拾う入口。まず polling で安定させ、WebSub は ingress bridge として後から追加する。

- 初期ノード: `flowgraph.web.rss_fetch`
- 入力: `url`, `limit`, `since`, `include_content`
- 出力: `items_json`, `items_table`, `latest_updated_at`
- RSS / Atom を同じ item schema へ正規化する
- WebSub は購読管理・callback URL・署名検証が必要なため、polling 実装後に独立 subphase とする

### RI-5 OS desktop notification

常駐アプリとして自然な通知出口。GUI 内 Toast ではなく OS の通知センターへ出す。

- Service: desktop runner 側の notification service
- Node: `flowgraph.notify.desktop`
- 入力: `title`, `body`, `urgency`, `group`, `icon`, `timeout_ms`
- 初期実装では action callback は持たない
- Runtime Mode 側に通知許可・抑制・重要度の policy を持たせる
- cross-platform crate が安定して使えるなら採用し、難しければ Windows 実装を先行する

### RI-6 OBS Browser Source template output

ユーザーが試しやすい完成形を優先する。OBS 側に複数ソースを手で組ませるより、VAC が Browser Source 1 つへテンプレート画面を配信する。

- Runtime: VAC の web server から template page を提供する
- Transport: template page は WebSocket / SSE / polling のいずれかで VAC の event を受け取る
- 初期テンプレート:
  - simple subtitle
  - conversation dialog
  - Twitch chat compact / rich
  - raid alert
- 後続で StreamDeck / global hotkey / MIDI と接続し、テンプレート演出の手動制御へ広げる

### RI-7 physical constants

緊急度は低いが、Flowgraph の汎用言語化には相性がよい。最初は標準ライブラリー風の pure provider として小さく入れる。

- Node candidates: `flowgraph.constants.physics`, `flowgraph.constants.math`
- 出力: `c`, `G`, `h`, `k_B` など
- 単位系・quantity 型が未整備なら、まずは明示名つき Float 定数として扱う

### RI-8 authenticated Google Sheets

public reader の実用確認後に着手する。OAuth / token storage / scope / GUI 認可導線が絡むため、最初から入れない。

- read-only scope を既定にする
- 認可状態は GUI で見えるようにする
- token は platform keyring か既存 secure storage 方針に合わせる

## 3. Toast / Notification の整理

GUI 内 Toast と OS desktop notification は別物として扱う。

- GUI Toast: 操作結果、軽いエラー、画面内フィードバック
- OS notification: VAC が背後で動いている間にユーザーへ知らせるべき外部イベント
- Flowgraph node から出す通知は OS notification 側
- Runtime Mode は通知の quiet / normal / important を制御する

## 4. 実装前提

- `Table` 型と既存 `flowgraph.table.*` ノードを入口仕様の基準にする
- node catalog / manual / GUI node picker を同時更新する
- 外部 I/O ノードは timeout / error message / retry policy を明記する
- 常駐 desktop runner と CLI runner の差は通知などの host capability として明示する
