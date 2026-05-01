<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import Icon, { type IconName } from './Icon.svelte';
import KeybindRow from './KeybindRow.svelte';
import Modal from './Modal.svelte';
import SettingsPtt from './SettingsPtt.svelte';
import PluginSettingsSection from './plugin-ui/PluginSettingsSection.svelte';
import UiIcon from './plugin-ui/UiIcon.svelte';
import { pluginUi } from './plugin-ui/pluginUi.svelte';
import type { AppConfig, AudioBackendStatus, KeybindAction, SinkInfo } from './types';

interface Props {
    open: boolean;
    config: AppConfig;
    outputs: SinkInfo[];
    onSetKeybind: (accelerator: string, action: KeybindAction) => Promise<void>;
    onClearKeybind: (accelerator: string) => Promise<void>;
    onConfigUpdate: (next: AppConfig) => void;
    onClose: () => void;
}

let { open = $bindable(), config, outputs, onSetKeybind, onClearKeybind, onConfigUpdate, onClose }: Props = $props();

type BuiltinSectionId = 'general' | 'appearance' | 'keybinds' | 'ptt' | 'devices';
type ActiveSection =
    | { kind: BuiltinSectionId }
    | { kind: 'plugin'; plugin_id: string; surface_id: string };
const SECTIONS: { id: BuiltinSectionId; label: string; icon: IconName; hint: string }[] = [
    { id: 'general', label: 'General', icon: 'info', hint: 'Overview of your routing setup.' },
    { id: 'appearance', label: 'Appearance', icon: 'palette', hint: 'Theme and visual preferences.' },
    { id: 'keybinds', label: 'Keybinds', icon: 'keyboard', hint: 'Global mute shortcuts.' },
    { id: 'ptt', label: 'Push-to-Talk', icon: 'volume-mute', hint: 'Mode toggle and hold-to-talk.' },
    { id: 'devices', label: 'Devices', icon: 'cable', hint: 'Detected PipeWire sinks.' },
];

let active = $state<ActiveSection>({ kind: 'general' });

type KeybindCategory = 'outputs' | 'channels' | 'mixes';
let kbCategory = $state<KeybindCategory>('outputs');

let sinks = $state<SinkInfo[]>([]);
let sinksError = $state('');

let backend = $state<AudioBackendStatus | null>(null);
let backendChecking = $state(false);
let backendError = $state('');

async function refreshBackend() {
    backendChecking = true;
    backendError = '';
    try {
        backend = await invoke<AudioBackendStatus>('ensure_audio_backend_cmd');
    } catch (e) {
        backendError = String(e);
    } finally {
        backendChecking = false;
    }
}

function bindingFor(action: KeybindAction): string | null {
    for (const [accel, a] of Object.entries(config.keybinds ?? {})) {
        if (a.type !== action.type) continue;
        if (a.type === 'toggle_output_mute' && action.type === 'toggle_output_mute' && a.sink === action.sink) return accel;
        if (a.type === 'toggle_channel_mute' && action.type === 'toggle_channel_mute' && a.channel === action.channel) return accel;
        if (a.type === 'toggle_mix_enabled' && action.type === 'toggle_mix_enabled' && a.mix_id === action.mix_id) return accel;
    }
    return null;
}

function isAccelTaken(accel: string): boolean {
    return Object.keys(config.keybinds ?? {}).includes(accel);
}

type ThemeChoice = 'auto' | 'tideline-light' | 'tideline-dark';
let theme = $state<ThemeChoice>('auto');

function loadTheme(): ThemeChoice {
    try {
        const t = localStorage.getItem('tideline-theme');
        if (t === 'tideline-dark' || t === 'tideline-light') return t;
    } catch (_) {}
    return 'auto';
}

function applyTheme(choice: ThemeChoice) {
    if (choice === 'auto') {
        document.documentElement.removeAttribute('data-theme');
        try { localStorage.removeItem('tideline-theme'); } catch (_) {}
    } else {
        document.documentElement.setAttribute('data-theme', choice);
        try { localStorage.setItem('tideline-theme', choice); } catch (_) {}
    }
}

$effect(() => {
    if (open) {
        theme = loadTheme();
        invoke<SinkInfo[]>('list_sinks')
            .then(s => { sinks = s; sinksError = ''; })
            .catch(e => { sinksError = String(e); });
        refreshBackend();
    }
});

function selectTheme(t: ThemeChoice) {
    theme = t;
    applyTheme(t);
}

let mixCount = $derived(config.mixes.length);
let channelCount = $derived(config.channels.length);
let inputCount = $derived(config.channels.filter(c => c.kind === 'input' || c.kind === 'physical_input').length);
let outputCount = $derived(config.channels.filter(c => (c.kind ?? 'output') === 'output').length);
</script>

{#if open}
<Modal label="Settings" maxWidth="760px" onClose={onClose}>
    <div class="flex items-center justify-between px-4 py-3 border-b border-base-content/10">
        <h2 class="text-lg font-semibold m-0">Settings</h2>
        <button class="btn btn-ghost btn-square btn-sm" onclick={onClose} aria-label="Close" data-modal-close>
            <Icon name="close" size={14} />
        </button>
    </div>

    <div class="flex min-h-[460px]">
        <nav class="flex flex-col gap-0.5 w-48 shrink-0 px-2 py-3 border-r border-base-content/10 bg-base-300/40" aria-label="Settings sections">
            {#each SECTIONS as s (s.id)}
                {@const isActive = active.kind === s.id}
                <button
                    type="button"
                    class="flex items-center gap-2.5 px-2.5 py-2 rounded-md text-left transition-colors cursor-pointer
                           {isActive ? 'bg-primary/15 text-primary font-semibold' : 'text-base-content/70 hover:bg-base-content/5 hover:text-base-content'}"
                    onclick={() => active = { kind: s.id }}
                    aria-pressed={isActive}
                >
                    <Icon name={s.icon} size={14} />
                    <span class="text-sm">{s.label}</span>
                </button>
            {/each}
            {#each pluginUi.contributions.settings_sections.slice().sort((a, b) => b.priority - a.priority || a.plugin_id.localeCompare(b.plugin_id)) as section (section.plugin_id + ':' + section.surface_id)}
                {@const isActive = active.kind === 'plugin' && active.plugin_id === section.plugin_id && active.surface_id === section.surface_id}
                <button
                    type="button"
                    class="flex items-center gap-2.5 px-2.5 py-2 rounded-md text-left transition-colors cursor-pointer
                           {isActive ? 'bg-primary/15 text-primary font-semibold' : 'text-base-content/70 hover:bg-base-content/5 hover:text-base-content'}"
                    onclick={() => active = { kind: 'plugin', plugin_id: section.plugin_id, surface_id: section.surface_id }}
                    aria-pressed={isActive}
                >
                    {#if section.icon}
                        <UiIcon node={{ kind: 'icon', id: 'nav-' + section.surface_id, icon: section.icon, size: 14 }} />
                    {/if}
                    <span class="text-sm">{section.title}</span>
                </button>
            {/each}
        </nav>

        <section class="flex-1 min-w-0 px-5 py-4 overflow-y-auto" aria-live="polite">
            {#if active.kind === 'general'}
                <header class="flex flex-col gap-1 mb-4">
                    <h3 class="text-base font-semibold m-0">General</h3>
                    <p class="text-sm text-base-content/55 m-0 leading-snug">Overview of your routing setup. Manage mixes from the sidebar.</p>
                </header>
                <div class="grid grid-cols-2 gap-2.5">
                    <div class="rounded-md border border-base-content/10 bg-base-100 px-3 py-2.5 flex flex-col gap-0.5">
                        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/45">Mixes</span>
                        <span class="text-2xl font-semibold tabular-nums">{mixCount}</span>
                    </div>
                    <div class="rounded-md border border-base-content/10 bg-base-100 px-3 py-2.5 flex flex-col gap-0.5">
                        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/45">Channels</span>
                        <span class="text-2xl font-semibold tabular-nums">{channelCount}</span>
                    </div>
                    <div class="rounded-md border border-base-content/10 bg-base-100 px-3 py-2.5 flex flex-col gap-0.5">
                        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/45">Outputs</span>
                        <span class="text-2xl font-semibold tabular-nums">{outputCount}</span>
                    </div>
                    <div class="rounded-md border border-base-content/10 bg-base-100 px-3 py-2.5 flex flex-col gap-0.5">
                        <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/45">Inputs</span>
                        <span class="text-2xl font-semibold tabular-nums">{inputCount}</span>
                    </div>
                </div>
                <p class="text-xs text-base-content/45 m-0 leading-snug mt-4">Each mix routes its assigned output channels to one or more sinks.</p>
            {:else if active.kind === 'appearance'}
                <header class="flex flex-col gap-1 mb-4">
                    <h3 class="text-base font-semibold m-0">Appearance</h3>
                    <p class="text-sm text-base-content/55 m-0 leading-snug">Pick a theme. Automatic follows your system preference.</p>
                </header>
                <div class="join w-full">
                    <button
                        class="btn join-item flex-1 {theme === 'auto' ? 'btn-primary' : 'btn-soft'}"
                        onclick={() => selectTheme('auto')}
                        aria-pressed={theme === 'auto'}
                    >Automatic</button>
                    <button
                        class="btn join-item flex-1 {theme === 'tideline-light' ? 'btn-primary' : 'btn-soft'}"
                        onclick={() => selectTheme('tideline-light')}
                        aria-pressed={theme === 'tideline-light'}
                    >Light</button>
                    <button
                        class="btn join-item flex-1 {theme === 'tideline-dark' ? 'btn-primary' : 'btn-soft'}"
                        onclick={() => selectTheme('tideline-dark')}
                        aria-pressed={theme === 'tideline-dark'}
                    >Dark</button>
                </div>
            {:else if active.kind === 'keybinds'}
                <header class="flex flex-col gap-1 mb-3">
                    <h3 class="text-base font-semibold m-0">Keybinds</h3>
                    <p class="text-sm text-base-content/55 m-0 leading-snug">Global shortcuts. Press a key combination after clicking Bind. Esc cancels.</p>
                </header>

                {#if outputs.length === 0 && config.channels.length === 0 && config.mixes.length === 0}
                    <p class="text-sm text-base-content/45 m-0 leading-snug">Nothing to bind yet. Add channels, mixes, or detect outputs first.</p>
                {:else}
                    <div role="tablist" class="tabs tabs-box bg-base-300/40 mb-3 inline-flex" aria-label="Keybind category">
                        <button
                            role="tab"
                            class="tab gap-1.5 {kbCategory === 'outputs' ? 'tab-active' : ''}"
                            aria-selected={kbCategory === 'outputs'}
                            onclick={() => kbCategory = 'outputs'}
                        >
                            <Icon name="speaker" size={12} />
                            Outputs
                            <span class="text-[10px] tabular-nums opacity-60">{outputs.length}</span>
                        </button>
                        <button
                            role="tab"
                            class="tab gap-1.5 {kbCategory === 'channels' ? 'tab-active' : ''}"
                            aria-selected={kbCategory === 'channels'}
                            onclick={() => kbCategory = 'channels'}
                        >
                            <Icon name="audio-lines" size={12} />
                            Channels
                            <span class="text-[10px] tabular-nums opacity-60">{config.channels.length}</span>
                        </button>
                        <button
                            role="tab"
                            class="tab gap-1.5 {kbCategory === 'mixes' ? 'tab-active' : ''}"
                            aria-selected={kbCategory === 'mixes'}
                            onclick={() => kbCategory = 'mixes'}
                        >
                            <Icon name="mixer" size={12} />
                            Mixes
                            <span class="text-[10px] tabular-nums opacity-60">{config.mixes.length}</span>
                        </button>
                    </div>

                    {#if kbCategory === 'outputs'}
                        <div class="flex items-center gap-2 px-2.5 mb-2 text-xs text-base-content/55">
                            <Icon name="volume-mute" size={12} />
                            <span>Mute / unmute output sinks. Useful for instantly silencing speakers.</span>
                        </div>
                        {#if outputs.length === 0}
                            <p class="text-sm text-base-content/45 m-0 leading-snug px-2.5">No outputs detected. Connect a sink to bind shortcuts.</p>
                        {:else}
                            <div class="flex flex-col">
                                {#each outputs as o (o.name)}
                                    <KeybindRow
                                        label={o.description}
                                        sublabel={o.name}
                                        action={{ type: 'toggle_output_mute', sink: o.name }}
                                        bound={bindingFor({ type: 'toggle_output_mute', sink: o.name })}
                                        onSet={onSetKeybind}
                                        onClear={onClearKeybind}
                                        isConflict={isAccelTaken}
                                    />
                                {/each}
                            </div>
                        {/if}
                    {:else if kbCategory === 'channels'}
                        <div class="flex items-center gap-2 px-2.5 mb-2 text-xs text-base-content/55">
                            <Icon name="volume-mute" size={12} />
                            <span>Mute / unmute audio channels. Silences any apps routed through them.</span>
                        </div>
                        {#if config.channels.length === 0}
                            <p class="text-sm text-base-content/45 m-0 leading-snug px-2.5">No channels yet. Add channels from the matrix.</p>
                        {:else}
                            <div class="flex flex-col">
                                {#each config.channels as c (c.name)}
                                    <KeybindRow
                                        label={c.name}
                                        sublabel={c.kind === 'physical_input' ? 'Hardware mic' : c.kind === 'input' ? 'Virtual mic' : 'Output channel'}
                                        action={{ type: 'toggle_channel_mute', channel: c.name }}
                                        bound={bindingFor({ type: 'toggle_channel_mute', channel: c.name })}
                                        onSet={onSetKeybind}
                                        onClear={onClearKeybind}
                                        isConflict={isAccelTaken}
                                    />
                                {/each}
                            </div>
                        {/if}
                    {:else if kbCategory === 'mixes'}
                        <div class="flex items-center gap-2 px-2.5 mb-2 text-xs text-base-content/55">
                            <Icon name="split" size={12} />
                            <span>Enable / disable mixes. A disabled mix stops routing to its outputs.</span>
                        </div>
                        {#if config.mixes.length === 0}
                            <p class="text-sm text-base-content/45 m-0 leading-snug px-2.5">No mixes yet. Add a mix from the sidebar.</p>
                        {:else}
                            <div class="flex flex-col">
                                {#each config.mixes as m (m.id)}
                                    <KeybindRow
                                        label={m.name}
                                        action={{ type: 'toggle_mix_enabled', mix_id: m.id }}
                                        bound={bindingFor({ type: 'toggle_mix_enabled', mix_id: m.id })}
                                        onSet={onSetKeybind}
                                        onClear={onClearKeybind}
                                        isConflict={isAccelTaken}
                                    />
                                {/each}
                            </div>
                        {/if}
                    {/if}
                {/if}
            {:else if active.kind === 'ptt'}
                <SettingsPtt {config} {onConfigUpdate} />
            {:else if active.kind === 'devices'}
                <header class="flex flex-col gap-1 mb-4">
                    <h3 class="text-base font-semibold m-0">Devices</h3>
                    <p class="text-sm text-base-content/55 m-0 leading-snug">Audio backend status and detected sinks.</p>
                </header>

                {@const ok = backend && backend.server_name && backend.errors.length === 0}
                {@const warn = backend && backend.server_name && backend.errors.length > 0}
                {@const broken = backend && !backend.server_name}
                <div class="rounded-md border px-3 py-2.5 mb-4
                            {ok ? 'border-success/30 bg-success/10' : ''}
                            {warn ? 'border-warning/30 bg-warning/10' : ''}
                            {broken ? 'border-error/30 bg-error/10' : ''}
                            {!backend ? 'border-base-content/10 bg-base-100' : ''}">
                    <div class="flex items-center gap-2 mb-1">
                        <span class="w-2 h-2 rounded-full
                                     {ok ? 'bg-success' : ''}
                                     {warn ? 'bg-warning' : ''}
                                     {broken ? 'bg-error' : ''}
                                     {!backend ? 'bg-base-content/30' : ''}"
                              aria-hidden="true"></span>
                        <span class="text-sm font-semibold flex-1">
                            {#if !backend}Checking…
                            {:else if broken}Audio backend not detected
                            {:else if backend.on_pipewire}PipeWire ready
                            {:else}{backend.server_name}
                            {/if}
                        </span>
                        <button
                            type="button"
                            class="btn btn-ghost btn-xs"
                            onclick={refreshBackend}
                            disabled={backendChecking}
                            title="Re-check and start services if needed"
                        >
                            {backendChecking ? 'Checking…' : 'Re-check'}
                        </button>
                    </div>
                    {#if backend?.server_name}
                        <p class="text-xs font-mono text-base-content/55 m-0 truncate">{backend.server_name}</p>
                    {/if}
                    {#if backend?.started_services?.length}
                        <p class="text-xs text-base-content/70 m-0 mt-1">Started: {backend.started_services.join(', ')}</p>
                    {/if}
                    {#if backend?.errors?.length}
                        <ul class="text-xs text-error m-0 mt-1.5 pl-4 list-disc flex flex-col gap-0.5">
                            {#each backend.errors as err (err)}
                                <li>{err}</li>
                            {/each}
                        </ul>
                    {/if}
                    {#if broken}
                        <p class="text-xs text-base-content/70 m-0 mt-1.5 leading-snug">
                            Tideline tried to start <code class="font-mono">pipewire</code>,
                            <code class="font-mono">pipewire-pulse</code>, and
                            <code class="font-mono">wireplumber</code>, but <code class="font-mono">pactl info</code> still fails.
                            Check that those packages are installed and your user systemd is running.
                        </p>
                    {/if}
                    {#if backendError}
                        <p class="text-xs text-error m-0 mt-1">{backendError}</p>
                    {/if}
                </div>

                <h4 class="text-[10px] font-bold uppercase tracking-widest text-base-content/55 m-0 mb-2">Detected sinks</h4>
                {#if sinksError}
                    <div class="alert alert-error alert-soft py-2">
                        <span class="text-sm">{sinksError}</span>
                    </div>
                {:else if sinks.length === 0}
                    <p class="text-sm text-base-content/55 m-0 leading-snug">No sinks detected. Is PipeWire running?</p>
                {:else}
                    <ul class="list bg-base-100 border border-base-content/10 rounded-md">
                        {#each sinks as s (s.name)}
                            <li class="list-row flex flex-col gap-0.5 py-2 px-3">
                                <span class="text-sm">{s.description}</span>
                                <span class="font-mono text-xs text-base-content/55 truncate">{s.name}</span>
                            </li>
                        {/each}
                    </ul>
                {/if}
            {:else if active.kind === 'plugin'}
                {@const pluginActive = active}
                {@const section = pluginUi.contributions.settings_sections.find((s) => s.plugin_id === pluginActive.plugin_id && s.surface_id === pluginActive.surface_id)}
                {#if section}
                    <PluginSettingsSection {section} emit={(e) => pluginUi.emit(pluginActive.plugin_id, e)} />
                {/if}
            {/if}
        </section>
    </div>

    <div class="flex justify-end px-4 py-3 border-t border-base-content/10">
        <button class="btn btn-soft btn-sm" onclick={onClose}>Close</button>
    </div>
</Modal>
{/if}
