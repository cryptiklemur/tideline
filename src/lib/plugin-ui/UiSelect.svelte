<script lang="ts">
import type { UiEvent, UiNode } from './types';
interface Props {
    node: Extract<UiNode, { kind: 'select' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
</script>

<label class="flex items-center gap-2">
    {#if node.label}<span class="text-sm w-20">{node.label}</span>{/if}
    <select
        class="select select-sm flex-1"
        value={node.value}
        onchange={(e) =>
            emit({
                surface_id: surfaceId,
                node_id: node.id,
                value: { type: 'string', value: (e.target as HTMLSelectElement).value },
            })}
    >
        {#each node.options as opt (opt.value)}
            <option value={opt.value}>{opt.label}</option>
        {/each}
    </select>
</label>
