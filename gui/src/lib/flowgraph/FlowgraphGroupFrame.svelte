<script lang="ts">
 type Data = {
  groupId: string;
  label: string;
  nodeCount: number;
  color: string | null;
  width: number;
  height: number;
 };

 let { data }: { data: Data } = $props();
</script>

<div
 class="group-frame"
 style={`--group-color: ${data.color ?? '#38bdf8'}; width: ${data.width}px; height: ${data.height}px;`}
 aria-label={`Flowgraph group ${data.groupId}`}
 data-testid={`flowgraph-group-${data.groupId}`}
>
 <div class="group-frame-label">
  <span>{data.label}</span>
  <small>{data.nodeCount} nodes</small>
 </div>
</div>

<style>
 .group-frame {
  position: relative;
  pointer-events: none;
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
