<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onDestroy, onMount } from 'svelte';
import Icon from '$lib/Icon.svelte';
import PluginPickerModal from './PluginPickerModal.svelte';
import type { UiEvent, UiNode } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    pluginId: string;
    channelUuid: string;
    channelName?: string;
    channelKind?: 'physical_input' | 'input' | 'output';
    onClose: () => void;
}
let { pluginId, channelUuid, channelName, channelKind, onClose }: Props = $props();

let dlg: HTMLDialogElement | undefined = $state();
let tree: UiNode | null = $state(null);
let error: string | null = $state(null);
let loading = $state(true);
let rescanning = $state(false);
let pickerOpen = $state(false);
let busyMessage = $state<{ title: string; sub: string }>({ title: 'Scanning for plugins…', sub: 'Probing LV2, CLAP, VST3, and VST2 directories' });
let surfaceId = $derived(`effects-rack:${channelUuid}`);
let unlistenChange: UnlistenFn | null = null;

async function refresh() {
    try {
        const result = await invoke<UiNode>('tideline_plugin_request', {
            pluginId,
            method: 'effects.render_rack',
            params: { channel_uuid: channelUuid },
        });
        tree = result;
        error = null;
    } catch (e) {
        error = String(e);
    } finally {
        loading = false;
    }
}

async function emit(e: UiEvent) {
    if (e.node_id === 'open_plugin_picker') {
        pickerOpen = true;
        return;
    }
    const isLongOp = e.node_id === 'rescan' || e.node_id === 'auto_bridge_reaplugs';
    if (isLongOp) {
        if (e.node_id === 'auto_bridge_reaplugs') {
            busyMessage = {
                title: 'Bridging ReaPlugs with yabridge…',
                sub: 'Running yabridgectl add + sync, then rescanning',
            };
        } else {
            busyMessage = {
                title: 'Scanning for plugins…',
                sub: 'Probing LV2, CLAP, VST3, and VST2 directories',
            };
        }
        rescanning = true;
    }
    try {
        const result = await invoke<{ open_window?: { plugin_id: string; surface_id: string; title?: string } }>(
            'tideline_plugin_request',
            {
                pluginId,
                method: 'effects.rack_event',
                params: {
                    channel_uuid: channelUuid,
                    node_id: e.node_id,
                    value: e.value && 'value' in e.value ? (e.value as { value: unknown }).value : null,
                },
            },
        );
        if (result?.open_window) {
            await invoke('open_plugin_window', {
                pluginId: result.open_window.plugin_id,
                surfaceId: result.open_window.surface_id,
                title: result.open_window.title,
            });
        }
        await refresh();
    } catch (err) {
        error = String(err);
    } finally {
        if (isLongOp) rescanning = false;
    }
}

onMount(async () => {
    dlg?.showModal();
    await refresh();
    unlistenChange = await listen<{ topic: string; params: { channel_uuid?: string } }>(
        'tideline-plugin:event',
        async (msg) => {
            if (msg.payload?.topic === 'tideline-effects:rack_changed'
                && msg.payload.params?.channel_uuid === channelUuid) {
                await refresh();
            }
        },
    );
});

onDestroy(() => {
    unlistenChange?.();
});

async function persistAndClose() {
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.persist_now',
            params: {},
        });
    } catch (e) {
        // Best-effort — never block close on a save failure.
        console.warn('effects.persist_now failed', e);
    }
    onClose();
}

function onCancel(ev: Event) {
    ev.preventDefault();
    void persistAndClose();
}

function onBackdropClick(ev: MouseEvent) {
    if (ev.target === dlg) void persistAndClose();
}

function onKeydown(ev: KeyboardEvent) {
    if (ev.key === 'Escape') {
        ev.preventDefault();
        void persistAndClose();
    }
}
</script>

<dialog
    bind:this={dlg}
    class="modal"
    oncancel={onCancel}
    onclick={onBackdropClick}
    onkeydown={onKeydown}
>
    <div class="modal-box max-w-3xl w-full p-0 bg-base-200 border border-base-content/15 rounded-xl shadow-2xl flex flex-col max-h-[calc(100dvh-4rem)]">
        <header class="flex items-center gap-3 px-5 py-3 border-b border-base-content/15 shrink-0">
            <span class="inline-flex items-center justify-center size-8 rounded-md bg-primary/15 text-primary shrink-0">
                <Icon name="fx" size={16} />
            </span>
            <div class="flex flex-col min-w-0">
                <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55 leading-none">
                    Effects rack
                </span>
                <h2 class="m-0 text-sm font-semibold leading-tight truncate">
                    {channelName ?? 'Channel'}
                </h2>
            </div>
            <div class="flex-1"></div>
            <kbd class="kbd kbd-xs hidden sm:inline-flex">esc</kbd>
            <button
                type="button"
                class="btn btn-ghost btn-sm btn-square"
                onclick={() => void persistAndClose()}
                aria-label="Close"
            >
                <Icon name="close" size={16} />
            </button>
        </header>

        <div class="px-5 py-4 bg-base-100 flex-1 min-h-[260px] overflow-y-auto">
            {#if error}
                <div class="alert alert-error text-xs items-start gap-2" role="alert">
                    <Icon name="alert" size={14} />
                    <div class="flex flex-col gap-1">
                        <span class="font-semibold">Couldn't load rack</span>
                        <span class="opacity-80">{error}</span>
                    </div>
                </div>
            {:else if rescanning}
                <div class="flex flex-col items-center justify-center gap-4 py-16" role="status" aria-live="polite">
                    <span class="loading loading-spinner loading-lg text-primary" aria-hidden="true"></span>
                    <div class="flex flex-col items-center gap-1">
                        <span class="text-sm font-semibold">{busyMessage.title}</span>
                        <span class="text-xs text-base-content/60">{busyMessage.sub}</span>
                    </div>
                </div>
            {:else if loading && !tree}
                <div class="flex flex-col gap-2" aria-busy="true">
                    <div class="skeleton h-9 w-full"></div>
                    <div class="skeleton h-14 w-full"></div>
                    <div class="skeleton h-14 w-full"></div>
                </div>
            {:else if tree}
                <UiTree node={tree} {surfaceId} {pluginId} {emit} />
            {/if}
        </div>
    </div>
</dialog>

{#if pickerOpen}
    <PluginPickerModal
        {pluginId}
        {channelKind}
        onClose={() => { pickerOpen = false; }}
        onPick={async (uri) => {
            pickerOpen = false;
            await emit({ surface_id: surfaceId, node_id: `add:${uri}`, value: { type: 'click' } });
        }}
    />
{/if}
