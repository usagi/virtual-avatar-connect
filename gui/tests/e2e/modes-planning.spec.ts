import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: Modes planning surface', () => {
 test('planned modes show transition preview and Runtime Mode backend status', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#modes`);

  const main = page.getByRole('main');
  await expect(page.getByRole('heading', { name: 'Modes' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(main.getByText('遷移プレビュー', { exact: true })).toBeVisible();
  await expect(main.getByText('現在の mode:')).toBeVisible();
  await expect(main.getByText('設定済み modes:')).toBeVisible();
  await expect(main.getByText('daily', { exact: true }).first()).toBeVisible();

  await main.getByRole('button', { name: /Work/ }).click();
  await expect(main.getByText('important_only')).toBeVisible();
  await expect(main.getByRole('button', { name: '先に mode を設定' })).toBeDisabled();

  await main.getByRole('button', { name: /Streaming/ }).click();
  await expect(main.getByText('stream_safe')).toBeVisible();
  await expect(main.getByText('Dry-run plan', { exact: true })).toBeVisible();
  await expect(main.getByText('Flowgraph desired state')).toBeVisible();
  await expect(main.getByText('Managed App desired state')).toBeVisible();
  await expect(main.getByText('Managed App の変更はありません。')).toBeVisible();
  await expect(main.getByText('daily').first()).toBeVisible();
  await expect(main.getByText('streaming').last()).toBeVisible();
  await expect(main.getByRole('button', { name: '遷移' })).toBeEnabled();
  await main.getByRole('button', { name: '遷移' }).click();
  await expect(main.getByText('遷移進捗')).toBeVisible();
  await expect(main.getByText(/completed|applying|firing|suppressing/)).toBeVisible();
  await expect(main.getByText('現在の mode:')).toBeVisible();
  await expect(main.getByRole('button', { name: '現在の mode' })).toBeDisabled();
 });
});
