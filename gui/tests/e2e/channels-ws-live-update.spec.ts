/**
 * §3.5 channels-ws-live-update — WebSocket live propagation.
 *
 * Verifies that when something is pushed through Control API `/ingress`,
 * the already-open `/api/v1/control/events` WebSocket (driven by
 * `gui/src/lib/events.svelte.ts`) delivers a matching `channel_datum`
 * frame to the browser within a short window.
 *
 * Why this matters: Phase γ-2 / φ-3e relies on the WS path for near-
 * realtime channel views. A silent regression to WS wiring (auth,
 * broadcast channel lagging, frame shape) is only caught by end-to-end
 * evidence that a POST on one socket is observed on another.
 *
 * We attach the Playwright WS listener before navigating, so we cannot
 * miss the frame to a race with the GUI's own `eventsStore.connect()`.
 */
import { expect, test } from '@playwright/test';

import { authHeader, TOKEN, tokenQuery } from './fixtures';

const CHANNEL = 'e2e_ws_channel';

test.describe('§3.5 channels-ws-live-update', () => {
 test('ingress POST is delivered as channel_datum over /events WS', async ({
  page,
  request,
 }) => {
  const controlWsPath = '/api/v1/control/events';
  const nonce = `e2e-ws-${Date.now()}`;
  const content = `nu-2 ws propagation ${nonce}`;

  // Capture the control WS as soon as the page opens it. `eventsStore`
  // is a module singleton that connects during module init, so a single
  // websocket event is expected right after navigation.
  const wsPromise = page.waitForEvent('websocket', {
   predicate: (ws) => ws.url().includes(controlWsPath),
   timeout: 20_000,
  });
  await page.goto(`/gui/${tokenQuery()}`);
  const ws = await wsPromise;

  // Wait for the channel_datum frame that carries our unique content.
  // `lagged` / `heartbeat` frames are ignored via the predicate.
  // ControlEvent::ChannelDatum is a flat object: { kind, phase, id,
  // channel, content, flags, datetime }. No nested `datum` wrapper.
  const framePromise = ws.waitForEvent('framereceived', {
   predicate: (frame) => {
    if (typeof frame.payload !== 'string') return false;
    try {
     const obj = JSON.parse(frame.payload) as {
      kind?: string;
      channel?: string;
      content?: string;
     };
     return (
      obj.kind === 'channel_datum' &&
      obj.channel === CHANNEL &&
      obj.content === content
     );
    } catch {
     return false;
    }
   },
   timeout: 15_000,
  });

  const ingress = await request.post('/api/v1/control/ingress', {
   headers: { ...authHeader(), 'Content-Type': 'application/json' },
   data: { channel: CHANNEL, content, is_final: true },
  });
  expect(ingress.status(), await ingress.text()).toBe(200);

  const frame = await framePromise;
  // Parse once more so a future shape change fails with a readable diff
  // instead of a vague timeout.
  const decoded = JSON.parse(
   typeof frame.payload === 'string' ? frame.payload : '',
  ) as {
   kind: string;
   phase: string;
   channel: string;
   content: string;
  };
  expect(decoded.kind).toBe('channel_datum');
  expect(decoded.channel).toBe(CHANNEL);
  expect(decoded.content).toBe(content);

  // Bonus assertion: the fixture token is what authorized the WS.
  // This catches future regressions where we accidentally allowlist
  // tokenless WS upgrades on loopback.
  expect(ws.url()).toContain(`token=${encodeURIComponent(TOKEN)}`);
 });
});
