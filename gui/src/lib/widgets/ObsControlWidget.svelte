<script lang="ts">
 /**
  * Phase VI-γ-6a: OBS Studio 連携ウィジェット（MVP）。
  *
  * - `obs-websocket v5`（OBS 30+ に内蔵）に **ブラウザから直接** WebSocket で繋ぐ。
  *   VAC Backend を経由しないので、OBS が別 PC にあっても LAN 到達できれば動く。
  * - 接続情報は `localStorage` に保存（GUI only）。パスワードは masked 表示。
  * - 提供機能（MVP）:
  *    - シーン一覧の取得 + 現行シーンの表示・切替
  *    - 録画 on/off（RecordStateChanged を購読して状態を反映）
  *    - 配信 on/off（StreamStateChanged を購読）
  *    - Virtual Camera on/off
  *
  * 将来拡張:
  *    - スタジオモード、シーンアイテム操作、プロファイル切替、ソース音量など
  *    - 複数 OBS インスタンスの並列管理（widgets foundation 本体化）
  */
 import { onDestroy } from 'svelte';
 import { ObsWsClient } from './obsWsClient';
 import { toastStore } from '../toasts.svelte';

 const LS_URL = 'vac.obs.url';
 const LS_PASSWORD = 'vac.obs.password';

 type Scene = { sceneName: string; sceneIndex: number };

 let url = $state<string>(localStorage.getItem(LS_URL) ?? 'ws://127.0.0.1:4455');
 let password = $state<string>(localStorage.getItem(LS_PASSWORD) ?? '');
 let showPassword = $state(false);

 let client = $state<ObsWsClient | null>(null);
 let connecting = $state(false);
 let connected = $state(false);
 let error = $state<string | null>(null);

 let obsVersion = $state<string | null>(null);
 let currentScene = $state<string | null>(null);
 let scenes = $state<Scene[]>([]);
 let recording = $state(false);
 let streaming = $state(false);
 let virtualCam = $state(false);

 async function connect() {
  connecting = true;
  error = null;
  // 現接続を切る
  client?.disconnect();
  const c = new ObsWsClient(url.trim(), password || null);
  c.onEvent((type, data) => handleEvent(type, data));
  c.onClose((reason) => {
   connected = false;
   client = null;
   toastStore.warn('OBS との接続が切れました', reason);
  });
  try {
   await c.connect();
   client = c;
   connected = true;
   localStorage.setItem(LS_URL, url.trim());
   localStorage.setItem(LS_PASSWORD, password);
   await refreshAll();
   toastStore.success('OBS に接続しました', obsVersion ?? '');
  } catch (e) {
   error = e instanceof Error ? e.message : String(e);
   c.disconnect();
   toastStore.error('OBS 接続に失敗しました', error);
  } finally {
   connecting = false;
  }
 }

 function disconnect() {
  client?.disconnect();
  client = null;
  connected = false;
  scenes = [];
  currentScene = null;
  recording = false;
  streaming = false;
  virtualCam = false;
  obsVersion = null;
 }

 onDestroy(() => {
  disconnect();
 });

 async function refreshAll() {
  if (!client) return;
  try {
   const ver = (await client.request<{ obsVersion: string }>('GetVersion')) as { obsVersion: string };
   obsVersion = ver.obsVersion;
  } catch {
   /* ignore */
  }
  try {
   const list = (await client.request<{ currentProgramSceneName: string; scenes: Scene[] }>('GetSceneList')) as {
    currentProgramSceneName: string;
    scenes: Scene[];
   };
   currentScene = list.currentProgramSceneName;
   // OBS の GetSceneList は逆順で返すので flip する（= UI 上は上から順に並ぶ）
   scenes = list.scenes.slice().reverse();
  } catch (e) {
   toastStore.error('シーン一覧取得に失敗', e instanceof Error ? e.message : String(e));
  }
  try {
   const rec = (await client.request<{ outputActive: boolean }>('GetRecordStatus')) as { outputActive: boolean };
   recording = rec.outputActive;
  } catch {
   /* ignore */
  }
  try {
   const st = (await client.request<{ outputActive: boolean }>('GetStreamStatus')) as { outputActive: boolean };
   streaming = st.outputActive;
  } catch {
   /* ignore */
  }
  try {
   const vc = (await client.request<{ outputActive: boolean }>('GetVirtualCamStatus')) as { outputActive: boolean };
   virtualCam = vc.outputActive;
  } catch {
   /* ignore */
  }
 }

 function handleEvent(type: string, data: unknown) {
  const d = (data ?? {}) as Record<string, unknown>;
  switch (type) {
   case 'CurrentProgramSceneChanged':
    currentScene = (d.sceneName as string) ?? currentScene;
    break;
   case 'SceneListChanged':
    // 最新の一覧を取り直すのが確実
    void refreshAll();
    break;
   case 'RecordStateChanged':
    // outputState: 'OBS_WEBSOCKET_OUTPUT_STARTED' 等
    recording = Boolean(d.outputActive);
    break;
   case 'StreamStateChanged':
    streaming = Boolean(d.outputActive);
    break;
   case 'VirtualcamStateChanged':
    virtualCam = Boolean(d.outputActive);
    break;
   default:
    break;
  }
 }

 async function switchScene(name: string) {
  if (!client || name === currentScene) return;
  try {
   await client.request('SetCurrentProgramScene', { sceneName: name });
  } catch (e) {
   toastStore.error('シーン切替に失敗', e instanceof Error ? e.message : String(e));
  }
 }

 async function toggleRecord() {
  if (!client) return;
  try {
   await client.request('ToggleRecord');
  } catch (e) {
   toastStore.error('録画トグルに失敗', e instanceof Error ? e.message : String(e));
  }
 }

 async function toggleStream() {
  if (!client) return;
  try {
   await client.request('ToggleStream');
  } catch (e) {
   toastStore.error('配信トグルに失敗', e instanceof Error ? e.message : String(e));
  }
 }

 async function toggleVirtualCam() {
  if (!client) return;
  try {
   await client.request('ToggleVirtualCam');
  } catch (e) {
   toastStore.error('Virtual Cam トグルに失敗', e instanceof Error ? e.message : String(e));
  }
 }

 function onKeyDown(e: KeyboardEvent) {
  if (e.key === 'Enter' && !connected && !connecting) {
   void connect();
  }
 }
</script>

<section class="rounded border border-surface-200-800 bg-surface-50-950">
 <header class="flex items-center justify-between border-b border-surface-200-800 px-4 py-2">
  <div>
   <h3 class="text-sm font-semibold">OBS Studio 連携</h3>
   <p class="text-xs opacity-60">
    {#if connected}
     {obsVersion ? `obs-websocket v5 / OBS ${obsVersion}` : '接続中'}
    {:else}
     obs-websocket v5 に接続（OBS 30+ に内蔵 / ツール→WebSocket サーバー設定）
    {/if}
   </p>
  </div>
  <div class="flex items-center gap-2">
   {#if connected}
    <span class="rounded bg-success-300-700 px-1.5 py-0.5 text-[10px] font-semibold uppercase text-success-900-100">connected</span>
    <button
     type="button"
     class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-100-900"
     onclick={() => void refreshAll()}
    >
     更新
    </button>
    <button
     type="button"
     class="rounded border border-error-500/40 px-2 py-1 text-xs text-error-500 hover:bg-error-500/10"
     onclick={disconnect}
    >
     切断
    </button>
   {:else}
    <span class="rounded bg-surface-300-700 px-1.5 py-0.5 text-[10px] font-semibold uppercase">offline</span>
   {/if}
  </div>
 </header>

 {#if !connected}
  <div class="space-y-2 p-4">
   <label class="block text-xs">
    <span class="opacity-70">WebSocket URL</span>
    <input
     type="text"
     bind:value={url}
     class="mt-0.5 w-full rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
     placeholder="ws://127.0.0.1:4455"
     onkeydown={onKeyDown}
    />
   </label>
   <label class="block text-xs">
    <span class="opacity-70">パスワード（未設定時は空欄）</span>
    <div class="mt-0.5 flex items-center gap-2">
     <input
      type={showPassword ? 'text' : 'password'}
      bind:value={password}
      class="flex-1 rounded border border-surface-300-700 bg-surface-50-950 px-2 py-1 font-mono text-xs"
      onkeydown={onKeyDown}
     />
     <button
      type="button"
      class="rounded border border-surface-300-700 px-2 py-1 text-xs hover:bg-surface-100-900"
      onclick={() => (showPassword = !showPassword)}
     >
      {showPassword ? '隠す' : '表示'}
     </button>
    </div>
   </label>
   {#if error}
    <p class="rounded border border-error-500/40 bg-error-500/10 p-2 text-xs text-error-900-100">{error}</p>
   {/if}
   <div class="flex justify-end">
    <button
     type="button"
     class="rounded bg-primary-500 px-3 py-1.5 text-xs font-semibold text-white hover:bg-primary-600 disabled:opacity-50"
     onclick={() => void connect()}
     disabled={connecting || !url.trim()}
    >
     {connecting ? '接続中…' : '接続'}
    </button>
   </div>
  </div>
 {:else}
  <div class="space-y-3 p-4">
   <div>
    <div class="mb-1 flex items-center justify-between">
     <span class="text-xs opacity-70">シーン</span>
     {#if currentScene}
      <span class="text-xs opacity-60">現行: {currentScene}</span>
     {/if}
    </div>
    <div class="grid grid-cols-2 gap-1 sm:grid-cols-3">
     {#each scenes as scene (scene.sceneName)}
      {@const active = scene.sceneName === currentScene}
      <button
       type="button"
       class="rounded border px-2 py-1 text-xs {active
        ? 'border-primary-500 bg-primary-500 text-white'
        : 'border-surface-300-700 hover:bg-surface-100-900'}"
       onclick={() => void switchScene(scene.sceneName)}
      >
       {scene.sceneName}
      </button>
     {/each}
    </div>
   </div>
   <div class="flex flex-wrap gap-2">
    <button
     type="button"
     class="rounded px-3 py-1.5 text-xs font-semibold {recording
      ? 'bg-error-500 text-white hover:bg-error-600'
      : 'border border-surface-300-700 hover:bg-surface-100-900'}"
     onclick={() => void toggleRecord()}
    >
     {recording ? '録画停止' : '録画開始'}
    </button>
    <button
     type="button"
     class="rounded px-3 py-1.5 text-xs font-semibold {streaming
      ? 'bg-error-500 text-white hover:bg-error-600'
      : 'border border-surface-300-700 hover:bg-surface-100-900'}"
     onclick={() => void toggleStream()}
    >
     {streaming ? '配信停止' : '配信開始'}
    </button>
    <button
     type="button"
     class="rounded px-3 py-1.5 text-xs font-semibold {virtualCam
      ? 'bg-primary-500 text-white hover:bg-primary-600'
      : 'border border-surface-300-700 hover:bg-surface-100-900'}"
     onclick={() => void toggleVirtualCam()}
    >
     Virtual Cam {virtualCam ? '停止' : '開始'}
    </button>
   </div>
  </div>
 {/if}
</section>
