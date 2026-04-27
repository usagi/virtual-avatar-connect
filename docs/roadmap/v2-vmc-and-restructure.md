# VAC v2 再構造化 + VMC 対応 計画書（改訂版）

## 0. 方針

* VAC を **常駐型のデータフローエンジン** とし、まず **VMC（UDP）のモーショントラッキング** を扱えるようにする。その最初の機能として **並列パススルー**（マルチキャスト的転送）を可能にする。
* Flowgraph を **実行記述（プログラム）としての位置づけ** を文書・実装の両面で明確にする。
* **crate 分離**は段階的に進め、責務と保守性を整理する。

**記法**: 本書の **`Phase M0`〜`M5`** は VMC / Motion 作業用のラベルであり、[`roadmap.md`](../roadmap.md) の **δ / φ …** などギリシャ文字フェーズとは別系統（同ファイル **Phase M** 節で進捗管理）。Rust 型・モジュール名は `` `Snake_case` `` / `` `PascalCase` ``、HTTP は `ALL CAPS` パス表記。

**M0 詳細設計（実装・`conf.toml` スキーマの正本）**: [`phase-mu-vmc-motion-m0.md`](phase-mu-vmc-motion-m0.md)

### 0.1 横断ロードマップ（Flowgraph Language / Runtime Mode）と担当分割

[`flowgraph-language-roadmap.md`](flowgraph-language-roadmap.md) は、Flowgraph を **常駐型データフロー処理エンジン**および **型付きノード言語**へ伸ばす上位計画（Stage 0〜8、§8 Decisions、既存 Phase δ/λ/…との関係）の正本である。[`runtime-mode-roadmap.md`](runtime-mode-roadmap.md) は、常駐 VAC における **Runtime Mode**（`[modes.*]`、Flowgraph file/group の activation、Managed App の desired state、初期ノードは `flowgraph.mode.get` / `equals` / `transit` のみ、`on_transit` 専用 ingress は入れない）の正本である。入口は [`roadmap.md`](../roadmap.md) 冒頭および Backlog 節。

**ブランチ運用の目安（本 `v2` を土台にする場合）**

| 領域 | 担当の目安 |
|------|------------|
| GUI（mode 表示・セレクタ、transition plan preview、Flowgraph エディタ／パレット） | 派生ブランチで **利用者 + Codex** が実装し、**`v2` へ PR** で合流する。 |
| `conf` / `state` の mode 周りデータモデル、Mode Manager、loader の activation、Control API `modes/*`、Flowgraph ノード `flowgraph.mode.*` の **Rust 実装・単体テスト** | **`v2` 上で Cursor** が先行してよい。競合しやすいファイルは PR 単位で調整する。 |
| 文書のみ（language spec 草案、λ+、fixture runner 設計など） | 先に **`v2` にマージ**すると参照が安定する。 |

Language roadmap が挙げる **fixture test runner** や **loader 側の signature 抽出**、Runtime roadmap の **RM-1〜RM-3**（データモデル〜activation）は、GUI ビルド成果物と衝突しにくいことが多い。

---

## 1. 目標アーキテクチャ

```text
VAC
 ├─ Core runtime（`state` / `conf` 等の中核）
 ├─ Flowgraph ランタイム
 ├─ bridges 層（`src/bridges/`）
 │   ├─ web_input
 │   ├─ voice
 │   ├─ twitch
 │   ├─ channel_subscribe
 │   └─ vmc_ingress   ← 新規
 ├─ motion 層（`src/motion/`） ← 新規
 │   ├─ vmc_raw（VMC 生パケット）
 │   ├─ osc
 │   ├─ router
 │   └─ MotionFrame（後段）
 ├─ Control API（`src/web_interface/`）
 └─ GUI / トレイ（将来: desktop runner + Tauri shell）
```

### 1.1 実行形態と工程順（desktop / CLI / Tauri）

**工程順（この順で設計・実装する）**

1. **crate 再構造化**（本書 §3）— `vac-core` 等へ責務を分け、コア API を安定させる。
2. **2 実行ファイルの runner** — 詳細設計のあと実装する。いずれも **単体起動可能**で、コアは **同一ライブラリ**を呼ぶ薄い `main`（配布形態として「玄人用」と「一般用」を分ける）。
3. **Tauri（Phase ε-2）** — **desktop runner に組み込む**。CLI は API / ログ / 本体機能開発を優先する従来型 runner として残す。

**CLI 版（仮称 `virtual-avatar-connect-cli` 等）**

- **コンソールが付く**。ログやデバッグをそのまま見られることが価値。
- 対象: 開発者、スクリプト・CI、詳細ログが欲しい玄人。**一般ユーザーに「ターミナルを最小化してブラウザだけ使う」ことは期待しない**（黒いウィンドウの存在だけで不安になる人もいる）。
- GUI 配信と Control API は内蔵する。GUI 開発は `gui` 側の `npm run dev` と組み合わせ、CLI 自体は Tauri shell を持たない。

**desktop 版（仮称 `virtual-avatar-connect-desktop` 等）**

- **本質はシステムトレイ常駐**だが、パッケージ名・exe 名は **`desktop`** を採る。`systray` 等の実装者語より、「これを実行すれば VAC が動く」という **一般ユーザーへの伝わりやすさ**を優先する。
- Windows では **コンソールを出さない**常駐に寄せる（`windows_subsystem` 等は ε 文書・実装時に確定）。
- **期待 UX（目標）**: トレイにアイコン → **ダブルクリックで Web GUI**（既存 `gui/dist` + Control API）を開く → **右クリックでコンテキストメニュー**（GUI を表示 / 終了、将来は conf 一覧からリロード等）。Phase M3 の転送先 UI 等も、この入口と整合させる。

**Tauri の位置づけ**

- ブラウザで LAN 越しに操作する経路は維持しつつ、**Tauri GUI shell** は desktop プロセスの shell として載せる。desktop では tray 常駐と window show / focus を統合する（[`phase-epsilon-shutdown-and-tauri.md`](phase-epsilon-shutdown-and-tauri.md) §3 の方針と一致）。

### 1.2 GUI 静的成果物の内蔵（`npm run dev` なしで設定 GUI）

**狙い**: 配布バイナリだけで **desktop でも CLI でも** Control Panel（既存 Svelte GUI）を開ける。エンドユーザーが **別途 `npm run dev` を起動したり、`gui/dist` を手で置いたり**しなくてよい状態にする。

**ソースとビルドの分離**

- **編集・型チェック・高速ループ**はこれまでどおり `gui/`（Svelte + Vite）で行う。
- **`npm run build` で得た `gui/dist/`** を、Rust の **ビルド時**に取り込み、**実行ファイルまたは専用クレートに同梱**する。取り込み方は実装フェーズで決める（例: ワークスペースに **`vac-gui-assets`** のような薄い crate を切り、`build.rs` で `dist` を走査して `include_bytes!` 用の生成ソースを吐く／[`rust-embed`](https://crates.io/crates/rust-embed)／Tauri の **bundler + custom protocol**／単一 `include_dir!` 等）。**コア（`vac-core`）と同様に「成果物だけを境界として持つ」**イメージで、Svelte 本体は `gui/` に残し、Rust 側は **ビルド済みアセットの配信責務**だけを持つモジュール／crate に閉じる。

**ランタイムでの使い方（概念）**

- **actix**（既存 `web_interface`）: ディスク上の `gui_dist_path` の代わりに、**メモリ上のバイト列**から `index.html` / chunk を配信するルートを用意すれば、**CLI runner でも** ローカル URL だけで設定 GUI が開ける。
- **Tauri（desktop）**: WebView の入口 URL を **同梱静的**（`asset://` 相当）に切り替えられるなら、オフライン同梱と整合する。HTTP API は引き続きループバックの actix に向ける（[`phase-epsilon-shutdown-and-tauri.md`](phase-epsilon-shutdown-and-tauri.md) §3.5 と両立）。

**開発ビルド**

- 開発者向けには従来どおり **ファイルシステムの `gui/dist` または Vite dev server** を指す feature／環境変数を残してよい（リリースと開発の二経路）。

### 1.3 CI 方針（メモ・2026）

- **`embed-gui` だけを対象にした部分的な GitHub Actions 等は、いま入れない。** リポジトリにワークフローがほぼ無い現状では、単発ジョブの保守コストに対して得が小さい。`Cargo.lock` の扱い・ワークスペース（Step 5）・`vac-gui-assets`（Step 6b）が固まる前に CI を足すと、すぐ作り直すことになる。
- **CI は** 上記と **まとめて設計**する（Rust 既定テスト、`gui` の `npm ci && npm run build`、必要なら `--features embed-gui` を **一連の方針**で）。それまでは **開発に専念**し、リリース同梱は手元または既存の release 手順で十分とする。
- 参照: Step 6a 実装済み（`embed-gui`）。Step 6b の「CI での npm 統合」は **上記タイミングまで保留**とする（[`roadmap.md`](../roadmap.md) v2 メタ節）。

---

## 2. フェーズ設計

---

### Phase M0: motion 層の導入

#### 目的

* VMC 処理を Flowgraph 本体から分離する。
* パススルー経路の性能を最優先で確保する。

#### ディレクトリ

```text
src/motion/
 ├─ vmc_raw.rs
 ├─ osc.rs
 ├─ router.rs
 ├─ config.rs
```

#### 機能

* UDP ソケットの bind
* UDP データグラムの受信
* 複数転送先へのペイロードそのままの転送
* 転送先集合の管理

#### 要件

* **パース不要**で転送できること
* コピー回数・バッファを最小化
* async コンテキストでワーカーを占有しないこと

**M2（ハブ運用・ログ）**: 複数受信ソケット・任意 `label`・同一 `bind` の起動時警告・`send_to` 失敗のログ間引き。正本は [`phase-mu-vmc-motion-m0.md`](phase-mu-vmc-motion-m0.md) §9。

---

### Phase M1: VMC ingress ブリッジ

#### 目的

Flowgraph へ VMC 入力を接続する。

#### ノード

```text
flowgraph.ingress.vmc_udp
```

#### 挙動

* `vmc_ingress` ブリッジが UDP を受信する。
* `` `TriggerEvent` `` に変換し、生バイト列を payload（またはメタ）に載せる。

#### 汎用 OSC UDP ingress（Phase ρ・メタ分離）

* ノード: `flowgraph.ingress.osc_udp`、ブリッジ: `osc_ingress`。
* VMC  ingress（`vmc_udp`）と **同一の Base64 運搬**だが、`__meta__` に `profile: "osc_udp"` を載せ、既定 `__source_kind__` は `osc_udp` とする（VMC 専用メタ・運用と混ぜない）。

---

### Phase M2: パススルー・ハブ化

#### 目的

当面の実用価値として、VAC を **パススルー・ハブ**（1 受信 → 複数送信先）として使えるようにする。

#### データフローの例

```text
Waidayo / iFacialMocap
   ↓
VAC（vmc_ingress）
   ├─ Warudo
   ├─ UNVET
   └─ …
```

#### 特徴

* **デコード不要**（アプリ互換用の生転送）
* 遅延を最小化
* VAC を、配信クライアント等の横に置ける **常駐型データフロー処理** として位置づける第一歩

---

### Phase M3: Control API / GUI

#### API

```http
POST /api/v1/vmc/bind
POST /api/v1/vmc/forward/add
POST /api/v1/vmc/forward/remove
GET  /api/v1/vmc/status
```

#### GUI

* トレイ常駐（**一般ユーザー向けの入口は desktop 実行ファイル**と §1.1 を揃える）
* Web UI の表示
* 転送先（forward）の管理
* 受信パケットレートの表示

---

### Phase M4: `MotionFrame` 抽象（後段）

#### 目的

VMC を **意味のある構造**（中間表現）へ昇格させる。

#### 型（概念）

```text
MotionFrame {
  timestamp
  head_pose
  bone_transforms
  blendshapes
}
```

#### ノード

```text
flowgraph.motion.vmc_parse
flowgraph.motion.filter
flowgraph.motion.map
```

実装メモ（2026-04 時点）: 上記ノードのデータ経路は **`motion_frame`** ソケット（Rust の `MotionFrame` = `src/motion/frame.rs` の `byte_len` + `osc_messages`）と `json` との **coerce 往復**で統一。本節で概念化している `head_pose` / `bone_transforms` / `blendshapes` などの **意味付き IR** は未分解（M5 以降の拡張余地）。

---

### Phase M5: Flowgraph の用途拡張

#### 追加用途

* 表情トリガー
* ジェスチャ検出
* AI 入力
* 配信制御

---

## 3. crate 分離計画

### 現状の課題

* 単一 crate に責務が集中している
* 依存が肥大化している
* ビルド時間の増大
* モジュール境界が不明瞭

---

### 最終構成（目標）

```text
（ワークスペース root の Cargo.toml + メンバ）
  virtual-avatar-connect  … 現行アプリ crate
  crates/vac-core/        … shutdown broker（一段目）
  crates/vac-gui-assets/  … Svelte のビルド済み dist のみ（Step 6b で追加済み）
  crates/vac-motion/      … MotionFrame / OSC decode / UDP forwarding helper / OSC / VMC / VRChat encoding helper（一段目）
  crates/vac-flowgraph/   … Flowgraph pure primitives（一段目は Quantity system / instance config）

（未分割・論理名）
  vac-core
  vac-flowgraph
  vac-bridges
  vac-motion
  vac-control-api
  vac-app          ← 将来: vac-app-cli / vac-app-desktop の 2 bin に分割
```

（`vac-gui-assets` 以外の論理 crate 名と分割順序は §3「移行手順」に従う。）

---

### 各 crate の責務

#### `vac-core`

* `State` / `SharedState`
* `ChannelDatum`
* `Conf` / 設定モデル
* 共通型

#### `vac-flowgraph`

* `Node` trait
* `SocketType` / `SocketValue`
* ランタイム・実行器（executor）

#### `vac-bridges`

* `web_input` / `voice` / `twitch` / `channel_subscribe` / `vmc_ingress` 等、ingress のブリッジ実装

#### `vac-motion`

* VMC 生パケット処理
* OSC パース（後段）
* router
* `` `MotionFrame` ``

#### `vac-gui-assets`（論理名）

* `gui/` の **ビルド成果物**（`gui/dist`）のみを所有。`build.rs` / `rust-embed` / 生成ソース等は実装時に確定（§1.2）。
* `vac-control-api` はここから静的ファイルを解決し、**埋め込み配信**と **ファイル fallback**（開発用）を切り替え可能にする。

#### `vac-control-api`

* `actix-web`
* REST / WebSocket（Control API、GUI 用バックエンド）

#### `vac-app`（論理名・分割タイミングは再構造化で確定）

* 現状相当の `main` / 起動初期化 / 各サービス起動 / ランタイム統合
* 将来: **`vac-app-cli`** と **`vac-app-desktop`** の 2 bin（または同等の `[[bin]]` 2 本）に分け、いずれも `vac-core` を呼ぶ runner に落とす（§1.1）

---

### 移行手順

**進捗メモ**: Step 1〜3 は **Phase M0〜M1 相当として実装済み**（`src/motion/`、`vmc_ingress`、ingress ノード）。Step 4〜5 と **Step 6 以降**は未着手。以降は **§1.1** の工程順（再構造化 → Step 6 の GUI 同梱 → AppCore → runner → Tauri）と整合させる。

#### Step 1（完了）

* `src/motion/` を追加する
* `vmc_raw` を実装する

#### Step 2（完了）

* `src/bridges/` に `vmc_ingress` を追加する

#### Step 3（完了）

* Flowgraph（ingress ノード・`TriggerHandle` 経路）と接続する

#### Step 4（文書・契約の一段）

* **実施（本段）**: [`architecture.md`](../architecture.md)「レイヤ境界（Step 4）」に `motion` / `bridges` / `flowgraph` / `web_interface` / `state` の **許容依存と例外**（当初は `state → web_interface`）を表形式で固定。`motion` / `bridges` の crate 先頭ドキュメントに同趣旨の **依存契約**を追記。
* **実施（追記）**: `state → web_interface` の型依存は解消済み（[`src/twitch_oauth_sessions.rs`](../../src/twitch_oauth_sessions.rs)、[`src/control_events.rs`](../../src/control_events.rs)）。`web_interface::control::events` は `control_events` の再エクスポート。
* **実施（追記）**: `web_interface/control/flowgraph/` を **`mod.rs` + `util` / `reload` / `trigger` / `fragment_zip`** に分割（単一巨大 `flowgraph.rs` を廃止）。
* **実施（追記）**: `web_interface/control/table/` を **`mod.rs` + `util`** に分割（旧 `table.rs` を廃止）。
* **実施（追記）**: `web_interface/control/profiles/` を **`mod.rs` + `util`** に分割（旧 `profiles.rs` を廃止）。
* **実施（追記）**: `web_interface/control/restart/` を **`mod.rs` + `util.rs`** に分割（旧単一 `restart.rs` を廃止。パス解決・ラベル・パス同値判定は `util`）。
* **実施（追記）**: `web_interface/control/run_with/` を **`mod.rs` + `util.rs` + `toml_ops.rs`** に分割（旧単一 `run_with.rs` を廃止。TOML 配列操作と DTO→`Value` は `toml_ops`）。
* **実施（追記）**: `web_interface/control/auth/` を **`mod.rs` + `runtime.rs` + `middleware.rs`** に分割（旧 `auth.rs` を廃止。トークン解決は `runtime`、`control_api_auth` とクエリ解析テストは `middleware`）。
* **実施（追記）**: `web_interface/control/oauth_twitch/` を **`mod.rs` + `start_flow.rs`** に分割（旧 `oauth_twitch.rs` を廃止。DCF 開始〜ポーリング登録は `start_flow`）。
* **実施（追記）**: `web_interface/control/managed_app/` を **`mod.rs` + `lookup.rs`** に分割（旧 `managed_app.rs` を廃止。registry 参照は `lookup`）。
* **実施（追記）**: `web_interface/control/bos/` を **`mod.rs` + `util.rs`** に分割（旧 `bos.rs` を廃止。カテゴリ推定・channel 検出・タイトル生成は `util`）。
* **実施（追記）**: `web_interface/control/reload/` を **`mod.rs` + `ai_persona.rs`** に分割（旧 `reload.rs` を廃止。AI persona 適用は `ai_persona`）。
* **実施（追記）**: `web_interface/control/ws/` を **`mod.rs` + `actor.rs`** に分割（旧 `ws.rs` を廃止。`ControlEventsWs` は `actor`）。
* **実施（追記）**: `web_interface/control/actions/` を **`mod.rs` + `pause.rs`** に分割（旧 `actions.rs` を廃止）。
* **実施（追記）**: `web_interface/control/dto/` を **`mod.rs` + `twitch_summary.rs`** に分割（旧 `dto.rs` を廃止。モジュール先頭 rustdoc を日本語で整理）。
* **実施（追記）**: `web_interface/control/ping/` を **`mod.rs` + `responses.rs`** に分割（旧 `ping.rs` を廃止）。
* **実施（追記）**: `web_interface/control/shutdown/` を **`mod.rs` + `types.rs`** に分割（旧 `shutdown.rs` を廃止）。
* **実施（追記）**: `web_interface/control/ingress/` を **`mod.rs` + `post.rs`** に分割（旧 `ingress.rs` を廃止）。
* **残り（後続 PR）**: `web_interface/control/events.rs` や `control/mod.rs` の肥大化が顕在化した場合の分割、他モジュールの同様の表化など。

#### Step 5（一部完了）

* Cargo **ワークスペース**化する（上記 crate 図へ向けた土台）。**現状**: root `Cargo.toml` に `[workspace]`（`members = [".", "crates/vac-gui-assets", "crates/vac-motion", "crates/vac-flowgraph", "crates/vac-core"]`、`default-members = ["."]`）を追加済み。`cargo build` / `cargo test` 既定は **ルート crate のみ**（`vac-gui-assets` は `embed-gui` または `-p vac-gui-assets` でビルド）。
* **残り**: 他クレートの `members` 追加。`vac-core` は `ShutdownBroker` / runtime path primitives / DateTime boundary、`vac-motion` は第一段として `MotionFrame` / OSC decode / UDP forwarding helper / OSC / VMC / VRChat encoding helper、`vac-flowgraph` は第一段として Quantity system / instance config を分割済み。

#### Step 6 — `vac-gui-assets` と埋め込み配信

* **6a（実装済み）**: Cargo feature **`embed-gui`**。`web_interface::gui_embedded` で `/gui/*` をメモリ配信。既定ビルドでは `gui_disk`（ファイル）経路。
* **6b（crate 切り出し済み・CI は未）**: ワークスペースメンバ **`vac-gui-assets`**（[`crates/vac-gui-assets/`](../../crates/vac-gui-assets/)）が `include_dir` と `build.rs` で `gui/dist` を取り込み、メイン crate は `embed-gui` 時のみ依存。**GitHub Actions 等は §1.3 のとおり Step 5 確定後にまとめて設計**する。
* **開発時**: `embed-gui` 無効時は既存の `gui_dist_path` や Vite をそのまま利用（§1.2）。

#### Step 7 — `AppCore` 抽出（Phase ε-2a）

* [`phase-epsilon-shutdown-and-tauri.md`](phase-epsilon-shutdown-and-tauri.md) §3.2 のとおり `boot` / `serve` / `cleanup` に分離し、runner と Tauri の両方から同じコアを起動できるようにする。

#### Step 8 — CLI / desktop の 2 runner

* **`virtual-avatar-connect-cli`** / **`virtual-avatar-connect-desktop`**（仮称）の `[[bin]]` 2 本。いずれも `vac-core` + `AppCore` 経路を共有（§1.1）。

#### Step 9 — desktop Tauri GUI shell + 同梱静的 + tray

* [x] WebView の入口は loopback `/gui/`。`embed-gui` release build では actix が **同梱 GUI**（§1.2、[`phase-epsilon-shutdown-and-tauri.md`](phase-epsilon-shutdown-and-tauri.md) §3.6）を返す。Control API はループバック HTTP のまま。
* [x] Tauri GUI shell は desktop runner に組み込む。desktop では tray の `GUI を開く` / 左ダブルクリックから window show / focus する。
* [x] tray / window / Control API の停止導線は `ShutdownBroker` に合流させる。

---

## 4. 設計原則

### パフォーマンス

* **生バイト経路**を最優先する
* **パース**は後段（M4 以降）に回す
* **ヒープ割り当て**を抑える

### データフロー

* イベント駆動
* ingress → Flowgraph は `` `TriggerEvent` `` に統一する

### 拡張性

* ingress は追加可能にする
* ノードは **レジストリ**方式（現行の `registry` パターン）に合わせる

### 互換性

* VMC 送信側・受信側の期待するペイロードを壊さない
* 既存アプリのポートや送信パスを不当に奪わない

---

## 5. 最重要判断

### やる

* VMC / UDP の **並列パススルー**
* Flowgraph との統合
* GUI での可視化・操作
* crate / モジュール境界の整理

### やらない

* 重いシリアライズを hot path に載せること
* 先回りの過剰抽象化

---

## 6. VAC の再定義

```text
VAC =
常駐型のデータフローエンジン
+
モーショントラッキング・ハブ
+
アバター / 配信 / AI 統合環境
```

---

## 7. 結論

* VAC 単体で **モーショントラッキング・ハブ**（tracking hub）の役割を担えるようにする。
* Flowgraph は既に **実質的な実行記述** として運用されている。**本計画では VMC 線でも同一モデル**（`` `TriggerEvent` `` 等）に載せ、その位置づけを揃える。
* 最短ルートは、まず **VMC パススルー・ハブ** を完成させること。

---
