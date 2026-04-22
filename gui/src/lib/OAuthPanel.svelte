<script lang="ts">
 // Twitch Device Code Flow を GUI から回すためのパネル。
 //
 // 仕様（Rust 側 /api/v1/control/oauth/twitch/{broadcaster|moderator}/*）:
 //   - POST /start  → outcome ∈ {started, already_pending, already_authorized} + session (pending 時のみ user_code/uri を含む)
 //   - GET  /status → OAuthSessionView or 404 (セッション無し)
 //   - POST /cancel → canceled + session (最終スナップショット)
 //
 // UX 要点:
 //   - 両アカウント（broadcaster / moderator）をタブで切替
 //   - 現セッションがあれば user_code と verification_uri を大きく提示し、Copy/Open を一押しで済ませる
 //   - `ControlEvent.oauth_status` を WS で受信したら即時 state 反映
 //   - pending 中は残り時間（expires_at）を秒単位で更新

 import { onMount, onDestroy } from 'svelte';
 import { api } from './api';
 import {
  ControlApiError,
  type OAuthAccount,
  type OAuthSessionStatus,
  type OAuthSessionView,
  type StateSnapshot,
 } from './types';
 import { eventsStore } from './events.svelte';

 const ACCOUNTS: readonly OAuthAccount[] = ['broadcaster', 'moderator'];

 let tab: OAuthAccount = $state('broadcaster');

 /** 各 account の現在のセッションビュー（未開始なら null） */
 let sessions = $state<Record<OAuthAccount, OAuthSessionView | null>>({
  broadcaster: null,
  moderator: null,
 });

 /**
  * snapshot 由来の情報。
  * `twitch.{broadcaster,moderator}_authorized` を参照することで、DCF 開始前から
  * 「保存済みトークンで既に認可済みか」をユーザに提示できる（起動時認可検出）。
  */
 let snapshot = $state<StateSnapshot | null>(null);

 /** 操作中ロック */
 let busy = $state<Record<OAuthAccount, boolean>>({
  broadcaster: false,
  moderator: false,
 });

 /** 各アカウントの操作エラー（直近 1 件） */
 let errors = $state<Record<OAuthAccount, string | null>>({
  broadcaster: null,
  moderator: null,
 });

 /** 1 秒ごとに更新されるシンプルなカウンタ（カウントダウン表示用） */
 let tick = $state(0);
 let tickTimer: ReturnType<typeof setInterval> | null = null;

 let unsubscribe: (() => void) | null = null;

 onMount(() => {
  // 初期状態を 2 系統から取得:
  //   1. snapshot → `twitch.*_authorized` で「保存トークンが有効か」を判断
  //   2. /status   → 進行中 DCF セッションがあればその詳細
  void refreshSnapshot();
  for (const acc of ACCOUNTS) {
   void refreshStatus(acc);
  }

  // WS イベント: oauth_status を直接反映（polling 不要）
  unsubscribe = eventsStore.subscribe((ev) => {
   if (ev.event.kind === 'oauth_status') {
    const { account, view } = ev.event;
    if (view.status === 'authorized') {
     // DCF 完了後は「セッション詳細」を見せ続ける意味が無い。
     // snapshot ベースの「✓ 認可済み」カード表示に集約することで、起動直後の
     // 認可済みケースと DCF 直後の認可済みケースの UI を完全一致させる。
     sessions = { ...sessions, [account]: null };
     void refreshSnapshot();
    } else {
     sessions = { ...sessions, [account]: view };
     // expired/canceled/failed でも保存トークン状態が変わりうるので snapshot を更新。
     if (view.status !== 'pending') void refreshSnapshot();
    }
   }
  });

  // 残り秒数を 1Hz で更新
  tickTimer = setInterval(() => {
   tick += 1;
  }, 1_000);
 });

 onDestroy(() => {
  if (unsubscribe) unsubscribe();
  if (tickTimer !== null) clearInterval(tickTimer);
 });

 async function refreshSnapshot(): Promise<void> {
  try {
   snapshot = await api.snapshot();
  } catch {
   // OAuth パネルは snapshot が取れなくても sessions だけで動ける。
   // 取得エラーは SnapshotView 側で顕現するため、ここではサイレントに扱う。
  }
 }

 async function refreshStatus(acc: OAuthAccount): Promise<void> {
  try {
   const view = await api.oauthStatusOrNull(acc);
   // authorized セッションは snapshot に集約するため表示用には保持しない。
   sessions = { ...sessions, [acc]: view?.status === 'authorized' ? null : view };
   errors = { ...errors, [acc]: null };
  } catch (e) {
   errors = { ...errors, [acc]: formatError(e) };
  }
 }

 async function start(acc: OAuthAccount): Promise<void> {
  if (busy[acc]) return;
  busy = { ...busy, [acc]: true };
  errors = { ...errors, [acc]: null };
  try {
   const res = await api.oauthStart(acc);
   // `started` / `already_pending` は session が入ってくる → sessions を差し替え
   // `already_authorized` は session=null（authorized 状態は snapshot 経由で既に見えているはず）
   if (res.session) {
    sessions = { ...sessions, [acc]: res.session };
   } else if (res.outcome === 'already_authorized') {
    // 念のため authorized の真偽を最新化
    void refreshSnapshot();
   }
  } catch (e) {
   errors = { ...errors, [acc]: formatError(e) };
  } finally {
   busy = { ...busy, [acc]: false };
  }
 }

 async function cancel(acc: OAuthAccount): Promise<void> {
  if (busy[acc]) return;
  busy = { ...busy, [acc]: true };
  errors = { ...errors, [acc]: null };
  try {
   const res = await api.oauthCancel(acc);
   sessions = { ...sessions, [acc]: res.session };
  } catch (e) {
   errors = { ...errors, [acc]: formatError(e) };
  } finally {
   busy = { ...busy, [acc]: false };
  }
 }

 /**
  * 保存済みトークンファイルを削除する。
  *
  * ユースケース: 「別アカウントで認可し直したい」「挙動が怪しいので完全リセット」など。
  * 破壊的操作なので window.confirm で明示的な確認を取る。
  */
 async function deleteTokens(acc: OAuthAccount): Promise<void> {
  if (busy[acc]) return;
  const ok = window.confirm(
   `${acc} アカウントのトークンキャッシュファイルを削除します。\n\n` +
    `・次回以降、再認可 (Device Code Flow) が必要になります。\n` +
    `・既に起動中の Twitch 接続（eventsub など）はそのまま継続します。\n` +
    `  完全なリセットが必要なら VAC を再起動してください。\n\n` +
    `実行しますか？`,
  );
  if (!ok) return;
  busy = { ...busy, [acc]: true };
  errors = { ...errors, [acc]: null };
  try {
   await api.oauthDeleteTokens(acc);
   // サーバ側で session も除去されるので、こちらも反映。
   sessions = { ...sessions, [acc]: null };
   // snapshot を再取得して authorized バッジを false に倒す。
   await refreshSnapshot();
  } catch (e) {
   errors = { ...errors, [acc]: formatError(e) };
  } finally {
   busy = { ...busy, [acc]: false };
  }
 }

 function formatError(e: unknown): string {
  if (e instanceof ControlApiError) {
   return `${e.status} ${e.statusText}: ${typeof e.body === 'string' ? e.body : JSON.stringify(e.body)}`;
  }
  return e instanceof Error ? e.message : String(e);
 }

 // ---- UI helper ----------------------------------------------------------

 function statusTone(s: OAuthSessionStatus): string {
  switch (s) {
   case 'pending':
    return 'bg-warning-200-800 text-warning-900-100';
   case 'authorized':
    return 'bg-success-200-800 text-success-900-100';
   case 'expired':
   case 'canceled':
    return 'bg-surface-300-700 text-surface-800-200';
   case 'failed':
    return 'bg-error-200-800 text-error-900-100';
  }
 }

 /** expires_at までの残り秒（負なら 0）。tick 依存で 1Hz 更新。 */
 function remainingSecs(view: OAuthSessionView | null): number {
  if (!view) return 0;
  // tick を参照しておくことで reactivity を発火させる
  void tick;
  const ms = new Date(view.expires_at).getTime() - Date.now();
  return Math.max(0, Math.floor(ms / 1000));
 }

 function formatCountdown(secs: number): string {
  const m = Math.floor(secs / 60);
  const s = secs % 60;
  return `${m}:${String(s).padStart(2, '0')}`;
 }

 async function copyText(text: string): Promise<void> {
  try {
   await navigator.clipboard.writeText(text);
  } catch {
   // clipboard API が使えない環境はサイレントに失敗（表示自体はあるので手動コピー可能）
  }
 }

 /**
  * 「保存済みトークンで既に認可済みか」のフラグ。
  * snapshot 由来で、DCF 開始前から判定できるのがポイント。
  *   - broadcaster: `twitch.broadcaster_authorized`（eventsub 設定がなければ常に false）
  *   - moderator:   `twitch.moderator_authorized`（null なら設定なし扱い）
  * さらに「現セッションが authorized 状態である」もここで OR しておくことで、
  * /start → /status → WS event で authorized になった瞬間も即反映される。
  */
 const authorizedMap = $derived<Record<OAuthAccount, boolean>>({
  broadcaster:
   snapshot?.twitch?.broadcaster_authorized === true ||
   sessions.broadcaster?.status === 'authorized',
  moderator:
   snapshot?.twitch?.moderator_authorized === true ||
   sessions.moderator?.status === 'authorized',
 });

 /**
  * 「そのアカウントで認可を扱うための設定が揃っているか」。
  * 揃っていないタブでは DCF 開始・トークン削除などの操作ボタンを無効化する。
  *   - broadcaster: `[twitch.eventsub]` があり enabled=true であること
  *   - moderator:   `[twitch.moderator]` セクション自体があること（`moderator_authorized !== null`）
  */
 const configuredMap = $derived<Record<OAuthAccount, boolean>>({
  broadcaster: snapshot?.twitch?.eventsub_enabled === true,
  moderator:
   snapshot?.twitch != null && snapshot.twitch.moderator_authorized !== null,
 });

 /** 未設定時に表示する理由文（設定済みなら null）。 */
 const currentNotConfiguredReason: string | null = $derived.by(() => {
  if (!snapshot?.twitch) return '[twitch] 設定がありません。conf に Twitch 統合を追加してください。';
  if (tab === 'broadcaster') {
   return snapshot.twitch.eventsub_enabled
    ? null
    : '[twitch.eventsub] が無効または未設定のため、broadcaster 認可は不要です。';
  }
  return snapshot.twitch.moderator_authorized === null
   ? '[twitch.moderator] 設定が無いため、moderator 認可は不要です。'
   : null;
 });

 const currentSession = $derived(sessions[tab]);
 const currentBusy = $derived(busy[tab]);
 const currentError = $derived(errors[tab]);
 const currentAuthorized = $derived(authorizedMap[tab]);
 const currentConfigured = $derived(configuredMap[tab]);
 const isPending = $derived(currentSession?.status === 'pending');
 const remaining = $derived(remainingSecs(currentSession));
</script>

<section class="rounded-lg border border-surface-300-700 bg-surface-100-900 p-4">
 <header class="mb-3 flex items-center justify-between">
  <h3 class="text-sm font-semibold">OAuth (Twitch Device Code Flow)</h3>
  <button
   type="button"
   class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-0.5 text-[10px] hover:bg-surface-200-800"
   onclick={() => void refreshStatus(tab)}
  >
   ↻ 再取得
  </button>
 </header>

 <!-- タブ -->
 <div class="mb-3 flex gap-1 border-b border-surface-300-700">
  {#each ACCOUNTS as acc (acc)}
   <button
    type="button"
    class="rounded-t px-3 py-1 text-xs font-semibold capitalize {tab === acc
     ? 'bg-surface-50-950 text-primary-500'
     : 'opacity-60 hover:opacity-90'}"
    onclick={() => (tab = acc)}
   >
    {acc}
    {#if sessions[acc]}
     <span class="ml-1 inline-block rounded px-1 text-[10px] {statusTone(sessions[acc]!.status)}">
      {sessions[acc]!.status}
     </span>
    {:else if authorizedMap[acc]}
     <span class="ml-1 inline-block rounded bg-success-200-800 px-1 text-[10px] text-success-900-100">
      authorized
     </span>
    {/if}
   </button>
  {/each}
 </div>

 {#if currentError}
  <div class="mb-2 rounded bg-error-200-800 px-2 py-1 text-xs text-error-900-100">
   ERROR: {currentError}
  </div>
 {/if}

 {#if !currentSession}
  <div class="space-y-2 text-xs">
   {#if currentAuthorized}
    <div class="rounded-lg border border-success-500 bg-success-200-800/60 p-3 text-success-900-100">
     <div class="mb-1 font-semibold">✓ 既に認可済み</div>
     <p class="text-[11px] opacity-90">
      <code class="font-mono">{tab}</code> アカウントの有効なアクセストークンが保存されています。
      Device Code Flow は不要です（VAC は起動時に保存済みトークンを読み込みます）。
     </p>
    </div>
   {:else if currentNotConfiguredReason}
    <p class="opacity-70">
     {currentNotConfiguredReason}
    </p>
   {:else}
    <p class="opacity-70">
     {tab} アカウントの OAuth セッションはまだ開始されていません。
    </p>
   {/if}

   <!-- アクションボタン群。broadcaster / moderator で同一レイアウト。 -->
   <div class="flex flex-wrap items-center gap-2">
    <button
     type="button"
     class="rounded bg-primary-500 px-3 py-1 font-semibold text-primary-950 hover:bg-primary-400 disabled:opacity-50"
     onclick={() => void start(tab)}
     disabled={currentBusy || !currentConfigured}
    >
     {currentBusy
      ? '開始中…'
      : currentAuthorized
       ? 'それでも再試行'
       : 'Device Code Flow を開始'}
    </button>
    {#if currentAuthorized}
     <button
      type="button"
      class="rounded border border-error-500 bg-error-500/20 px-3 py-1 font-semibold text-error-900-100 hover:bg-error-500/40 disabled:opacity-50"
      onclick={() => void deleteTokens(tab)}
      disabled={currentBusy || !currentConfigured}
      title="保存済みトークンキャッシュファイルを削除します（再認可が必要になります）"
     >
      ⚠ トークンキャッシュを削除
     </button>
    {/if}
   </div>
   {#if currentAuthorized}
    <p class="text-[10px] opacity-60">
     ※ 削除しても既に起動中の Twitch 接続は継続します。完全なリセットが必要なら VAC を再起動してください。
    </p>
   {/if}
  </div>
 {:else}
  {@const v = currentSession}
  <div class="space-y-3 text-xs">
   <!-- status ヘッダ -->
   <div class="flex flex-wrap items-center gap-2">
    <span class="rounded px-2 py-0.5 font-mono font-semibold uppercase {statusTone(v.status)}">
     {v.status}
    </span>
    <span class="opacity-70">
     started: {new Date(v.started_at).toLocaleTimeString('ja-JP', { hour12: false })}
    </span>
    {#if isPending}
     <span class="font-mono opacity-80">
      残り {formatCountdown(remaining)} / poll {v.interval_secs}s
     </span>
    {/if}
    {#if v.last_error}
     <span class="rounded bg-error-200-800 px-2 py-0.5 text-[10px] text-error-900-100">
      last_error: {v.last_error}
     </span>
    {/if}
   </div>

   <!-- user_code / verification_uri 提示（pending のときに最も重要） -->
   {#if isPending}
    <div class="rounded-lg border border-primary-500 bg-primary-50-950 p-3">
     <div class="mb-2 text-[10px] opacity-70">
      1. 以下の URL をブラウザで開き、2. コードを入力して認証してください。
     </div>

     <!-- user_code -->
     <div class="mb-2 flex items-center gap-2">
      <span class="w-20 shrink-0 opacity-60">user code</span>
      <code class="flex-1 rounded bg-surface-50-950 px-2 py-1 text-center text-2xl font-bold tracking-widest">
       {v.user_code}
      </code>
      <button
       type="button"
       class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 hover:bg-surface-200-800"
       onclick={() => void copyText(v.user_code)}
       title="user_code をクリップボードにコピー"
      >
       Copy
      </button>
     </div>

     <!-- verification_uri -->
     <div class="flex items-center gap-2">
      <span class="w-20 shrink-0 opacity-60">verify URL</span>
      <code
       class="flex-1 truncate rounded bg-surface-50-950 px-2 py-1 font-mono"
       title={v.verification_uri}
      >
       {v.verification_uri}
      </code>
      <a
       href={v.verification_uri}
       target="_blank"
       rel="noopener noreferrer"
       class="rounded bg-primary-500 px-2 py-1 font-semibold text-primary-950 hover:bg-primary-400"
      >
       Open
      </a>
      <button
       type="button"
       class="rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 hover:bg-surface-200-800"
       onclick={() => void copyText(v.verification_uri)}
      >
       Copy
      </button>
     </div>
    </div>
   {/if}

   <!-- アクション -->
   <div class="flex items-center gap-2">
    {#if isPending}
     <button
      type="button"
      class="rounded bg-error-500 px-3 py-1 font-semibold text-error-950 hover:bg-error-400 disabled:opacity-50"
      onclick={() => void cancel(tab)}
      disabled={currentBusy}
     >
      {currentBusy ? '…' : 'Cancel'}
     </button>
    {:else}
     <button
      type="button"
      class="rounded bg-primary-500 px-3 py-1 font-semibold text-primary-950 hover:bg-primary-400 disabled:opacity-50"
      onclick={() => void start(tab)}
      disabled={currentBusy}
     >
      {currentBusy ? '開始中…' : '再度 Device Code Flow を開始'}
     </button>
    {/if}
    <span class="opacity-60">
     account: <code class="font-mono">{tab}</code>
    </span>
   </div>
  </div>
 {/if}
</section>
