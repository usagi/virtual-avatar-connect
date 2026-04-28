import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: リソース概要', () => {
 test('Resources に Managed App の運用概要を表示する', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#resources`);

  await expect(page.getByRole('heading', { name: 'リソース概要' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(page.getByText('現在の run_with 登録から Managed App の状態を確認します。')).toBeVisible();
  await expect(page.getByText('Apps', { exact: true })).toBeVisible();
  await expect(page.getByText('起動中', { exact: true })).toBeVisible();
  await expect(page.getByText('追跡対象', { exact: true })).toBeVisible();
  await expect(page.getByText('PIDs', { exact: true })).toBeVisible();
  await expect(page.getByRole('heading', { name: 'VMC passthrough' })).toBeVisible();
  await expect(page.getByText('motion 層の UDP 転送状態と packet 統計を確認します。')).toBeVisible();
  await expect(page.getByText('Routes', { exact: true })).toBeVisible();
  await expect(page.getByText('Packets', { exact: true })).toBeVisible();
  await expect(page.getByRole('button', { name: 'route 追加' })).toBeVisible();
 });
});
