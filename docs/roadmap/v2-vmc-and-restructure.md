# VAC v2 再構造化 + VMC 対応 計画書（改訂版）

## 0. 方針

* VAC を **常駐型のデータフローエンジン** とし、まず **VMC（UDP）のモーショントラッキング** を扱えるようにする。その最初の機能として **並列パススルー**（マルチキャスト的転送）を可能にする。
* Flowgraph を **実行記述（プログラム）としての位置づけ** を文書・実装の両面で明確にする。
* **crate 分離**は段階的に進め、責務と保守性を整理する。

**記法**: 本書の **`Phase M0`〜`M5`** は VMC / Motion 作業用のラベルであり、[`roadmap.md`](../roadmap.md) の **δ / φ …** などギリシャ文字フェーズとは別系統（同ファイル **Phase M** 節で進捗管理）。Rust 型・モジュール名は `` `Snake_case` `` / `` `PascalCase` ``、HTTP は `ALL CAPS` パス表記。

**M0 詳細設計（実装・`conf.toml` スキーマの正本）**: [`phase-mu-vmc-motion-m0.md`](phase-mu-vmc-motion-m0.md)

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
 └─ GUI / トレイ
```

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

* トレイ常駐
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
crates/
  vac-core
  vac-flowgraph
  vac-bridges
  vac-motion
  vac-control-api
  vac-app
```

（ワークスペース化後の **論理 crate 名**。実リネーム順序は §3「移行手順」に従う。）

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

#### `vac-control-api`

* `actix-web`
* REST / WebSocket（Control API、GUI 用バックエンド）

#### `vac-app`

* `main`
* 起動時の初期化
* 各サービス起動
* ランタイム統合

---

### 移行手順

#### Step 1（今やる）

* `src/motion/` を追加する
* `vmc_raw` を実装する

#### Step 2

* `src/bridges/` に `vmc_ingress` を追加する

#### Step 3

* Flowgraph（ingress ノード・`TriggerHandle` 経路）と接続する

#### Step 4

* モジュール境界を整理する

#### Step 5

* Cargo **ワークスペース**化する

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
