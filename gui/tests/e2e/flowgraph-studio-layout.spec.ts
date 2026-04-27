import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: Flowgraph Studio layout', () => {
 test('Flowgraph Studio exposes IDE-style workspace regions', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#flowgraph`);

  await expect(page.getByRole('heading', { name: 'Flowgraph Studio' })).toBeVisible({
   timeout: 15_000,
  });

  const main = page.getByRole('main');
  await expect(main.getByText('Author, inspect, and repair runtime dataflow files.')).toBeVisible();
  await expect(main.getByText('Files', { exact: true }).first()).toBeVisible();
  await expect(main.getByText('Canvas', { exact: true })).toBeVisible();
  await expect(main.getByText('Node Palette', { exact: true })).toBeVisible();
  await expect(main.getByText('Inspector', { exact: true })).toBeVisible();
  await expect(main.getByText('Problems', { exact: true })).toBeVisible();
  await expect(main.getByText('Nodes', { exact: true })).toBeVisible();
  await expect(main.getByText('Edges', { exact: true })).toBeVisible();
 });
});
