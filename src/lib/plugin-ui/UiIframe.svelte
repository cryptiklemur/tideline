<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onDestroy, onMount } from 'svelte';
import type { UiNode } from './types';

interface Props {
    node: Extract<UiNode, { kind: 'iframe' }>;
    pluginId: string;
    surfaceId: string;
}
let { node, pluginId, surfaceId }: Props = $props();
let frame: HTMLIFrameElement | null = $state(null);
let unlisten: UnlistenFn | null = null;
let src = $derived(`tideline-plugin://${pluginId}/${node.src_id}/`);
let h = $derived(`${node.height ?? 320}px`);

onMount(async () => {
    const topic = `tideline-plugin:iframe:${pluginId}:${surfaceId}`;
    unlisten = await listen<unknown>(topic, (e) => {
        frame?.contentWindow?.postMessage({ __tideline: true, payload: e.payload }, '*');
    });
});

onDestroy(() => {
    unlisten?.();
});

function onLoad() {
    void invoke('tideline_plugin_iframe_send', {
        pluginId,
        surfaceId,
        message: { kind: 'host_ready' },
    });
}
</script>

<iframe
    bind:this={frame}
    title="plugin {pluginId} surface {surfaceId}"
    {src}
    sandbox="allow-scripts"
    referrerpolicy="no-referrer"
    style:width="100%"
    style:height={h}
    style:border="0"
    onload={onLoad}
></iframe>
