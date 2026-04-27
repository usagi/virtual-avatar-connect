import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: Now dashboard', () => {
 test('Now is the first screen and shows runtime summary cards', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}`);

  await expect(page.getByRole('heading', { name: 'Now' })).toBeVisible({
   timeout: 15_000,
  });
  const main = page.getByRole('main');
  await expect(main.getByText('Runtime cockpit')).toBeVisible();

  await expect(main.getByText('接続')).toBeVisible();
  await expect(main.getByText('Mode', { exact: true }).first()).toBeVisible();
  await expect(main.getByText('AI', { exact: true })).toBeVisible();
  await expect(main.getByText('連携アプリ').first()).toBeVisible();
  await expect(main.getByText('Flowgraph', { exact: true })).toBeVisible();

  await expect(main.getByText('Runtime', { exact: true })).toBeVisible();
  await expect(main.getByText('Flowgraph 問題')).toBeVisible();
  await expect(main.getByText('最近のイベント')).toBeVisible();
 });

 test('Now quick actions navigate to operational surfaces', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}`);
  const main = page.getByRole('main');

  await main.getByRole('button', { name: '管理' }).click();
  await expect(page.getByRole('heading', { name: 'Modes' })).toBeVisible({ timeout: 15_000 });

  await page.getByRole('navigation', { name: 'Main tabs' }).getByRole('button', { name: 'Now' }).click();
  await main.getByRole('button', { name: 'Flowgraph Studio' }).click();
  await expect(page.getByText('root =')).toBeVisible({ timeout: 15_000 });

  await page.getByRole('navigation', { name: 'Main tabs' }).getByRole('button', { name: 'Now' }).click();
  await main.getByRole('button', { name: 'Resources' }).click();
  await expect(page.getByRole('heading', { name: '連携アプリ設定（run_with）' })).toBeVisible({
   timeout: 15_000,
  });
 });
});
