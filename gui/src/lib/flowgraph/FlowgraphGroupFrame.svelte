<script lang="ts">
 type Data = {
  groupId: string;
  label: string;
  nodeCount: number;
  color: string | null;
  selected: boolean;
  width: number;
  height: number;
 };

 let { data }: { data: Data } = $props();

 function onKeydown(ev: KeyboardEvent) {
  if (ev.key !== 'Enter' && ev.key !== ' ') return;
  ev.preventDefault();
  (ev.currentTarget as HTMLElement | null)?.click();
 }
</script>

<div
 class="group-frame"
 class:selected={data.selected}
 style={`--group-color: ${data.color ?? '#38bdf8'}; width: ${data.width}px; height: ${data.height}px;`}
 role="button"
 tabindex="0"
 aria-label={`Flowgraph group ${data.groupId}`}
 data-testid={`flowgraph-group-${data.groupId}`}
 onkeydown={onKeydown}
>
 <div class="group-frame-label">
  <span>{data.label}</span>
  <small>{data.nodeCount} nodes</small>
 </div>
</div>

<style>
 .group-frame {
  position: relative;
  pointer-events: auto;
  cursor: pointer;
  border: 1px solid color-mix(in srgb, var(--group-color, #38bdf8) 42%, transparent);
  border-radius: 8px;
  background:
   linear-gradient(
    135deg,
    color-mix(in srgb, var(--group-color, #38bdf8) 9%, transparent),
    color-mix(in srgb, var(--group-color, #38bdf8) 3%, transparent)
   );
  box-shadow:
   inset 0 0 0 1px rgba(255, 255, 255, 0.28),
   0 12px 32px color-mix(in srgb, var(--group-color, #38bdf8) 10%, transparent);
 }

 .group-frame.selected {
  border-color: color-mix(in srgb, var(--group-color, #38bdf8) 72%, white);
  background:
   linear-gradient(
    135deg,
    color-mix(in srgb, var(--group-color, #38bdf8) 14%, transparent),
    color-mix(in srgb, var(--group-color, #38bdf8) 6%, transparent)
   );
  box-shadow:
   inset 0 0 0 1px color-mix(in srgb, var(--group-color, #38bdf8) 38%, transparent),
   0 0 0 1px color-mix(in srgb, var(--group-color, #38bdf8) 24%, transparent),
   0 14px 36px color-mix(in srgb, var(--group-color, #38bdf8) 14%, transparent);
 }

 .group-frame-label {
  position: absolute;
  left: 10px;
  top: -22px;
  display: flex;
  max-width: calc(100% - 20px);
  align-items: center;
  gap: 6px;
  color: color-mix(in srgb, var(--group-color, #38bdf8) 78%, currentColor);
  font-size: 10px;
  font-weight: 700;
  letter-spacing: 0;
  text-shadow: 0 1px 2px rgba(0, 0, 0, 0.18);
 }

 .group-frame-label span {
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
  white-space: nowrap;
 }

 .group-frame-label small {
  flex: none;
  font-size: 9px;
  font-weight: 600;
  opacity: 0.62;
 }
</style>
