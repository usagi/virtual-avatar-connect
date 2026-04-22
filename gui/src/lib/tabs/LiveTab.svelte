<script lang="ts">
 /**
  * Live タブ（γ-2a + γ-2c + γ-6a + γ-8c + φ-3e + φ-4）。
  *
  * - 最上段: 大型 Pause / 再開
  * - 中段: 字幕プレビュー（BosPreview: select + URL コピー + iframe）
  * - 中下段 1: **V2 Live Quick-Add (φ-4)** — Flowgraph `dictionary.learn` ノードに
  *   1-shot trigger を発火して配信中に即語録を追加する。`[[control_api.tables]]`
  *   に `quick_add` を指定した Table が対象。
  * - 中下段 2: **Dictionary Editor Pane (φ-3)** — Control API 経由で
  *   `role = "dictionary"` の TSV を GUI から直接編集する。
  * - 中下段 3: V1 widget（`DictionaryQuickAddWidget`）— modify processor 時代の
  *   dead code 寄りだが、環境によっては依然 `modify` を使っていることがあるため
  *   互換のため残す。V2 への完全移行後に χ で削除予定。
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
 import DictionaryEditorPane from '../control/dictionary/DictionaryEditorPane.svelte';
 import DictionaryLiveQuickAdd from '../control/dictionary/DictionaryLiveQuickAdd.svelte';
</script>

<div class="space-y-4">
 <BigPauseButtons />
 <BosPreview />
 <DictionaryLiveQuickAdd />
 <section class="rounded border border-surface-200-800 bg-surface-50-950 p-3">
  <DictionaryEditorPane />
 </section>
 <DictionaryQuickAddWidget />
 <div class="grid grid-cols-1 gap-4 lg:grid-cols-[minmax(0,1fr)_minmax(0,1fr)]">
  <IngressPanel />
  <ObsControlWidget />
 </div>
</div>
