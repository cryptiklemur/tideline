<script lang="ts">
import type { UiEvent, UiNode } from './types';
interface Props {
    node: Extract<UiNode, { kind: 'input' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
</script>

<label class="flex items-center gap-2">
    {#if node.label}<span class="text-sm w-20">{node.label}</span>{/if}
    <input
        type="text"
        class="input input-sm flex-1"
        placeholder={node.placeholder}
        value={node.value}
        oninput={(e) =>
            emit({
                surface_id: surfaceId,
                node_id: node.id,
                value: { type: 'string', value: (e.target as HTMLInputElement).value },
            })}
    />
</label>
