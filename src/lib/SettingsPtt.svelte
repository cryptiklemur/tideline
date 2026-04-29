<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onDestroy, onMount } from 'svelte';
import BindingCapture from './BindingCapture.svelte';
import Icon from './Icon.svelte';
import type { AppConfig, Binding, PttState, SourceInfo } from './types';

interface Props {
    config: AppConfig;
    onConfigUpdate: (next: AppConfig) => void;
}
let { config, onConfigUpdate }: Props = $props();

let pttState = $state<PttState>({ mode: 'open', hold_active: false, transmitting: true, error: null });
let sources = $state<SourceInfo[]>([]);
let waveXlrPresent = $state(false);
let unlisten: UnlistenFn | null = null;

onMount(async () => {
    pttState = await invoke<PttState>('ptt_get_state');
    sources = await invoke<SourceInfo[]>('list_hardware_inputs').catch(() => []);
    waveXlrPresent = await invoke<boolean>('ptt_wave_xlr_present').catch(() => false);
    unlisten = await listen<PttState>('ptt:state', (e) => { pttState = e.payload; });
});

onDestroy(() => { unlisten?.(); });

async function setModeToggle(b: Binding) {
    await invoke('ptt_set_mode_toggle_binding', { binding: b });
    onConfigUpdate({ ...config, ptt: { ...config.ptt, mode_toggle_binding: b } });
}
async function clearModeToggle() {
    await invoke('ptt_set_mode_toggle_binding', { binding: null });
    onConfigUpdate({ ...config, ptt: { ...config.ptt, mode_toggle_binding: null } });
}
async function setHold(b: Binding) {
    await invoke('ptt_set_hold_binding', { binding: b });
    onConfigUpdate({ ...config, ptt: { ...config.ptt, hold_binding: b } });
}
async function clearHold() {
    await invoke('ptt_set_hold_binding', { binding: null });
    onConfigUpdate({ ...config, ptt: { ...config.ptt, hold_binding: null } });
}
async function setInputDevice(name: string) {
    await invoke('ptt_set_input_device', { device: name });
    onConfigUpdate({ ...config, ptt: { ...config.ptt, input_device: name } });
}
async function setTonesEnabled(enabled: boolean) {
    await invoke('ptt_set_tones_enabled', { enabled });
    onConfigUpdate({ ...config, ptt: { ...config.ptt, tones_enabled: enabled } });
}
async function setTonesVolume(volume: number) {
    await invoke('ptt_set_tones_volume', { volume });
    onConfigUpdate({ ...config, ptt: { ...config.ptt, tones_volume: volume } });
}
async function autoDetect() {
    const node = await invoke<string | null>('ptt_detect_wave_xlr');
    if (node) await setInputDevice(node);
}
</script>

<header class="flex flex-col gap-1 mb-3">
    <h3 class="text-base font-semibold m-0">Push-to-Talk</h3>
    <p class="text-sm text-base-content/55 m-0 leading-snug">
        Bind a tap shortcut to switch between Open mic and PTT, and a hold shortcut to transmit while held.
    </p>
</header>

{#if pttState.error}
    <div class="alert alert-warning alert-soft py-2 mb-3 whitespace-pre-line">
        <span class="text-sm">{pttState.error}</span>
    </div>
{/if}

<section class="flex flex-col gap-1 mb-4">
    <BindingCapture
        label="Mode toggle (tap)"
        sublabel="Switches between Open mic and PTT"
        binding={config.ptt.mode_toggle_binding}
        onSet={setModeToggle}
        onClear={clearModeToggle}
    />
    <BindingCapture
        label="PTT hold"
        sublabel="Hold to transmit while in PTT mode"
        binding={config.ptt.hold_binding}
        onSet={setHold}
        onClear={clearHold}
    />
</section>

<section class="flex flex-col gap-2 mb-4">
    <h4 class="text-[10px] font-bold uppercase tracking-widest text-base-content/55 m-0">PTT input device</h4>
    <div class="flex items-center gap-2">
        <select class="select select-sm flex-1" value={config.ptt.input_device} onchange={(e) => setInputDevice((e.target as HTMLSelectElement).value)}>
            <option value="">— Not set —</option>
            {#each sources as s (s.name)}
                <option value={s.name}>{s.description} ({s.name})</option>
            {/each}
        </select>
        <button class="btn btn-soft btn-sm gap-1" onclick={autoDetect} title="Auto-detect Wave XLR">
            <Icon name="cable" size={12} />
            Detect Wave XLR
        </button>
    </div>
    {#if !waveXlrPresent && config.ptt.input_device.toLowerCase().includes('wave')}
        <p class="text-xs text-warning leading-snug">Wave XLR not currently detected on USB — mute will fail until it's plugged back in.</p>
    {/if}
</section>

<section class="flex flex-col gap-2">
    <h4 class="text-[10px] font-bold uppercase tracking-widest text-base-content/55 m-0">PTT tones</h4>
    <label class="flex items-center gap-3 cursor-pointer w-fit">
        <input
            type="checkbox"
            class="toggle toggle-md toggle-primary shrink-0"
            checked={config.ptt.tones_enabled}
            onchange={(e) => setTonesEnabled((e.currentTarget as HTMLInputElement).checked)}
        />
        <span class="text-sm">Play a short cue on press / release</span>
    </label>
    <label class="flex items-center gap-2">
        <span class="text-sm w-20">Volume</span>
        <input type="range" min="0" max="100" value={config.ptt.tones_volume} class="range range-sm range-primary flex-1" oninput={(e) => setTonesVolume(Number((e.target as HTMLInputElement).value))} />
        <span class="text-xs tabular-nums w-10 text-right">{config.ptt.tones_volume}%</span>
    </label>
</section>
