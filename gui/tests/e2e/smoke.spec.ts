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

  // TabNav is rendered (Live / Setup / Flowgraph / Logs / Tools).
  // We only assert one tab by role+name to keep this spec narrow.
  await expect(
   page.getByRole('tab', { name: /live/i }),
  ).toBeVisible({ timeout: 15_000 });

  // Control API is reachable and auth succeeds with the fixture token.
  const res = await request.get('/api/v1/control/channels', {
   headers: { Authorization: `Bearer ${TOKEN}` },
  });
  expect(res.status(), await res.text()).toBe(200);
 });
});
