/**
 * §3.1 control-panel-smoke — baseline control panel smoke.
 *
 * Asserts:
 *   a) The GUI loads at `/gui/?token=<fixture>` and the main TabNav renders.
 *   b) `/api/v1/control/ping` is reachable with the fixture Bearer token,
 *      proving the auth middleware accepts the configured bearer_token.
 *
 * This is deliberately narrow — any failure here means the webServer
 * boot path, static GUI assets, or the control_api auth layer regressed.
 */
import { expect, test } from '@playwright/test';

import { authHeader, tokenQuery } from './fixtures';

test.describe('§3.1 control-panel-smoke', () => {
 test('GUI loads and Control API auth works with fixture token', async ({
  page,
  request,
 }) => {
  await page.goto(`/gui/${tokenQuery()}`);

  // TabNav.svelte renders plain `<button>` elements inside a
  // `<nav aria-label="Main tabs">` landmark. Scope the lookup to that
  // landmark (§5.3 selector policy).
  const tabs = page.getByRole('navigation', { name: 'Main tabs' });
  await expect(
   tabs.getByRole('button', { name: /live/i }),
  ).toBeVisible({ timeout: 15_000 });

  // /ping is the lightest endpoint inside the auth-wrapped scope; 200
  // here means routing + bearer_token both work.
  const res = await request.get('/api/v1/control/ping', {
   headers: authHeader(),
  });
  expect(res.status(), await res.text()).toBe(200);
 });
});
