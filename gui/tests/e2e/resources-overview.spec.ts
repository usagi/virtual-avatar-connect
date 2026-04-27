import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: Resources overview', () => {
 test('Resources shows Managed App operational summary', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#resources`);

  await expect(page.getByRole('heading', { name: 'Resource Overview' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(page.getByText('Managed App status from the current run_with registry.')).toBeVisible();
  await expect(page.getByText('Apps', { exact: true })).toBeVisible();
  await expect(page.getByText('Running', { exact: true })).toBeVisible();
  await expect(page.getByText('Tracked', { exact: true })).toBeVisible();
  await expect(page.getByText('PIDs', { exact: true })).toBeVisible();
 });
});
