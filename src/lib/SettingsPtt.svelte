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

let pttState = $state<PttState>({
    mode: 'open',
    hold_active: false,
    transmitting: true,
    error: null,
    capture_method: 'none',
});
let sources = $state<SourceInfo[]>([]);
let unlisten: UnlistenFn | null = null;
let installing = $state(false);
let installResult = $state<string | null>(null);
let configuring = $state(false);

onMount(async () => {
    pttState = await invoke<PttState>('ptt_get_state');
    sources = await invoke<SourceInfo[]>('list_hardware_inputs').catch(() => []);
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
async function autoDetect() {
    const node = await invoke<string | null>('ptt_detect_wave_xlr');
    if (node) await setInputDevice(node);
}

async function installUdevRule() {
    installing = true;
    installResult = null;
    try {
        installResult = await invoke<string>('ptt_install_udev_rule');
    } catch (e) {
        installResult = String(e);
    } finally {
        installing = false;
    }
}

async function configureShortcuts() {
    configuring = true;
    try {
        await invoke('ptt_configure_shortcuts');
    } catch (e) {
        // Most compositors that support the portal also implement
        // configure_shortcuts; surface the error if not.
        installResult = `Could not open shortcut settings: ${e}`;
    } finally {
        configuring = false;
    }
}
</script>

<header class="flex flex-col gap-1 mb-3">
    <h3 class="text-base font-semibold m-0">Push-to-Talk</h3>
    <p class="text-sm text-base-content/55 m-0 leading-snug">
        {#if pttState.capture_method === 'portal'}
            Shortcuts are managed by your desktop's GlobalShortcuts portal. Configure the actual key combos in your system shortcut settings.
        {:else}
            Bind a tap shortcut to switch between Open mic and PTT, and a hold shortcut to transmit while held.
        {/if}
    </p>
</header>

{#if pttState.error}
    <div class="alert alert-warning alert-soft py-2 mb-3 whitespace-pre-line">
        <div class="flex flex-col gap-2 w-full">
            <span class="text-sm">{pttState.error}</span>
            {#if pttState.capture_method === 'evdev'}
                <div class="flex items-center gap-2 flex-wrap">
                    <button
                        class="btn btn-warning btn-sm gap-1"
                        onclick={installUdevRule}
                        disabled={installing}
                    >
                        {#if installing}
                            <span class="loading loading-spinner loading-xs"></span>
                            Granting...
                        {:else}
                            <Icon name="shield-check" size={12} />
                            Grant input access
                        {/if}
                    </button>
                    <span class="text-xs text-base-content/55">Installs a udev rule via pkexec — youll see a password prompt.</span>
                </div>
            {/if}
        </div>
    </div>
{/if}

{#if installResult}
    <div class="alert alert-info alert-soft py-2 mb-3">
        <span class="text-sm">{installResult}</span>
    </div>
{/if}

{#if pttState.capture_method === 'portal'}
    <section class="flex flex-col gap-2 mb-4">
        <div class="flex items-center gap-2 text-sm">
            <span class="badge badge-success badge-soft px-2 py-0.5 text-xs uppercase tracking-widest">Portal</span>
            <span class="text-base-content/70">Capturing via xdg-desktop-portal GlobalShortcuts</span>
        </div>
        <button
            class="btn btn-soft btn-sm gap-1 w-fit"
            onclick={configureShortcuts}
            disabled={configuring}
        >
            {#if configuring}
                <span class="loading loading-spinner loading-xs"></span>
                Opening...
            {:else}
                <Icon name="settings" size={12} />
                Configure shortcuts in System Settings
            {/if}
        </button>
        <p class="text-xs text-base-content/55 leading-snug">
            Look for <code class="text-xs">mode_toggle</code> and <code class="text-xs">hold</code> under Tideline shortcuts.
        </p>
    </section>
{:else}
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
{/if}

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
</section>
