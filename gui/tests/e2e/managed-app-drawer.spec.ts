import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI shell: 連携アプリドロワー', () => {
 test('shell action からドロワーを開ける', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}`);

  await page.getByRole('button', { name: '連携アプリ…' }).click();

  const drawer = page.getByRole('complementary', { name: 'Managed app drawer' });
  await expect(drawer.getByRole('heading', { name: '連携アプリ' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(drawer.getByText('run_with で管理するアプリの監視と操作')).toBeVisible();
  await expect(drawer.getByRole('button', { name: '再読込' })).toBeVisible();
  await expect(drawer.getByRole('button', { name: '閉じる' })).toBeVisible();
 });
});
