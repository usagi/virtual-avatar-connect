<script lang="ts">
 /**
  * Runtime settings and developer utilities.
  *
  * 永続 profile/conf 管理、reload、低レベル診断は Resources から分離する。
  * 日常運用では Resources を状態確認に寄せ、Settings は durable な調整に寄せる。
  */
 import PingStatus from '../PingStatus.svelte';
 import ReloadPanel from '../ReloadPanel.svelte';
 import WidgetSlot from '../WidgetSlot.svelte';
 import { VAC_THEMES, vacThemeStore, type VacThemeId } from '../theme.svelte';
 import ProfileManagerPanel from './ProfileManagerPanel.svelte';

 function setTheme(themeId: VacThemeId) {
  vacThemeStore.set(themeId);
 }
</script>

<div class="grid grid-cols-1 gap-4 xl:grid-cols-[minmax(0,1.2fr)_minmax(0,0.8fr)]">
 <div class="space-y-4">
  <ProfileManagerPanel />
  <ReloadPanel />
 </div>
 <div class="space-y-4">
  <section class="vac-panel p-4">
   <div class="mb-3">
    <h2 class="text-sm font-semibold opacity-80">テーマ</h2>
    <p class="mt-1 text-xs opacity-60">
     見た目だけを切り替えます。情報設計と操作語彙は共通です。
    </p>
   </div>
   <div class="grid gap-2">
    {#each VAC_THEMES as theme (theme.id)}
     {@const active = vacThemeStore.current === theme.id}
     <button
      type="button"
      class="vac-theme-choice rounded border p-3 text-left transition-colors"
      class:is-active={active}
      aria-pressed={active}
      onclick={() => setTheme(theme.id)}
     >
      <span class="flex items-center justify-between gap-3">
       <span class="min-w-0">
        <span class="block text-sm font-semibold">{theme.label}</span>
        <span class="block text-xs opacity-65">{theme.tone}</span>
       </span>
       <span class="vac-theme-swatch" data-theme-swatch={theme.id} aria-hidden="true"></span>
      </span>
      <span class="mt-2 block text-xs opacity-70">{theme.description}</span>
     </button>
    {/each}
   </div>
  </section>
  <section class="rounded-lg border border-surface-200-800 bg-surface-100-900 p-4">
   <h2 class="mb-2 text-sm font-semibold opacity-80">接続情報</h2>
   <PingStatus />
  </section>
  <WidgetSlot
   title="開発者向けユーティリティ"
   subtitle="γ-6 予定"
   placeholder="ログダウンロード、ランタイム診断、ニッチな操作（broadcasters の force_remove 等）を整理します。"
  />
 </div>
</div>
