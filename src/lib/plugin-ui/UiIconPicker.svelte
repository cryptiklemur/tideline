<script lang="ts">
import Icon, { ICONS, type IconName } from '$lib/Icon.svelte';
import type { UiEvent, UiNode } from './types';
interface Props {
    node: Extract<UiNode, { kind: 'icon_picker' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();
function pick(name: string) {
    emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'string', value: name } });
}
</script>

<div class="flex flex-col gap-2">
    {#if node.label}<span class="text-sm">{node.label}</span>{/if}
    <div class="grid grid-cols-8 gap-1">
        {#each node.choices as name (name)}
            <button
                class="btn btn-square btn-xs {node.value === name ? 'btn-primary' : 'btn-ghost'}"
                onclick={() => pick(name)}
                aria-label={name}
            >
                {#if name in ICONS}
                    <Icon name={name as IconName} size={12} />
                {:else}
                    <span class="text-[8px]">{name}</span>
                {/if}
            </button>
        {/each}
    </div>
</div>
