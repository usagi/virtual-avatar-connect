/**
 * Phase ν — Playwright E2E test configuration.
 *
 * Design is pinned in `docs/roadmap/phase-nu-gui-e2e-playwright.md` (§5).
 *
 * Key decisions baked in here:
 *  - `testDir = 'tests/e2e'`: specs live inside `gui/` so Cargo never
 *    accidentally picks them up.
 *  - `webServer` boots **VAC itself** (Rust, release build) against
 *    `conf.fixture.e2e.toml` on port 57098. We deliberately do *not* go
 *    through the Vite dev proxy — E2E has to exercise the production
 *    `/gui/` path served by actix.
 *  - `workers: 1` for the initial ν-2 batch. We only bump this once all
 *    5 baseline specs are green and we understand their race profile.
 *  - No `toHaveScreenshot` / visual baselines in ν. That is ν+ scope.
 *
 * Local usage:
 *   cd gui
 *   npm run test:e2e:install        # one-shot chromium download
 *   npm run test:e2e                # headless run
 *   npm run test:e2e:ui             # interactive / time-travel UI
 *
 * The fixture ships a fixed Bearer token (`e2e-fixture-token`) so that
 * auth is actually exercised; specs attach it through the URL query
 * (`?token=...`) which `src/lib/auth.ts` persists to localStorage.
 */
import { defineConfig, devices } from '@playwright/test';

const PORT = 57098;
const BASE_URL = `http://127.0.0.1:${PORT}`;
const FIXTURE_CONF = 'conf.fixture.e2e.toml';

const isCI = !!process.env.CI;

export default defineConfig({
 testDir: 'tests/e2e',
 timeout: 60_000,
 expect: { timeout: 10_000 },
 fullyParallel: false,
 forbidOnly: isCI,
 retries: isCI ? 1 : 0,
 workers: 1,
 reporter: isCI
  ? [['github'], ['html', { open: 'never' }]]
  : [['list'], ['html', { open: 'never' }]],
 use: {
  baseURL: BASE_URL,
  trace: 'on-first-retry',
  video: 'retain-on-failure',
  screenshot: 'only-on-failure',
 },
 projects: [
  {
   name: 'chromium',
   use: { ...devices['Desktop Chrome'] },
  },
 ],
 webServer: [
  {
   // We rely on the developer having run `cargo build --release` at
   // least once. `cargo run --release` on a warm target is ~1-2s.
   command: `cargo run --quiet --release --bin virtual-avatar-connect-cli -- ${FIXTURE_CONF}`,
   cwd: '..',
   url: `${BASE_URL}/gui/`,
   reuseExistingServer: !isCI,
   timeout: 180_000,
   stdout: 'pipe',
   stderr: 'pipe',
  },
 ],
});
