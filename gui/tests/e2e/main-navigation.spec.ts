import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: main navigation', () => {
 test('new top-level surfaces are reachable from the main navigation', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}`);

  const tabs = page.getByRole('navigation', { name: 'Main tabs' });
  await expect(tabs.getByRole('button', { name: /now/i })).toBeVisible({
   timeout: 15_000,
  });

  await tabs.getByRole('button', { name: /modes/i }).click();
  await expect(page.getByRole('heading', { name: 'Modes' })).toBeVisible();
  await expect(page.getByText('Runtime mode control')).toBeVisible();

  await tabs.getByRole('button', { name: /flowgraph studio/i }).click();
  await expect(page.getByText('root =')).toBeVisible({ timeout: 15_000 });

  await tabs.getByRole('button', { name: /resources/i }).click();
  await expect(page.getByRole('heading', { name: 'OAuth (Twitch Device Code Flow)' })).toBeVisible({
   timeout: 15_000,
  });

  await tabs.getByRole('button', { name: /observability/i }).click();
  await expect(page.getByRole('heading', { name: 'Observability' })).toBeVisible({
   timeout: 15_000,
  });

  await tabs.getByRole('button', { name: /settings/i }).click();
  await expect(page.getByRole('heading', { name: 'Connection' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(page.getByRole('heading', { name: 'Developer utilities' })).toBeVisible({
  });
 });

 test('legacy hashes fall back to the new information architecture', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#live`);
  await expect(page.getByRole('heading', { name: 'Now' })).toBeVisible({
   timeout: 15_000,
  });

  await page.goto(`/gui/${tokenQuery()}#setup`);
  await expect(page.getByRole('heading', { name: 'OAuth (Twitch Device Code Flow)' })).toBeVisible({
   timeout: 15_000,
  });

  await page.goto(`/gui/${tokenQuery()}#logs`);
  await expect(page.getByRole('heading', { name: 'Observability' })).toBeVisible({
   timeout: 15_000,
  });

  await page.goto(`/gui/${tokenQuery()}#tools`);
  await expect(page.getByRole('heading', { name: 'Connection' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(page.getByRole('heading', { name: 'Developer utilities' })).toBeVisible({
  });
 });
});
