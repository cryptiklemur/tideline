<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onDestroy, onMount } from 'svelte';
import Icon from './Icon.svelte';
import type { PttState } from './types';

let state = $state<PttState>({ mode: 'open', hold_active: false, transmitting: true, error: null });
let unlisten: UnlistenFn | null = null;

onMount(async () => {
    try { state = await invoke<PttState>('ptt_get_state'); } catch (_) {}
    unlisten = await listen<PttState>('ptt:state', (e) => { state = e.payload; });
});

onDestroy(() => { unlisten?.(); });

async function toggle() {
    try { await invoke('ptt_toggle_mode'); } catch (_) {}
}

let label = $derived(
    state.mode === 'open'
        ? 'Open mic'
        : state.transmitting ? 'PTT — live' : 'PTT — muted'
);
let cls = $derived(
    state.error
        ? 'border-warning bg-warning/15 text-warning'
        : state.transmitting
            ? 'border-primary bg-primary/15 text-primary'
            : 'border-error bg-error/15 text-error'
);
</script>

<button
    type="button"
    class="inline-flex items-center gap-1.5 px-2.5 py-1 pl-2 rounded-full border text-[12px] font-bold tracking-widest uppercase cursor-pointer transition-colors {cls}"
    onclick={toggle}
    title={state.error ?? 'Click to toggle mode'}
>
    <span class="w-2 h-2 rounded-full flex-shrink-0 transition-all {state.transmitting ? 'bg-current shadow-[0_0_6px_currentColor] animate-pulse' : 'bg-current/45'}"></span>
    <span>{label}</span>
    {#if state.error}
        <Icon name="info" size={10} />
    {/if}
</button>
