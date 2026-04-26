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

  await expect(main.getByText('Connection')).toBeVisible();
  await expect(main.getByText('AI', { exact: true })).toBeVisible();
  await expect(main.getByText('Managed Apps')).toBeVisible();
  await expect(main.getByText('Flowgraph', { exact: true })).toBeVisible();

  await expect(main.getByText('Runtime', { exact: true })).toBeVisible();
  await expect(main.getByText('Recent Events')).toBeVisible();
 });
});
