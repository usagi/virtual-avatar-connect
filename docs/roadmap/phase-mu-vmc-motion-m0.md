# Phase M0 — motion 層: VMC 生 UDP パススルー

親計画: [`v2-vmc-and-restructure.md`](v2-vmc-and-restructure.md)  
ロードマップ進捗: [`../roadmap.md`](../roadmap.md) の **Phase M** 節。

## 1. スコープ（M0）

- `src/motion/` を新設し、**パースなし**の UDP 受信 → 同一ペイロードの複数宛 `send_to` を実装する。
- **M0 時点**では `conf.toml` の `[motion]` のみ。Flowgraph ノード・bridges の `` `TriggerEvent` `` は **M1**（§8）。
- Control API / GUI には触れない（M3）。

## 2. 設定スキーマ（`conf.toml`）

トップレベルに任意の `[motion]` を置く。省略時は motion ワーカーを起動しない。

| TOML | 型 | 説明 |
|------|-----|------|
| `[[motion.vmc_passthrough]]` | テーブル配列 | 受信ソケット 1 本ごとのエントリ |
| `enabled` | bool | `false` で当該エントリを無効化（既定 `true`） |
| `bind` | string | 受信 UDP の `"host:port"`（例 `0.0.0.0:39539`） |
| `forward_to` | string 配列 | 転送先ごとの `"host:port"`。**空のときは当該エントリをスキップ**（警告ログ） |

複数の `[[motion.vmc_passthrough]]` を並べれば、受信ポートを複数立てられる。

## 3. ランタイム挙動

- 起動: `lib.rs::run` が `State` 生成・Flowgraph bridges 初期化の **直後**に [`MotionHandles::spawn_all`](../../src/motion/mod.rs) を呼ぶ。
- 各エントリは **独立した** `tokio::spawn` タスク。`tokio::net::UdpSocket::bind` → `recv_from` ループ。
- 停止: `tokio::select!` で [`ShutdownBroker::wait`](../../src/shutdown.rs) と `recv_from` を待ち合わせる。停止要求後はループを抜けてソケットを drop。
- 終了 cleanup: `run()` の shutdown フェーズで [`MotionHandles::finish_all`](../../src/motion/mod.rs)（残タスクの `abort` + `await`）。

## 4. 非目標（M0 ではやらない）

- OSC パース、`MotionFrame`、Flowgraph ノード（M4 / ρ）。
- 転送先の hot-reload（将来: conf reload または Control API）。

## 5. 実装マップ

| ファイル | 役割 |
|----------|------|
| [`src/conf/motion.rs`](../../src/conf/motion.rs) | `MotionConf` / `VmcPassthroughSpec`（serde） |
| [`src/motion/mod.rs`](../../src/motion/mod.rs) | `MotionHandles`、spawn / finish |
| [`src/motion/vmc_raw.rs`](../../src/motion/vmc_raw.rs) | bind / recv / shutdown select |
| [`src/motion/router.rs`](../../src/motion/router.rs) | `forward_datagram`（マルチ `send_to`） |
| [`src/motion/osc.rs`](../../src/motion/osc.rs) | M4 までプレースホルダ |

## 6. 検証手順（手元）

1. 受信専用のダミー UDP を別プロセスで立てるか、実アプリ（例: Warudo）の受信ポートを `forward_to` に列挙する。
2. `conf.example-motion.toml` を参考に `[motion]` を有効化して VAC を起動する。
3. 送信元（例: iFacialMocap）から VMC 送信 → 各 `forward_to` で受信できること。

## 7. Commit メッセージ例

`M-0 feat(motion): VMC UDP passthrough + [motion] conf`

---

## 8. Phase M1 — `flowgraph.ingress.vmc_udp` + `vmc_ingress` ブリッジ

### 8.1 スコープ

- ノード **`flowgraph.ingress.vmc_udp`**（他 ingress と同型の echo + `__trigger__` パターン）。
- [`src/bridges/vmc_ingress.rs`](../../src/bridges/vmc_ingress.rs): `bind` で UDP を受信し、各データグラムを `` `TriggerEvent` `` で該当ノードに投入。

### 8.2 プロパティ（ノード `properties`）

| key | 説明 |
|-----|------|
| `bind` | `"host:port"`。空ならブリッジを起動しない（警告ログ）。 |
| `fixed_channel` | 任意。空なら `source_kind` に `vmc_udp`、非空ならその文字列を使用。 |

### 8.3 データマッピング

| 内部ポート | 値 |
|------------|-----|
| `__content__` | ペイロードの **Base64**（`String` ポート経由で echo） |
| `__source_actor__` | 送信元 `ip:port` |
| `__source_kind__` | `fixed_channel` または `vmc_udp` |
| `__meta__` | JSON: `remote`, `byte_len`, `encoding: "base64"` |

### 8.4 ライフサイクル

- [`bridges::spawn_all_from_state`](../../src/bridges/mod.rs) が `State.shutdown` を共有し、UDP ループは `ShutdownBroker::wait` で終了。
- [`BridgeHandles::finish_all`](../../src/bridges/mod.rs) で `vmc_udp` タスクを `abort`。

### 8.5 例

[`flowgraph.example/vmc-udp-ingress/main.flowgraph.toml`](../../flowgraph.example/vmc-udp-ingress/main.flowgraph.toml)

### 8.6 Commit メッセージ例

`M-1 feat(flowgraph,bridges): flowgraph.ingress.vmc_udp + vmc_ingress bridge`
