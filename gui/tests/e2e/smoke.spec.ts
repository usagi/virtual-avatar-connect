/**
 * Phase ν-1: scaffolding-level smoke spec.
 *
 * This single spec is shipped as part of ν-1 so the new Playwright setup
 * (config + webServer + fixture conf + fixture Bearer token) is end-to-end
 * verifiable before ν-2 starts layering on the real 5 cases. Once ν-2
 * lands we keep this spec as the canonical `control-panel-smoke`
 * (§3.1 of the phase doc).
 */
import { expect, test } from '@playwright/test';

const TOKEN = 'e2e-fixture-token';

test.describe('Phase ν-1 scaffolding smoke', () => {
 test('GUI loads and Control API auth works with fixture token', async ({
  page,
  request,
 }) => {
  await page.goto(`/gui/?token=${TOKEN}`);

  // TabNav is rendered. TabNav.svelte uses plain `<button>` inside
  // `<nav aria-label="Main tabs">`, so we scope the role lookup to that
  // nav to avoid collisions with other "Live" buttons on the page.
  const tabs = page.getByRole('navigation', { name: 'Main tabs' });
  await expect(
   tabs.getByRole('button', { name: /live/i }),
  ).toBeVisible({ timeout: 15_000 });

  // Control API is reachable and auth succeeds with the fixture token.
  // /ping is the lightest endpoint inside the auth-wrapped scope, so it's
  // ideal for confirming both routing and bearer_token handling.
  const res = await request.get('/api/v1/control/ping', {
   headers: { Authorization: `Bearer ${TOKEN}` },
  });
  expect(res.status(), await res.text()).toBe(200);
 });
});
