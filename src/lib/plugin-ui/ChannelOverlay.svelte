<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onDestroy, onMount } from 'svelte';
import type { ChannelOverlayContribution, UiEvent, UiNode } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    overlay: ChannelOverlayContribution;
    channelUuid?: string;
    emit: (e: UiEvent) => void;
}
let { overlay, channelUuid, emit }: Props = $props();

let dynamicTree: UiNode | null = $state(null);
let unlistenFn: UnlistenFn | null = null;

async function refresh() {
    if (!channelUuid) return;
    try {
        const result = await invoke<UiNode>('tideline_plugin_request', {
            pluginId: overlay.plugin_id,
            method: 'channel_overlay.render',
            params: { surface_id: overlay.surface_id, channel_uuid: channelUuid },
        });
        if (result) dynamicTree = result;
    } catch {
        // plugin doesnt support per-channel render or call failed; keep static tree
    }
}

onMount(async () => {
    await refresh();
    unlistenFn = await listen<{ topic: string; params?: { channel_uuid?: string } }>(
        'tideline-plugin:event',
        async (msg) => {
            const topic = msg.payload?.topic ?? '';
            if (!topic.startsWith(overlay.plugin_id + ':')) return;
            const cu = msg.payload?.params?.channel_uuid;
            if (cu && cu !== channelUuid) return;
            await refresh();
        },
    );
});

onDestroy(() => {
    unlistenFn?.();
});

let renderTree = $derived(dynamicTree ?? overlay.tree);
</script>

<div
    class="flex flex-col gap-1"
    data-overlay-placement={overlay.placement}
    data-plugin-id={overlay.plugin_id}
    data-surface-id={overlay.surface_id}
>
    <UiTree node={renderTree} surfaceId={overlay.surface_id} pluginId={overlay.plugin_id} {emit} />
</div>
