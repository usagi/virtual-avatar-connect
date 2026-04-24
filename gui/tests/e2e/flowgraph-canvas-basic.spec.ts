import { expect, test } from '@playwright/test';
import { authHeader, tokenQuery } from './fixtures';

/**
 * §3.2 flowgraph-canvas-basic (ν-β-2)
 *
 * Flowgraph タブのエディタで、γ-4a の dirty tracking + Ctrl+S が壊れていないことを
 * E2E で検証する。phase doc の初期案は「Palette からノードを 1 つ DnD し既存ノード
 * と接続する」だったが、Svelte Flow (`@xyflow/svelte`) のノード追加はクリック方式、
 * エッジ作成は handle 間の pointer drag で合成される（HTML5 DnD ではない）。
 * Playwright から後者を再現すると `pointerdown` の座標計算と `getViewport()` の
 * 初期アニメーションが絡んで強烈に flake る。ν-β では **click-add と save 経路だけ**
 * 覆い、edge drag / beforeunload ガードはβ+（将来フェーズ）へ送る。
 *
 * シナリオ:
 *   1. 現在の sample.flowgraph.toml 内容を API で snapshot（finally で戻す）
 *   2. /gui/?token=... を開き、Flowgraph タブへ移動し、`sample` を選択
 *   3. Save ボタン label が "Save"（= dirty でない）こと
 *   4. Palette 検索に `util.log` を入れて Log ノード行にクリック → 新ノード追加
 *   5. Save ボタン label が "Save *"（= dirty）に遷移
 *   6. Ctrl+S → `PUT /api/v1/control/flowgraph/file/sample` が 200 で飛ぶ
 *   7. Save ボタン label が "Save" に戻る（サーバー側 raw_toml が draft と一致）
 *   8. キャンバスに新ノード (id = `log_2`) が現れている
 *   9. finally: 元の TOML で PUT し直し、repo の fixture を汚さない
 */

const FQ = 'sample';
const FILE_PATH = `/api/v1/control/flowgraph/file/${FQ}`;

type FlowgraphFileResp = {
	raw_toml: string;
	parsed: unknown;
};

test.describe('§3.2 flowgraph-canvas-basic', () => {
	test('click-add node marks dirty; Ctrl+S clears dirty via PUT', async ({
		page,
		request,
	}) => {
		// --- 0. preflight: 元の TOML を snapshot しておく ------------------------
		// finally で戻す。前回 run が中断していても、毎回 API の現在値を基準にする
		// ことで fixture ドリフトを自己治癒させる（ν-β-1 の cleanup 思想と同じ）。
		const snapRes = await request.get(FILE_PATH, { headers: authHeader() });
		expect(snapRes.status(), await snapRes.text()).toBe(200);
		const originalToml = ((await snapRes.json()) as FlowgraphFileResp).raw_toml;
		expect(originalToml.length, 'snapshot raw_toml must not be empty').toBeGreaterThan(0);

		try {
			// --- 1. GUI を開いて Flowgraph タブに切り替え -------------------------
			await page.goto(`/gui/${tokenQuery()}`);

			const tabs = page.getByRole('navigation', { name: 'Main tabs' });
			await tabs.getByRole('button', { name: /flowgraph/i }).click();

			// --- 2. ファイルツリーから `sample` を選択 ----------------------------
			// FlowgraphTree.svelte はファイル行を
			//   <button title={fullPath}><span>{name}</span><span>{node_count}</span></button>
			// で描画するので、accessible name は "sample 4" 等（ノード数を末尾に含む）。
			// fixture 側のノード数変動に耐えるよう正規表現で頭合わせする。
			const sampleFile = page.getByRole('button', { name: /^sample\b/ });
			await expect(sampleFile).toBeVisible({ timeout: 15_000 });
			await sampleFile.click();

			// --- 3. キャンバス初期化を待つ（既存ノード "in" / "log" 描画） --------
			// FlowgraphNodeCard が `#<id>` 表記で id を出すのでそれを見張る。
			const canvas = page.locator('.svelte-flow');
			await expect(canvas).toBeVisible({ timeout: 15_000 });
			await expect(canvas.getByText('#in', { exact: true })).toBeVisible();
			await expect(canvas.getByText('#log', { exact: true })).toBeVisible();

			// --- 4. Save ボタンの初期ラベルが "Save"（clean）-----------------------
			// トップバーの Save ボタンは Ctrl+S タイトルを持つ唯一のボタン。
			const saveBtn = page.getByRole('button', { name: /^Save( \*)?$/ });
			await expect(saveBtn).toHaveText('Save');

			// --- 5. Palette 検索で Log ノードに絞り、クリックして追加 -------------
			// 検索文字列 `util.log` は feature 名にのみマッチし、category `util` や
			// 他の `flowgraph.util.*` とは区別できる（FlowgraphPalette.svelte の
			// filter は feature/title/category/description の includes 判定）。
			const paletteSearch = page.getByPlaceholder(/検索（feature \/ title）/);
			await expect(paletteSearch).toBeVisible();
			await paletteSearch.fill('util.log');

			// 絞り込み後は 1 カテゴリ 1 ノードの表示になる。ボタン title は
			// `spec.feature + "\n" + description` なので title^=feature で一意化。
			const logAddBtn = page.locator(
				'button[title^="flowgraph.util.log"]',
			);
			await expect(logAddBtn).toHaveCount(1);
			await logAddBtn.click();

			// --- 6. dirty 遷移を確認 ---------------------------------------------
			await expect(saveBtn).toHaveText('Save *', { timeout: 5_000 });

			// 追加ノードは uniqueId("flowgraph.util.log", existing) = "log_2"。
			// (既存 "log" と衝突するため suffix "_2" が自動付与される)
			await expect(canvas.getByText('#log_2', { exact: true })).toBeVisible();

			// --- 7. Ctrl+S → PUT /flowgraph/file/sample 200 -----------------------
			const putPromise = page.waitForResponse(
				(res) =>
					res.url().includes(FILE_PATH) &&
					res.request().method() === 'PUT' &&
					res.status() === 200,
				{ timeout: 15_000 },
			);
			// window にハンドラが付いているので focus は問わない。
			await page.keyboard.press('Control+s');
			const putRes = await putPromise;

			// body に我々の新ノードを含んだ TOML が載ってるはず（serializer 経路を
			// 軽く確認する意味でも）
			const putReqBody = putRes.request().postDataJSON() as { content: string };
			expect(putReqBody.content).toMatch(/id\s*=\s*"log_2"/);
			expect(putReqBody.content).toMatch(/feature\s*=\s*"flowgraph\.util\.log"/);

			// --- 8. dirty フラグが落ちる ------------------------------------------
			// Save 応答後に store.currentFile が更新されて parsed base も進むため、
			// isDirty は false に戻る。
			await expect(saveBtn).toHaveText('Save', { timeout: 10_000 });

			// --- 9. Dry-run の追加確認: GET し直した raw_toml にも新ノードがある -----
			const verifyRes = await request.get(FILE_PATH, { headers: authHeader() });
			expect(verifyRes.status()).toBe(200);
			const verifyToml = ((await verifyRes.json()) as FlowgraphFileResp).raw_toml;
			expect(verifyToml).toMatch(/id\s*=\s*"log_2"/);
		} finally {
			// --- 10. cleanup: 元の TOML に戻して fixture 汚染を残さない ------------
			const restore = await request.put(FILE_PATH, {
				headers: authHeader(),
				data: { content: originalToml },
			});
			expect(restore.status(), await restore.text()).toBe(200);
		}
	});
});
