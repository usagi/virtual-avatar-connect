import { expect, test } from '@playwright/test';

import { tokenQuery } from './fixtures';

test.describe('GUI redesign: Modes planning surface', () => {
 test('planned modes show transition preview without enabling fake controls', async ({ page }) => {
  await page.goto(`/gui/${tokenQuery()}#modes`);

  const main = page.getByRole('main');
  await expect(page.getByRole('heading', { name: 'Modes' })).toBeVisible({
   timeout: 15_000,
  });
  await expect(main.getByText('Transition Preview', { exact: true })).toBeVisible();
  await expect(main.getByRole('button', { name: 'Transit unavailable' })).toBeDisabled();

  await main.getByRole('button', { name: /Sleep/ }).click();
  await expect(main.getByText('critical_only')).toBeVisible();
  await expect(main.getByText('emergency_alerts').last()).toBeVisible();

  await main.getByRole('button', { name: /Streaming/ }).click();
  await expect(main.getByText('stream_safe')).toBeVisible();
  await expect(main.getByText('start obs').last()).toBeVisible();
 });
});
