import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI shell: Managed Apps drawer', () => {
 test('opens the drawer from the shell action', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}`);

  await page.getByRole('button', { name: 'Managed Apps...' }).click();

  const drawer = page.getByRole('complementary', { name: 'Managed Apps' });
  await expect(drawer.getByRole('heading', { name: 'Managed Apps' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(drawer.getByText('Monitor and control apps registered through run_with.')).toBeVisible();
  await expect(drawer.getByRole('button', { name: 'Refresh' })).toBeVisible();
  await expect(drawer.getByRole('button', { name: 'Close' })).toBeVisible();
 });
});
