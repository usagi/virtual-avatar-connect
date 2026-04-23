import { expect, test } from '@playwright/test';
import { authHeader, tokenQuery } from './fixtures';

/**
 * §3.4 live-quick-add-learn-undo
 *
 * conf.fixture.e2e.toml の `[[control_api.tables]]` + fixture flowgraph の
 * `dictionary.learn` / `dictionary.forget` ノードを使い、Live タブの
 * Live Quick-Add ウィジェットからの一連の UX を検証する。
 *
 *   1. Quick-Add カタログに sample_dict が出てくる
 *   2. source / replacement を入力して [Learn] を押すと
 *      POST /api/v1/control/flowgraph/default/trigger/sample::learn が
 *      期待 body で飛ぶ
 *   3. 履歴カウンタが 1 になり、[履歴] 展開で 1 行出る
 *   4. [Undo] を押すと
 *      POST /api/v1/control/flowgraph/default/trigger/sample::forget が飛び、
 *      行が "Undone" に遷移する
 *
 * 設計メモ:
 *   E2E ではファイル永続化 (table.write_tsv) までは配線しない。
 *   Quick-Add の関心事は「GUI → Control API → node 発火まで届いたか」であり、
 *   テーブル永続化は §3.3 Editor 側の責務として分離する。
 */

const TRIGGER_PREFIX = '/api/v1/control/flowgraph/default/trigger/';

test.describe('§3.4 live-quick-add-learn-undo', () => {
	test('Learn fires trigger POST and Undo fires forget trigger POST', async ({
		page,
		request,
	}) => {
		await page.goto(`/gui/${tokenQuery()}`);

		// Live タブ（default 表示）と Quick-Add カタログ読み込みを待つ。
		// API 経由でも catalog を別途 verify しておくと、UI が見つからないときに
		// conf 側の問題か UI 側の問題かを切り分けやすい。
		const catalog = await request.get('/api/v1/control/tables', {
			headers: authHeader(),
		});
		expect(catalog.status(), await catalog.text()).toBe(200);
		const catalogBody = (await catalog.json()) as {
			tables: Array<{ key: string; quick_add: unknown }>;
		};
		expect(
			catalogBody.tables.some(
				(t) => t.key === 'sample_dict' && t.quick_add != null,
			),
		).toBe(true);

		// Live Quick-Add widget のスコープを <section> レベルに確定させる。
		// DictionaryLiveQuickAdd.svelte は <section> > <header> > <h3>Live Quick-Add</h3> の構造。
		const quickAdd = page
			.locator('section')
			.filter({ has: page.getByRole('heading', { name: 'Live Quick-Add' }) });
		await expect(quickAdd).toBeVisible({ timeout: 20_000 });

		// placeholder ベースで input を探す（文言は DictionaryLiveQuickAdd.svelte の real text）。
		const sourceInput = quickAdd.getByPlaceholder(
			/source（聞こえた音 \/ 置換元）/,
		);
		const replacementInput = quickAdd.getByPlaceholder(
			/replacement（正しい読み \/ 置換先）/,
		);
		await expect(sourceInput).toBeVisible();
		await expect(replacementInput).toBeVisible();

		const nonce = `nu24-${Date.now()}`;
		const source = `e2e_source_${nonce}`;
		const replacement = `E2E 置換先 ${nonce}`;

		await sourceInput.fill(source);
		await replacementInput.fill(replacement);

		// Learn トリガ POST を捕まえる。node_id の URL encode 形は "sample%3A%3Alearn"。
		const learnRequestPromise = page.waitForRequest(
			(req) =>
				req.method() === 'POST' &&
				req.url().includes(TRIGGER_PREFIX) &&
				decodeURIComponent(req.url()).includes('sample::learn'),
			{ timeout: 15_000 },
		);

		await quickAdd.getByRole('button', { name: 'Learn' }).click();

		const learnRequest = await learnRequestPromise;
		const learnBody = learnRequest.postDataJSON() as {
			inputs: { source: string; replacement: string; kind: string; by?: string };
		};
		expect(learnBody.inputs.source).toBe(source);
		expect(learnBody.inputs.replacement).toBe(replacement);
		expect(learnBody.inputs.kind).toBe('literal');
		expect(learnBody.inputs.by).toBe('gui:quick_add');

		// 202 Accepted が返る（server 側実装: HttpResponse::Accepted）。
		const learnResponse = await learnRequest.response();
		expect(learnResponse?.status()).toBe(202);

		// 履歴カウンタが 1 になるはず（button label に "(1)" を含む）。
		const historyToggle = quickAdd.getByRole('button', { name: /^履歴 \(/ });
		await expect(historyToggle).toContainText('(1)', { timeout: 5_000 });

		// 展開して undo ボタンを出す。
		await historyToggle.click();
		const undoButton = quickAdd.getByRole('button', { name: 'Undo' });
		await expect(undoButton).toBeVisible();

		// Forget トリガ POST を捕まえる。
		const forgetRequestPromise = page.waitForRequest(
			(req) =>
				req.method() === 'POST' &&
				req.url().includes(TRIGGER_PREFIX) &&
				decodeURIComponent(req.url()).includes('sample::forget'),
			{ timeout: 15_000 },
		);
		await undoButton.click();
		const forgetRequest = await forgetRequestPromise;
		const forgetBody = forgetRequest.postDataJSON() as {
			inputs: { source: string; replacement: string; mode?: string };
		};
		expect(forgetBody.inputs.source).toBe(source);
		expect(forgetBody.inputs.replacement).toBe(replacement);
		expect(forgetBody.inputs.mode).toBe('latest');

		const forgetResponse = await forgetRequest.response();
		expect(forgetResponse?.status()).toBe(202);

		// Undo 済み表示に遷移する。
		await expect(
			quickAdd.getByRole('button', { name: 'Undone' }),
		).toBeVisible({ timeout: 5_000 });
	});
});
