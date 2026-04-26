<script lang="ts">
 /**
  * タブナビゲーションの表示。現在タブは URL hash (`#now`, `#flowgraph`, ...) で保持し、
  * ブラウザ戻る/進むや deep-link に対応する。
  *
  * GUI redesign 初期段階では水平タブを維持し、情報設計だけ先に v2 へ寄せる。
  */
 import { tabNavStore, type TabId, TABS } from './tabs.svelte';

 function onClick(id: TabId) {
  tabNavStore.setActive(id);
 }
</script>

<nav
 class="flex items-center gap-1 border-b border-surface-200-800 px-6"
 aria-label="Main tabs"
>
 {#each TABS as tab (tab.id)}
  {@const active = tabNavStore.active === tab.id}
  <button
   type="button"
   class="relative px-4 py-2.5 text-sm transition-colors"
   class:font-semibold={active}
   class:text-primary-500={active}
   class:text-surface-700-300={!active}
   class:hover:text-surface-950-50={!active}
   aria-current={active ? 'page' : undefined}
   onclick={() => onClick(tab.id)}
  >
   <span class="flex items-center gap-2">
    <span aria-hidden="true">{tab.icon}</span>
    <span>{tab.label}</span>
   </span>
   {#if active}
    <span class="absolute inset-x-2 -bottom-px h-0.5 rounded-full bg-primary-500"></span>
   {/if}
  </button>
 {/each}
</nav>
