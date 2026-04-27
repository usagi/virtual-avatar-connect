import { expect, test, type Page } from '@playwright/test';

import { tokenQuery } from './fixtures';

const THEMES = ['dr-usagi-default', 'dark-crimson', 'light-silver', 'soft-cute'] as const;

const SURFACES = [
 { hash: 'settings', marker: 'テーマ' },
 { hash: 'now', marker: 'Now' },
 { hash: 'flowgraph', marker: 'root =' },
] as const;

async function openSurfaceWithTheme(page: Page, theme: string, hash: string) {
 await page.addInitScript(
  ({ storageKey, themeId }) => {
   window.localStorage.setItem(storageKey, themeId);
  },
  { storageKey: 'vac.gui.theme', themeId: theme },
 );
 await page.goto(`/gui/${tokenQuery()}#${hash}`);
 await expect(page.locator('html')).toHaveAttribute('data-vac-theme', theme);
}

test.describe('GUI visual theme smoke', () => {
 for (const theme of THEMES) {
  test(`${theme} renders key cockpit surfaces`, async ({ page }) => {
   await page.setViewportSize({ width: 1366, height: 768 });

   for (const surface of SURFACES) {
    await openSurfaceWithTheme(page, theme, surface.hash);
    await expect(page.getByText(surface.marker).first()).toBeVisible({ timeout: 15_000 });

    const screenshot = await page.screenshot({ fullPage: false });
    expect(screenshot.byteLength).toBeGreaterThan(20_000);
   }
  });
 }
});
