<script lang="ts">
import type { UiEvent, UiNode } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    node: Extract<UiNode, { kind: 'tabs' }>;
    surfaceId: string;
    pluginId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, pluginId, emit }: Props = $props();

let activeId = $state('');

$effect(() => {
    const incoming = node.active_tab_id ?? node.tabs[0]?.id ?? '';
    if (incoming && incoming !== activeId) activeId = incoming;
});

function selectTab(id: string) {
    activeId = id;
    emit({ surface_id: surfaceId, node_id: node.id, value: { type: 'string', value: id } });
}

let activeTab = $derived(node.tabs.find((t) => t.id === activeId) ?? node.tabs[0]);
</script>

<div class="flex flex-col gap-2">
    <div role="tablist" class="flex gap-1 border-b border-base-content/15">
        {#each node.tabs as t (t.id)}
            {@const isActive = t.id === activeId}
            <button
                type="button"
                role="tab"
                aria-selected={isActive}
                class="px-3 py-1.5 text-sm font-semibold tracking-wide cursor-pointer bg-transparent border-0 border-b-2 transition-colors -mb-px
                       {isActive
                         ? 'border-primary text-base-content'
                         : 'border-transparent text-base-content/55 hover:text-base-content'}"
                onclick={() => selectTab(t.id)}
            >
                {t.label}
            </button>
        {/each}
    </div>

    {#if activeTab}
        <div role="tabpanel">
            <UiTree node={activeTab.content} {surfaceId} {pluginId} {emit} />
        </div>
    {/if}
</div>
