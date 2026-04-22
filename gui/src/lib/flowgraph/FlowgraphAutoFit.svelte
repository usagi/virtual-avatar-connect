<script lang="ts">
 /**
  * Svelte Flow のキャンバス内に挿入する小さなヘルパ。
  *
  * 責務:
  *   - `useSvelteFlow()` で fitView を取得し、`triggerKey` が変わるたびに
  *     requestAnimationFrame 2 回後に fitView を走らせる。
  *   - これにより「ファイル切替後にノードが描画された直後」のタイミングで
  *     全ノードが画面に収まるように viewport を調整する。
  *
  * 背景:
  *   - `<SvelteFlow fitView />` の既定挙動は初回マウント時のみ fitView を走らせる。
  *   - 我々は `$effect` 経由で nodes を後から差し込むため、初回 fit 時点では
  *     `nodes.length === 0` で fit しても意味のない viewport に落ち着く。
  *   - その後ノードが入ってきても fitView が再実行されないため、最左上ノードが
  *     画面右下の隅に小さく出るだけの状態になっていた（Part E 手動検証で判明）。
  */
 import { useSvelteFlow } from '@xyflow/svelte';

 let { triggerKey }: { triggerKey: string | number } = $props();
 const { fitView } = useSvelteFlow();

 $effect(() => {
  // triggerKey に依存させるためだけの参照
  void triggerKey;
  // 2 フレーム待って node の bounding rect が確定してから fit する
  const raf1 = requestAnimationFrame(() => {
   const raf2 = requestAnimationFrame(() => {
    void fitView({ padding: 0.12, duration: 200 });
   });
   void raf2;
  });
  return () => cancelAnimationFrame(raf1);
 });
</script>
