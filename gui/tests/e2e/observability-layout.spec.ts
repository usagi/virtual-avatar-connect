import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: Observability layout', () => {
 test('Observability presents snapshot and event timeline as operational evidence', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#observability`);

  await expect(page.getByRole('heading', { name: 'Observability' })).toBeVisible({
   timeout: 15_000,
  });

  const main = page.getByRole('main');
  await expect(main.getByText('Runtime snapshot, live event timeline, and operational evidence.')).toBeVisible();
  await expect(main.getByText('Snapshot', { exact: true })).toBeVisible();
  await expect(main.getByText('Timeline', { exact: true })).toBeVisible();
  await expect(main.getByText('Filters', { exact: true })).toBeVisible();
  await expect(main.getByRole('button', { name: 'channel_datum' })).toBeVisible();
  await expect(main.getByRole('button', { name: 'flowgraph_reloaded' })).toBeVisible();
  await expect(main.getByRole('heading', { name: 'Event History' })).toBeVisible();
  await expect(main.getByText('Server-side ring buffer for operational evidence.')).toBeVisible();
 });
});
