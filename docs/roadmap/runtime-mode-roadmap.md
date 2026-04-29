# VAC Runtime Mode Roadmap

> Status: accepted design draft for PR review. 本書は VAC が「配信時だけ起動する支援アプリ」から「常駐型データフローアプリ」へ移行することに伴い、ユーザーの現在状態に合わせて VAC の動作状態を滑らかに切り替えるための上位計画である。実装時は `conf` / profile / Flowgraph activation / Managed App desired state を明確に分離する。

---

## 0. 結論

VAC v0 当初の主用途は、VStreamer が配信時に起動する AI パートナー機能付き配信支援アプリだった。v2 構想では VAC は常駐型データフローアプリへ移行する。したがってユーザーは、配信していない間も VAC を動かし続け、日常・仕事・睡眠・ゲーム・RTA などの現在状態に応じて動作を切り替えたくなる。

この切替は `conf.toml` の丸ごと差し替えではなく、VAC runtime 上の **Runtime Mode** として扱う。

Runtime Mode は以下をまとめる。

- Flowgraph file / group の有効化状態
- AI assistant / persona / heartbeat の動作状態
- Managed App の desired state
- ingress / notification / capability policy
- mode 遷移時に発行される runtime event

`conf` profile は起動環境の粗い切替、Runtime Mode は常駐中の滑らかな動作状態切替である。

---

## 1. 背景

### 1.1 v0 の前提

v0 当初の VAC は、配信時に立ち上げて使うアプリとして設計されていた。この前提では、OBS / Warudo / TTS / AI 会話 / Twitch 連携などを「配信中に必要なもの」としてまとめて起動しても成立した。

### 1.2 v2 の前提

v2 構想の VAC は常駐型データフローアプリである。VAC はユーザーの PC 上で長時間動作し、必要な Flowgraph だけを維持し、通知・外部イベント・AI 会話・motion hub・配信補助を状況に応じて切り替える。

この前提では、配信していない日常状態で OBS / Warudo / TTS を常時起動するのは負荷と認知ノイズになる。一方で、日常状態でも AI assistant と雑談したいユーザー、緊急地震速報や RSS 更新だけ受けたいユーザー、仕事中は通知を絞りたいユーザー、睡眠中は critical alert だけ欲しいユーザーがいる。

VAC はこの差を mode として扱う必要がある。

---

## 2. 用語

### 2.1 Conf Profile

`conf.toml` または profile API で扱う起動設定。API token、root directory、motion backend、Control API bind address など、VAC プロセスの基本環境を表す。

Profile は粗い切替であり、頻繁な mode transition の主手段にはしない。

### 2.2 Runtime Mode

VAC 常駐中に切り替える現在の動作状態。例:

- `daily`
- `streaming`
- `work`
- `sleep`
- `rta`
- `game:<title>`

Runtime Mode は conf を置き換えず、現在起動中の runtime に overlay として適用される。

### 2.3 Flowgraph Activation

ロード済み Flowgraph file / group を runtime 上で有効化・無効化する状態。初期実装では file / group 単位を基本にする。node 単位 pause/resume は後段の拡張。

### 2.4 Managed App Desired State

`run_with` 由来の外部アプリに対する mode 別の目標状態。例:

- `start`
- `stop`
- `minimize`
- `leave`

Mode Manager は現在状態との差分を取り、必要な start / stop / minimize を実行する。

---

## 3. 設計原則

### 3.1 Mode は runtime の責務

Mode 切替は Flowgraph 自身ではなく、VAC runtime の責務である。

Flowgraph 内に「自分や他の Flowgraph を ON/OFF するノード」を主機構として置くと、止めたい Flowgraph が動いていることに停止処理が依存する。また、mode の全体整合性が各 Flowgraph に分散し、GUI や Control API から現在状態を理解しにくくなる。

### 3.2 Conf 丸ごと reload は主手段にしない

指定 conf へ切り替えて reload する方式でも大雑把な mode 切替は可能だが、常駐アプリの体験としては粗い。

Runtime Mode は差分適用を基本にする。起動環境そのものを変える必要がある場合だけ profile / restart を使う。

### 3.3 Flowgraph 側 API は最小にする

Flowgraph は mode を観測し、判定し、遷移を要求できれば十分である。初期実装で追加する mode 系ノードは以下に絞る。

- `flowgraph.mode.get`
- `flowgraph.mode.equals`
- `flowgraph.mode.transit`

`on_transit` ingress node は初期実装しない。理論上は存在しうるが、`mode_changed` runtime event を既存 trigger / event 系へ流せば足りる。専用 ingress を増やすと、ユーザーに「mode 切替を検知する方法」が複数あるように見え、混乱を招く。

### 3.4 File / group 単位を初期粒度にする

初期粒度は Flowgraph file / group 単位にする。node 単位 activation は強力だが、実行中 state、edge、memoization、GUI 表示、debugger の扱いが複雑になる。

ファイル名を `.flowgraph.toml.disabled` に物理変更する手法は手動運用には使えるが、滑らかな runtime transition の中核にはしない。Mode Manager は loader metadata と runtime state で有効化状態を管理する。

---

## 4. Mode Definition

Runtime Mode は `conf.toml` 内の `[modes.*]` として宣言的に定義する。初期実装では別ファイル `modes.toml` を導入しない。mode は VAC の起動環境に従属する runtime overlay であり、`conf` と同じ validation / profile 管理の対象に置く。

例:

```toml
[modes.daily]
display_name = "Daily"
flowgraph_groups.enable = ["assistant", "alerts", "rss"]
flowgraph_groups.disable = ["streaming", "avatar"]
managed_apps.stop = ["obs", "warudo", "tts"]
ai.enabled = true
notifications.level = "normal"
capability_policy.allow = ["file_read", "network", "desktop_notification"]
capability_policy.deny = ["obs_control", "process_control"]

[modes.streaming]
display_name = "Streaming"
flowgraph_groups.enable = ["assistant", "streaming", "avatar", "obs"]
managed_apps.start = ["obs", "warudo", "tts"]
ai.enabled = true
notifications.level = "stream_safe"
capability_policy.allow = ["file_read", "file_write", "network", "obs_control", "twitch_api", "process_control"]

[modes.work]
display_name = "Work"
flowgraph_groups.enable = ["assistant", "alerts"]
managed_apps.stop = ["obs", "warudo", "tts"]
ai.enabled = true
notifications.level = "important_only"

[modes.sleep]
display_name = "Sleep"
flowgraph_groups.enable = ["emergency_alerts"]
flowgraph_groups.disable = ["assistant", "streaming", "avatar", "rss"]
managed_apps.stop = ["obs", "warudo", "tts"]
ai.enabled = false
notifications.level = "critical_only"
```

Mode 定義は「現在 mode に入ったとき、VAC がどの状態へ収束すべきか」を表す。命令列ではなく desired state である。

---

## 5. Flowgraph Metadata

Flowgraph file 側には activation のための metadata を置く。

```toml
[meta]
name = "rss-alerts"
mode_groups = ["rss", "alerts"]
default_enabled = true
```

### 5.1 `mode_groups`

Flowgraph を mode から選択するための group。Mode 定義は group を enable / disable する。

### 5.2 `default_enabled`

mode 未設定時、または旧環境での後方互換のために使う既定有効状態。既存 Flowgraph は `default_enabled = true` 相当として扱う。

### 5.3 File ID

Runtime 上では物理 path ではなく、loader が決定する stable file id を使って activation state を持つ。初期実装では canonical relative path を id としてよいが、将来 package / library 化するときは `[meta].id` を導入できる余地を残す。

---

## 6. Transition Semantics

Mode transition は best-effort な差分適用として扱う。完全 transaction / rollback は初期実装の要件にしない。

基本手順:

1. target mode を検証する
2. 現在 mode との差分 plan を作る
3. 停止対象 Flowgraph への新規 trigger 投入を止める
4. 停止対象 Flowgraph を drain できる範囲で quiesce する
5. 無効化対象 Flowgraph を inactive にする
6. 停止対象 Managed App を stop / minimize する
7. 起動対象 Managed App を start する
8. 有効化対象 Flowgraph を active にする
9. `mode_changed` event を発行する

Transition API は dry-run を持つ。

```text
POST /api/v1/control/modes/transit
GET  /api/v1/control/modes
GET  /api/v1/control/modes/current
POST /api/v1/control/modes/plan
```

`plan` は GUI の確認表示、ログ、テストに使う。

---

## 7. Flowgraph Nodes

### 7.1 `flowgraph.mode.get`

現在 mode id を返す Pure node。

Inputs:

- none

Outputs:

- `mode: string`

### 7.2 `flowgraph.mode.equals`

現在 mode が指定 mode と一致するかを返す Pure node。

Inputs:

- `mode: string`

Outputs:

- `equals: bool`

### 7.3 `flowgraph.mode.transit`

指定 mode への遷移を runtime に要求する Effectful node。

Inputs:

- `exec`
- `mode: string`
- `reason: string` optional

Outputs:

- `exec`
- `accepted: bool`
- `message: string`

このノードは即座に mode を同期的に切り替えるのではなく、Mode Manager へ transition request を送る。循環遷移を避けるため、同一 mode への transit は no-op、transition 中の再入要求は reject する。

### 7.4 実装しないノード

初期実装では `flowgraph.ingress.mode_transit` / `flowgraph.ingress.on_transit` は追加しない。

Mode 変更を Flowgraph へ通知する必要がある場合は、runtime が `mode_changed` event を既存 event / trigger 経路へ流す。専用 ingress は、既存 event 系では表現できない実ユースケースが出た段階で再検討する。

---

## 8. Managed App Integration

`run_with` は「VAC と一緒に扱う外部アプリ」の registry として維持する。Runtime Mode はその registry に対して desired state を指定する。

例:

```toml
[[run_with]]
id = "obs"
name = "OBS Studio"
command = "obs64.exe"

[[run_with]]
id = "warudo"
name = "Warudo"
command = "Warudo.exe"

[modes.streaming.managed_apps]
start = ["obs", "warudo"]

[modes.daily.managed_apps]
stop = ["obs", "warudo"]
```

Mode Manager は `run_with` の定義を直接書き換えない。書き換えるのは「mode に入ったときの desired state」だけである。

---

## 9. GUI / UX

GUI は mode を常時表示し、ユーザーが明示的に切り替えられるようにする。

初期 UI:

- current mode indicator
- mode selector
- transition plan preview
- Managed App start / stop 結果
- active / inactive Flowgraph group 表示

将来 UI:

- schedule based mode transition
- hotkey / StreamDeck / MIDI trigger
- per-mode notification policy editor
- transition failure history

---

## 10. Implementation Roadmap

### RM-0 docs

- 本書を追加
- `roadmap.md` から参照
- Flowgraph Language Roadmap との責務分離を明記

### RM-1 data model

- `RuntimeModeId`
- `RuntimeModeDefinition`
- `ModeTransitionPlan`
- `ModeTransitionState`
- mode validation

### RM-2 Control API

- mode list
- current mode read
- dry-run transition plan
- transition request
- transition status

### RM-3 Flowgraph activation

- `[meta].mode_groups`
- `[meta].default_enabled`
- loader metadata
- runtime activation state
- inactive Flowgraph への trigger 抑止

### RM-4 Flowgraph mode nodes

- `flowgraph.mode.get`
- `flowgraph.mode.equals`
- `flowgraph.mode.transit`
- node catalog / GUI palette / tests

### RM-5 Managed App integration

- mode desired state
- diff application
- transition result reporting

### RM-6 GUI

- mode selector
- plan preview
- active Flowgraph / Managed App state display

### RM-7 advanced policy

- schedule / hotkey transition
- transition guard
- notification policy
- capability policy

---

## 11. Decisions

- Runtime Mode は conf profile とは別概念にする。
- Runtime Mode 定義は初期実装では `conf.toml` 内の `[modes.*]` に置く。
- Mode transition は runtime 差分適用を基本にする。
- 初期 activation 粒度は Flowgraph file / group 単位にする。
- `.flowgraph.toml.disabled` 物理リネームは runtime mode の主機構にしない。
- Flowgraph mode node は `get` / `equals` / `transit` の 3 種に絞る。
- `on_transit` 専用 ingress node は初期実装しない。
- transition 中の再入要求は queue せず reject する。
- Managed App は mode から直接定義せず、`run_with` registry に対する desired state として扱う。
- Transition は初期実装では best-effort。dry-run plan と結果 report を必須にする。
