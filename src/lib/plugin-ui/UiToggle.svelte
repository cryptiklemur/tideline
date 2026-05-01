<script lang="ts">
import type { UiEvent, UiNode } from './types';
interface Props {
    node: Extract<UiNode, { kind: 'toggle' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
</script>

<label class="flex items-center gap-3 cursor-pointer w-fit">
    <input
        type="checkbox"
        class="checkbox checkbox-sm checkbox-primary shrink-0"
        checked={node.value}
        onchange={(e) =>
            emit({
                surface_id: surfaceId,
                node_id: node.id,
                value: { type: 'bool', value: (e.currentTarget as HTMLInputElement).checked },
            })}
    />
    {#if node.label}<span class="text-sm">{node.label}</span>{/if}
</label>
