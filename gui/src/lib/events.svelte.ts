/**
 * `/api/v1/control/events` への WebSocket 購読を Svelte 5 runes で包む。
 *
 * 設計方針:
 *   - **単一の接続**をモジュールスコープに 1 本だけ持つ（シングルトン）。Dashboard と OAuth UI など
 *     複数コンポーネントが同時に subscribe しても WS は 1 本。
 *   - `$state` でストアの外面を作り、コンポーネント側は import して `eventsStore.connection` のような
 *     プロパティを読むだけでリアクティブに更新される。
 *   - **自動再接続**（指数バックオフ）を内蔵。配信中の VAC 再起動や一時的なネットワーク断にも耐える。
 *   - 直近 N 件のイベントをリングバッファで保持。Dashboard のログ表示に直結する。
 *   - Bearer トークンが必要な環境では `?token=...` クエリに乗せて送る（ブラウザ WebSocket は
 *     Authorization ヘッダを付けられないため）。
 */

import { getBearerToken } from './auth';
import type { ControlEvent, ControlEventKind } from './types';

export type ConnectionState = 'idle' | 'connecting' | 'open' | 'closed' | 'error';

/** 直近イベントを保持するリングバッファのデフォルト上限。Dashboard の「最近のイベント」用。 */
const DEFAULT_BUFFER_CAPACITY = 500;

/** 再接続時のバックオフ (ms)。exponential だが上限あり。 */
const RECONNECT_BASE_MS = 500;
const RECONNECT_MAX_MS = 15_000;

/** WS 上で受け取った 1 イベント + 受信時刻（UI 用）。 */
export type TimestampedEvent = {
 /** クライアント側で振った連番 ID（WS の再接続をまたいで単調増加）。 */
 readonly seq: number;
 /** 受信時刻（ブラウザ時計、Date.now() ベース）。 */
 readonly received_at: number;
 readonly event: ControlEvent;
};

class EventsStore {
 /** 現在の WS 接続状態。コンポーネントは `eventsStore.connection` を読めば自動で追従する。 */
 connection: ConnectionState = $state('idle');
 /** 直近の受信イベント（新しいものが後ろ）。リングバッファなので古いものは自動で捨てられる。 */
 recent: TimestampedEvent[] = $state([]);
 /** 直近エラー（接続失敗・再接続理由）。 */
 last_error: string | null = $state(null);
 /** 受信総数（Lagged を含まない "正常受信"）。 */
 received_count: number = $state(0);
 /** 直近の Lagged で落ちた件数の累積。 */
 dropped_count: number = $state(0);

 #capacity: number;
 #seq = 0;
 #ws: WebSocket | null = null;
 #reconnectAttempts = 0;
 #reconnectTimer: ReturnType<typeof setTimeout> | null = null;
 #manualClose = false;
 #listeners = new Set<(ev: TimestampedEvent) => void>();

 constructor(capacity: number = DEFAULT_BUFFER_CAPACITY) {
  this.#capacity = capacity;
 }

 /**
  * WS 接続を開く。既に open / connecting 中なら no-op。
  * アプリ起動時に `main.ts` から一度だけ呼ぶのが基本運用。
  */
 connect(): void {
  if (this.#ws && (this.connection === 'open' || this.connection === 'connecting')) {
   return;
  }
  this.#manualClose = false;
  this.#openSocket();
 }

 /** 明示的に切断する。自動再接続は走らない。 */
 disconnect(): void {
  this.#manualClose = true;
  if (this.#reconnectTimer !== null) {
   clearTimeout(this.#reconnectTimer);
   this.#reconnectTimer = null;
  }
  if (this.#ws) {
   try {
    this.#ws.close(1000, 'client disconnect');
   } catch {
    // ignore
   }
   this.#ws = null;
  }
  this.connection = 'closed';
 }

 /**
  * ControlEvent が届くたびに呼ばれるコールバックを登録する。
  * コンポーネント側で特定のイベントだけ購読したいときに使う。解除関数を返す。
  *
  * ```ts
  * $effect(() => {
  *   const off = eventsStore.subscribe((ev) => {
  *     if (ev.event.kind === 'channel_datum') { ... }
  *   });
  *   return off;
  * });
  * ```
  */
 subscribe(listener: (ev: TimestampedEvent) => void): () => void {
  this.#listeners.add(listener);
  return () => {
   this.#listeners.delete(listener);
  };
 }

 /** リングバッファを空にする（Dashboard の「クリア」ボタン用）。 */
 clearRecent(): void {
  this.recent = [];
 }

 // -------------------------------------------------------------------------
 // 内部実装
 // -------------------------------------------------------------------------

 #openSocket(): void {
  this.connection = 'connecting';
  const url = buildWsUrl();
  let ws: WebSocket;
  try {
   ws = new WebSocket(url);
  } catch (e) {
   this.last_error = e instanceof Error ? e.message : String(e);
   this.connection = 'error';
   this.#scheduleReconnect();
   return;
  }
  this.#ws = ws;

  ws.addEventListener('open', () => {
   this.connection = 'open';
   this.last_error = null;
   this.#reconnectAttempts = 0;
  });

  ws.addEventListener('message', (e) => {
   this.#handleMessage(e.data);
  });

  ws.addEventListener('error', () => {
   // error イベント単体ではメッセージが取れないので、close で扱う
  });

  ws.addEventListener('close', (e) => {
   this.#ws = null;
   if (this.#manualClose) {
    this.connection = 'closed';
    return;
   }
   this.connection = 'closed';
   this.last_error = `WebSocket closed (code=${e.code}${e.reason ? `, reason=${e.reason}` : ''})`;
   this.#scheduleReconnect();
  });
 }

 #handleMessage(data: unknown): void {
  if (typeof data !== 'string') {
   // 本プロトコルでは binary は送らない想定
   return;
  }
  let parsed: ControlEvent;
  try {
   parsed = JSON.parse(data) as ControlEvent;
  } catch (e) {
   this.last_error = `JSON parse failed: ${e instanceof Error ? e.message : String(e)}`;
   return;
  }

  // 既知バリアントを事前チェック（Rust の ControlEvent と同期されている想定）。
  // 不明な kind が来た場合は、Rust 側で variant が追加されたのに types.ts が追従していない状況。
  // ここでは runtime にクラッシュさせず警告のみ残す。consumer の switch で `assertNever` を使えば
  // **型更新漏れ**は compile-time に検出される（本プロジェクトのうま味）。
  if (!KNOWN_KINDS.has((parsed as { kind?: string }).kind ?? '')) {
   console.warn('[events] unknown ControlEvent.kind — types.ts の更新漏れ？', parsed);
   return;
  }

  if (parsed.kind === 'lagged') {
   this.dropped_count += parsed.dropped;
  } else {
   this.received_count += 1;
  }

  const wrapped: TimestampedEvent = {
   seq: this.#seq++,
   received_at: Date.now(),
   event: parsed,
  };

  // リングバッファに push（上限超過なら先頭を捨てる）
  const next = this.recent.concat(wrapped);
  if (next.length > this.#capacity) {
   next.splice(0, next.length - this.#capacity);
  }
  this.recent = next;

  for (const listener of this.#listeners) {
   try {
    listener(wrapped);
   } catch (e) {
    console.error('[events] listener threw', e);
   }
  }
 }

 #scheduleReconnect(): void {
  if (this.#manualClose) return;
  if (this.#reconnectTimer !== null) return;
  const delay = Math.min(RECONNECT_BASE_MS * 2 ** this.#reconnectAttempts, RECONNECT_MAX_MS);
  this.#reconnectAttempts += 1;
  this.#reconnectTimer = setTimeout(() => {
   this.#reconnectTimer = null;
   this.#openSocket();
  }, delay);
 }
}

// ---------------------------------------------------------------------------
// URL 組み立て
// ---------------------------------------------------------------------------

function buildWsUrl(): string {
 const { protocol, host } = window.location;
 const wsProto = protocol === 'https:' ? 'wss:' : 'ws:';
 const base = `${wsProto}//${host}/api/v1/control/events`;
 const token = getBearerToken();
 return token ? `${base}?token=${encodeURIComponent(token)}` : base;
}

/**
 * types.ts の `ControlEventKind` と同期している既知 kind の集合。
 *
 * **整合性チェック**: この配列の型は `readonly ControlEventKind[]` で制約する。
 * - types.ts の `ControlEventKind` に新規 variant が追加されたら、ここを更新する必要がある。
 * - 更新し忘れて runtime に来た場合は `#handleMessage` が警告のみ残して drop する（クラッシュしない）。
 * - 逆に `ControlEventKind` に無い文字列をここに書くと TS エラー。
 */
const KNOWN_KIND_LIST: readonly ControlEventKind[] = [
 'channel_datum',
 'lagged',
 'heartbeat',
 'pause_state',
 'reloaded',
 'oauth_status',
 'processor_invoked',
 'restarting',
 'managed_app_state',
 'flowgraph_reloaded',
] as const;

const KNOWN_KINDS = new Set<string>(KNOWN_KIND_LIST);

/** ControlEvent の型述語。将来 zod/valibot で本格的な検証に差し替える予定の足場。 */
export function isControlEvent(x: unknown): x is ControlEvent {
 if (typeof x !== 'object' || x === null) return false;
 const kind = (x as { kind?: unknown }).kind;
 return typeof kind === 'string' && KNOWN_KINDS.has(kind);
}

// ---------------------------------------------------------------------------
// シングルトン
// ---------------------------------------------------------------------------

export const eventsStore = new EventsStore();
