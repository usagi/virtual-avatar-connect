# Phase ν — GUI E2E testing with Playwright

> **Status**: Active（ψ-α 完了後に本格着手予定）。本書は ν-0 で本文を書き下ろす前の **スタブ**。
> 起点となるスコープ感は [`../roadmap.md`](../roadmap.md) の "Phase ν" を参照。

---

## 0. Status

- 現状: `gui/` に自動テストは 0 件（`svelte-check` の型検査のみ）
- ψ-α と独立だが、ψ-α 完了後に続けて着手する（ランタイム変更が連続する方がメモリコストが低い）
- 本書は ν-0 で本文を書き下ろす

---

## 1. 動機（E2E でしか拾えない挙動）

- Phase φ の Dictionary Editor Pane（optimistic lock / 409 → 3-way merge ダイアログ / Live Quick-Add の Learn + Undo）
- Flowgraph Canvas（γ-4a / γ-4a.0 のノード追加・接続・削除・Save-dirty 表示・Ctrl+S・beforeunload ガード）
- Live Tab / Managed App Drawer（γ-2 の restart + status polling）
- OBS Browser Source（`/browser-output/*`）の WebKit 挙動検証

---

## 2. スコープ（ν-0 で確定予定）

初期 5 ケースの想定:

1. `control-panel-smoke` — GUI 起動 + 認証トークン疎通 + channels 一覧表示
2. `flowgraph-canvas-basic` — ノード追加→接続→Save-dirty→Ctrl+S→dirty 解消
3. `dictionary-editor-409-merge` — 同時編集→409→3-way merge ダイアログ解決
4. `live-quick-add-learn-undo` — Quick-Add で Learn → Undo → 元の状態に戻る
5. `channels-ws-live-update` — WebSocket で channel ingress がリアルタイムに流れる

要件:

- `gui/playwright.config.ts` + `gui/tests/e2e/` を配置
- `webServer` で VAC を `conf.fixture.e2e.toml`（Twitch / OpenAI / TTS 全 disabled、外部依存ゼロ）で起動
- Linux / Windows どちらでも手元で回せる
- CI 化は optional（ν-2 以降）

---

## 3. Sub-phase Breakdown（暫定）

| sub | 内容 |
|---|---|
| ν-0 | docs: 本書の本文を書き下ろす。fixture conf 設計 / 5 ケース仕様 / run 手順の確定 |
| ν-1 | chore(gui): Playwright 依存追加 + `playwright.config.ts` + `webServer` 起動基盤 + `conf.fixture.e2e.toml` |
| ν-2 | test(gui): 初期 5 ケース実装 |
| ν-3 | docs: CHANGELOG / manual 追記 + roadmap tick |

---

## 4. References

- [`../roadmap.md`](../roadmap.md) の "Phase ν" セクション
- [Playwright 公式](https://playwright.dev/)
