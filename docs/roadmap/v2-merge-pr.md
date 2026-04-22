# v2 → main マージ PR 本文ドラフト

`v2` ブランチを `main` にマージするときの PR テンプレート。実際の PR では本文を
そのままコピーして使える粒度にまとめてある。

---

## Summary

Flowgraph-only アーキテクチャへの最終移行（v0.10.0）。v1 の `[[processors]]` 層は完全に
撤去され、ingress〜変換〜出口のすべてが `flowgraph_dir` 配下の `.flowgraph.toml` と
Flowgraph Runtime で表現される。直近の v2 マージ準備ロードマップ（ζ-3 → γ-4a.0 → γ-4a →
γ-2 → δ-X → ζ-4）で整地を済ませた。

主な内容:

- **ζ-3: Flowgraph reload 時のブリッジ再配線**
  `BridgeHandles` を `SharedState` に常時保持し、`reload_runtime` から旧 bridges と旧
  `FlowgraphRuntime` worker を graceful shutdown してから一括 respawn する経路を統一。
  `web_input` endpoint 差分時は `ControlEvent::RestartRecommended` を push して GUI に
  リスタート推奨 toast を出す。
- **γ-4a.0: GUI ノード/エッジ削除 UX**
  ノードカード hover 時の × ボタン、`ondelete` で一括削除、toast から「元に戻す」。
- **γ-4a: Pipeline エディタ基盤**
  `isDirty` 追跡、`Save *` 表示、Ctrl+S 保存、未保存 `beforeunload` ガード。
- **γ-2: Live タブ仕上げ**
  `POST /managed_apps/:id/restart`、`ManagedAppDrawer` の再起動ボタン、`BosPreview` の
  縦画面時アコーディオン。
- **δ-X: `flowgraph.command.set` ノード**
  V1 scene switcher の Flowgraph ネイティブ版。`flowgraph.example/command-sets/` サンプル。
- **ζ-4: AI-Twitch ハイブリッド方針**
  `conf.example-openai-chat.toml` に `vac_twitch_chat_say` 利用基準を行動規範として記述。
- **merge prep**: `Cargo.toml` を 0.10.0 に bump、`CHANGELOG.md` 更新、
  `docs/manual/v1-to-v2-migration.md` 追加、README に移行ガイド導線を追加。

## Breaking changes

- `[[processors]]` は TOML には残ってもランタイムでは参照されない（起動時警告）。
- `channel_to` は非推奨。Flowgraph の `channel.emit` へ移行。
- `PauseTarget::Processors` / V1 Control API（`/processors/:i/config` 等）はすでに v0.9.x
  で削除済み。再追加はしない。

移行ガイド: [`docs/manual/v1-to-v2-migration.md`](../manual/v1-to-v2-migration.md)

## Test plan

- [x] `cargo test --lib`（全 358 passed）
- [x] `cargo check --lib`（warn 2 のみ、v1 残滓の未使用関数で想定内）
- [x] `cd gui && npm run check`（svelte-check 0 errors / 0 warnings）
- [x] `cd gui && npm run build`（vite build 成功）
- [x] `flowgraph.example/` が `load_flowgraph_dir` でエラー無くロードされる
- [x] `BLESS_NODE_CATALOG=1` で再生成した `docs/manual/node-catalog.md` が最新
- [ ] `cargo build --release`（レビュー担当側で確認）
- [ ] `cargo run -- conf.example-*.toml` で各 example の deserialize が通る（レビュー担当）
- [ ] `conf.local.solo.toml` 等の実運用 conf で 1 配信分の回帰確認（メンテナ）

## 既知の回帰

- `windows` crate 0.61 → 0.62 の API 失効（OCR / screenshot 系）は v0.9.x 時点で
  `anyhow::bail!` に暫定 fallback 済み。δ-9.3 で `windows-future` 経由に再実装予定。

## 関連

- ロードマップ原本: `.cursor/plans/v2_merge-ready_roadmap_*.plan.md`
- CHANGELOG: [`CHANGELOG.md`](../../CHANGELOG.md)
- ノード一覧: [`docs/manual/node-catalog.md`](../manual/node-catalog.md)
