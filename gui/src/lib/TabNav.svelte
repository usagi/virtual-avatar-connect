<script lang="ts">
 /**
  * タブナビゲーションの表示。現在タブは URL hash (`#now`, `#flowgraph`, ...) で保持し、
  * ブラウザ戻る/進むや deep-link に対応する。
  *
  * GUI redesign では desktop を左レール、mobile を横スクロールにする。
  */
 import { tabNavStore, type TabId, TABS } from './tabs.svelte';

 function onClick(id: TabId) {
  tabNavStore.setActive(id);
 }
</script>

<nav
 class="flex items-center gap-1 overflow-x-auto px-4 py-2 lg:flex-col lg:items-stretch lg:overflow-visible lg:px-3 lg:py-4"
 aria-label="Main tabs"
>
 {#each TABS as tab (tab.id)}
  {@const active = tabNavStore.active === tab.id}
  <button
   type="button"
   class="relative rounded px-4 py-2.5 text-left text-sm transition-colors lg:w-full"
   class:font-semibold={active}
   class:text-primary-500={active}
   class:text-surface-700-300={!active}
   class:bg-surface-50-950={active}
   class:hover:text-surface-950-50={!active}
   class:hover:bg-surface-50-950={!active}
   aria-current={active ? 'page' : undefined}
   onclick={() => onClick(tab.id)}
  >
   <span class="flex items-center gap-2">
    <span aria-hidden="true">{tab.icon}</span>
    <span>{tab.label}</span>
   </span>
   {#if active}
    <span class="absolute inset-x-2 bottom-0 h-0.5 rounded-full bg-primary-500 lg:inset-x-0 lg:inset-y-2 lg:h-auto lg:w-0.5"></span>
   {/if}
  </button>
 {/each}
</nav>
