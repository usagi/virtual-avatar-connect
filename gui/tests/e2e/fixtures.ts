/**
 * Shared helpers for Phase ν Playwright specs.
 *
 * - TOKEN must stay in sync with conf.fixture.e2e.toml `[control_api].bearer_token`.
 * - authHeader() / tokenQuery() are the two ways the GUI accepts Bearer tokens:
 *   HTTP Authorization header for REST, and `?token=...` query for WebSocket
 *   (browsers cannot attach Authorization to ws:// upgrade). We mirror both in
 *   the specs so any future regression on either path is caught.
 */

export const TOKEN = 'e2e-fixture-token';

export function authHeader(): Record<string, string> {
 return { Authorization: `Bearer ${TOKEN}` };
}

/** Returns `?token=<TOKEN>` (URL-safe) for attaching to a navigation URL. */
export function tokenQuery(): string {
 return `?token=${encodeURIComponent(TOKEN)}`;
}
