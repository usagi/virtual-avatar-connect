import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI visual theme', () => {
 test('theme selection is applied and restored from localStorage', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#settings`);

  await expect(page.getByRole('heading', { name: 'テーマ' })).toBeVisible({
   timeout: 15_000,
  });

  await page.getByRole('button', { name: /Dark Crimson/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-vac-theme', 'dark-crimson');
  await expect(page.getByRole('button', { name: /Dark Crimson/ })).toHaveAttribute(
   'aria-pressed',
   'true',
  );

  await page.reload();
  await expect(page.locator('html')).toHaveAttribute('data-vac-theme', 'dark-crimson');

  await page.getByRole('button', { name: /Light Silver/ }).click();
  await expect(page.locator('html')).toHaveAttribute('data-vac-theme', 'light-silver');
  const storedTheme = await page.evaluate(() => window.localStorage.getItem('vac.gui.theme'));
  expect(storedTheme).toBe('light-silver');
 });
});
