# VAC crate / runner / desktop 再編計画

> Status: design fixed for v2 direct implementation. GUI visual baseline が `v2` に入ったため、次は crate 境界、CLI / desktop runner、system tray / Tauri の順で配布形態を整える。

---

## 1. 結論

この作業は **GUI 担当側（Codex）を主担当**にする。理由は、crate 再編そのものよりも、その後の **desktop runner / tray / Tauri / GUI 同梱**が主目的だからだ。非 GUI runtime の中身を担当する開発者がここを主導すると、Rust crate としては綺麗でも、常駐 GUI アプリとしての起動・終了・配布 UX が後付けになりやすい。

ただし Flowgraph / VMC / Runtime Mode の中身は並行担当に任せる。こちらは起動形態と境界を整え、既存 runtime の意味論は変えない。

---

## 2. 担当分担

| 領域 | 主担当 | 備考 |
| --- | --- | --- |
| crate 境界設計、workspace 整理、runner 境界 | Codex | GUI / desktop 配布形態を前提に切る。 |
| `AppCore` の `boot` / `serve` / `cleanup` 分離 | Codex | Tauri `.setup()` と CLI `main` の両方から呼べる形にする。 |
| CLI / desktop 2 runner | Codex | CLI は console、desktop は tray / windowed 前提。 |
| Tauri / system tray / GUI 起動 UX | Codex | `ShutdownBroker` と Control API を既存経路のまま使う。 |
| Flowgraph engine / node / fixture runner | v2 メイン担当 | crate 移動中も意味論変更は避ける。 |
| VMC / OSC / VRChat / motion | v2 メイン担当 | `motion` / `flowgraph` の境界契約内で進める。 |
| Runtime Mode backend | v2 メイン担当 | GUI は Control API 経由で追従する。 |

---

## 3. 目標構成

最終形は次の workspace を目指す。ただし一度に全部は移さない。

```text
virtual-avatar-connect/
  Cargo.toml
  crates/
    vac-core/          # conf, runtime paths, shutdown, shared state primitives
    vac-flowgraph/     # Flowgraph language/runtime/nodes
    vac-motion/        # VMC/OSC UDP motion transport and frame helpers
    vac-bridges/       # ingress/egress bridge tasks
    vac-control-api/   # actix Control API + GUI static serving
    vac-app/           # AppCore boot/serve/cleanup orchestration
    vac-cli/           # console runner
    vac-desktop/       # desktop/tray/Tauri runner
  vac-gui-assets/      # 既存。将来 crates/ 配下へ移動してよい
  gui/                 # Svelte source
```

当面は root crate を残したまま、内部 module の公開境界を整え、移動しやすい単位を作る。`crates/` への物理移動は、依存方向が確認できた順に小さく行う。

---

## 4. 実装順

### R0: 設計固定

- 本書を追加する。
- [`architecture.md`](../architecture.md) と [`roadmap.md`](../roadmap.md) から本書へリンクする。
- `v2-vmc-and-restructure.md` の Step 7〜9 と矛盾しないようにする。

### R1: `AppCore` API を runner 前提に分ける

現状の `src/app_core.rs` は `run_vac_application(conf, audio_sink)` の中で boot、serve、cleanup を直列に実行している。これを次の形へ寄せる。

```rust
pub struct AppCore {
    /* conf / state / shutdown / handles */
}

impl AppCore {
    pub async fn boot(conf: Conf, audio_sink: SharedAudioSink) -> Result<Self>;
    pub async fn serve(&self) -> Result<()>;
    pub async fn cleanup(self) -> Result<()>;
}
```

`crate::run()` は `Args`、特殊モード、`Conf::new`、`conf.execute_run_with()` までを担当し、その後は `AppCore` に委譲する。ここでは挙動を変えない。

### R2: runner 用 entry API を追加

CLI と desktop の両方が同じ起動準備を使えるように、root crate に薄い entry API を作る。

```rust
pub async fn run_cli() -> Result<()>;
pub async fn run_desktop_headless() -> Result<()>;
```

`run_desktop_headless` は Tauri 導入前の橋渡しで、console policy 以外は CLI と同じ動きをする。これで desktop runner の binary 名、ログ、終了コードの扱いを先に固定できる。

### R3: CLI / desktop 2 binary

`Cargo.toml` に runner を追加する。

```text
virtual-avatar-connect          # 互換 alias。当面は CLI と同じ。
virtual-avatar-connect-cli      # console runner
virtual-avatar-connect-desktop  # desktop runner
```

Windows release の desktop 側だけ `windows_subsystem = "windows"` を使う。開発中は console を残す。`virtual-avatar-connect` は移行期間の互換名として残し、どの時点で alias を変えるかは release note で決める。

### R4: crate 物理分割の第一段

最初に切るのは依存が軽いものに限定する。

1. `vac-motion`
2. `vac-gui-assets` の `crates/` 配下移動（必要なら）
3. `vac-flowgraph` の pure 部分

`state`、`web_interface`、`bridges` は相互依存が濃いので後回し。先に動かすと実装速度が落ちる。

### R5: desktop tray 最小実装

Tauri の前に、desktop runner の責務を固定する。

- 起動時に VAC runtime を立ち上げる。
- GUI を既定ブラウザで開ける。
- 終了時は `ShutdownBroker` を使う。
- 将来 tray から呼ぶ操作を Rust API として用意する。

tray menu の初期項目は次だけでよい。

| 項目 | 動作 |
| --- | --- |
| GUI を開く | ループバックの `/gui/` を開く。 |
| 終了 | `ShutdownBroker` 経由で graceful shutdown。 |

Restart / profile switch は GUI 側の既存 Control API があるため、tray 初期実装に入れない。

### R6: Tauri shell

Tauri は desktop runner にだけ載せる。

- WebView は既存 Svelte GUI を表示する。
- GUI は引き続き HTTP/WS Control API を叩く。
- `invoke` は原則使わない。bootstrap 情報が必要な場合だけ薄く追加する。
- tray close / window close / GUI 終了ボタンは `ShutdownBroker` へ合流させる。

---

## 5. やらないこと

- 最初から全 crate を `crates/` へ大移動しない。
- Tauri IPC で Control API を置き換えない。
- GUI と runtime の通信を HTTP/WS 以外へ急に変えない。
- Flowgraph / VMC / Runtime Mode の意味論変更を crate 再編 PR に混ぜない。
- CI matrix は crate 構造が落ち着くまで増やさない。

---

## 6. 並行作業の候補

他担当者に任せるなら、次が衝突しにくい。

1. **Flowgraph fixture runner の拡充**
   - crate 移動後も使える純粋な regression asset になる。
   - GUI / desktop runner とファイル競合しにくい。

2. **VMC / OSC / VRChat ノードの追加テスト**
   - `src/flowgraph/**` と `src/motion/**` 中心。
   - こちらが触る `app_core` / runner / docs と衝突しにくい。

3. **Runtime Mode backend の細部**
   - Mode transition、Managed App desired state、activation の単体テスト。
   - GUI は既存 Control API で追従できる。

4. **node catalog / manual 更新**
   - 実装済みノードの説明整備。
   - crate 再編中でも独立して進めやすい。

避けた方がよい並行作業:

- `src/app_core.rs`
- `src/lib.rs`
- `src/main.rs`
- root `Cargo.toml` の bin / workspace 周辺
- `src/web_interface/gui*`

ここは私が触る可能性が高く、衝突しやすい。

---

## 7. 検証方針

各段階で最低限次を確認する。

```powershell
cargo check --lib
cargo check --bins
cargo test --lib
cd gui
npm.cmd run check
npm.cmd run build
```

runner / desktop 段階では追加で手動確認する。

- `cargo run --bin virtual-avatar-connect-cli -- conf.fixture.e2e.toml`
- `cargo run --bin virtual-avatar-connect-desktop -- conf.fixture.e2e.toml`
- `/api/v1/control/ping`
- `/gui/`
- GUI の終了ボタン

Tauri 導入後は、Windows の console 非表示、tray close、double click / context menu を別途見る。
