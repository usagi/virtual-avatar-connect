<script lang="ts">
 /**
  * Live タブ（γ-2a + γ-2c + γ-6a + γ-8c）。
  *
  * - 最上段: 大型 Pause / 再開
  * - 中段: 字幕プレビュー（BosPreview: select + URL コピー + iframe）
  * - 中下段: クイック辞書追加（配信中の誤読即対応）
  * - 下段: Ingress (テスト発話 投入) + OBS 連携ウィジェット
  *
  * Widget foundation（`widgets/`）は γ-6a で導入。今後ここに connected
  * ウィジェットを並べていく。
  */
 import BigPauseButtons from '../BigPauseButtons.svelte';
 import BosPreview from '../BosPreview.svelte';
 import IngressPanel from '../IngressPanel.svelte';
 import DictionaryQuickAddWidget from '../widgets/DictionaryQuickAddWidget.svelte';
 import ObsControlWidget from '../widgets/ObsControlWidget.svelte';
</script>

<div class="space-y-4">
 <BigPauseButtons />
 <BosPreview />
 <DictionaryQuickAddWidget />
 <div class="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
  <IngressPanel />
  <ObsControlWidget />
 </div>
</div>
