# GUI E2E Testing with Playwright

Virtual Avatar Connect の GUI には Playwright ベースの E2E テストが同梱されています。
フィクスチャに外部 IO を含めない方針で、ローカル実機から OpenAI / Twitch / TTS 等を一切
叩かずに UI と Control API の往復を確認できます。

> Phase ν（`docs/roadmap/phase-nu-gui-e2e-playwright.md`）で導入。
> 現状 3 specs（`control-panel-smoke` / `channels-ws-live-update` / `live-quick-add-learn-undo`）
> が landed。残り 2 spec（Flowgraph Canvas DnD / Dictionary Editor 409 merge）は Phase ν-β。

## 前提

- Node.js（`gui/package.json` の engines に合わせる）。
- Rust toolchain（`cargo build --release` が通ること）。
- OS: Windows / Linux / macOS いずれでも動作する設計。Windows では `target/release/virtual-avatar-connect.exe` のファイルロックを避けるため、GUI 開発と E2E 実行を同時に別プロセスで走らせない方が無難。

## 初回セットアップ

```bash
cd gui
npm install
npm run test:e2e:install   # chromium のバイナリを gui/node_modules 下に取得
```

GUI の静的ビルドも一度は作っておく必要があります（`webServer` は `gui_dist_path="gui/dist"` を配信する VAC 本体を起動するだけなので、`vite dev` は使いません）。

```bash
cd gui
npm run build
```

## 実行

```bash
cd gui
npm run test:e2e          # headless（ci 向け）
npm run test:e2e:ui       # Playwright UI モード（spec 単位で step-through）
```

初回は `cargo run --quiet --release --bin virtual-avatar-connect -- conf.fixture.e2e.toml`
が裏で走るため数分かかります。2 回目以降は `reuseExistingServer: !CI` が効いて、既に起動中
の fixture VAC があればそれに乗るため秒単位で済みます。

## フィクスチャ構成

- `conf.fixture.e2e.toml`（workspace root）: ポート 57098 / 固定 Bearer token `e2e-fixture-token` / 外部 IO 全 OFF / `flowgraph_dir = gui/tests/e2e/fixtures/flowgraph`。
- `gui/tests/e2e/fixtures/flowgraph/sample.flowgraph.toml`: `web_input → log` ＋ `dictionary.learn` / `dictionary.forget` ＋ `table.from_json` のみの最小セット。
- `gui/tests/e2e/fixtures/dictionary/sample.dict.tsv`: 2 行 seed の 11 列辞書。
- `gui/tests/e2e/fixtures.ts`: `TOKEN` / `authHeader()` / `tokenQuery()` の共有ユーティリティ。

## セレクタ規約

- まず `getByRole` / `getByLabel` / `getByText` / `getByPlaceholder` のような accessibility-first なセレクタを試す。
- `data-testid` は **最後の手段**。追加する場合は `vac-<area>-<role>` 命名（例: `vac-flowgraph-node-<id>`）。
- Svelte Flow の生成 DOM や動的 id に頼るセレクタは禁止（spec が flaky になる）。

## 失敗時の切り分け

1. **`webServer` が立ち上がらない**: `target/release/virtual-avatar-connect.exe` がロックされていないか、57098 ポートが他プロセスに掴まれていないか。Windows では開発サーバや旧 fixture プロセスが残っていることがよくある。
2. **認証 401**: `conf.fixture.e2e.toml` の `bearer_token` と `gui/tests/e2e/fixtures.ts` の `TOKEN` が同じ文字列か確認。
3. **特定 spec だけ timeout**: `npm run test:e2e -- <spec-name>` で単発実行し、`--ui` モードに切り替えてステップ実行するのが最短。`test-results/<spec>/video.webm` と `error-context.md` も自動生成される。
4. **Flowgraph trigger API で 503**: fixture flowgraph の先頭ノード（`dictionary.learn` / `dictionary.forget`）に必要な Pure 入力が欠けていないか（現状 `dict_src: table.from_json` 経由で補充している）。

## CI について

現状 CI 化は optional。GitHub Actions 等に載せる場合は

- `npm install` → `npx playwright install --with-deps chromium` → `cargo build --release --bin virtual-avatar-connect` → `npm run build` (gui) → `npm run test:e2e`

の順序で、artifacts として `gui/test-results/` と `gui/playwright-report/` を保存するのが定番です。
