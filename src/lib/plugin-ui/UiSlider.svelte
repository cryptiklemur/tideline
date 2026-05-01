<script lang="ts">
import type { UiEvent, UiNode } from './types';
interface Props {
    node: Extract<UiNode, { kind: 'slider' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
</script>

<label class="flex items-center gap-2">
    {#if node.label}<span class="text-sm w-20">{node.label}</span>{/if}
    <input
        type="range"
        min={node.min}
        max={node.max}
        step={node.step ?? 1}
        value={node.value}
        class="range range-sm range-primary flex-1"
        oninput={(e) =>
            emit({
                surface_id: surfaceId,
                node_id: node.id,
                value: { type: 'number', value: Number((e.target as HTMLInputElement).value) },
            })}
    />
    <span class="text-xs tabular-nums w-10 text-right">{node.value}{node.suffix ?? ''}</span>
</label>
