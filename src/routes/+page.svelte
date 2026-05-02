<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { onMount, onDestroy } from 'svelte';
import MatrixRow from '$lib/MatrixRow.svelte';
import AddChannelModal from '$lib/AddChannelModal.svelte';
import AddInputModal from '$lib/AddInputModal.svelte';
import AddMixModal from '$lib/AddMixModal.svelte';
import ChannelSettingsModal from '$lib/ChannelSettingsModal.svelte';
import InputDetail from '$lib/InputDetail.svelte';
import OutputDetail from '$lib/OutputDetail.svelte';
import MixDetail from '$lib/MixDetail.svelte';
import Settings from '$lib/Settings.svelte';
import TitleBar from '$lib/TitleBar.svelte';
import ResizeEdges from '$lib/ResizeEdges.svelte';
import Icon from '$lib/Icon.svelte';
import Sidebar from '$lib/Sidebar.svelte';
import StatusPill from '$lib/StatusPill.svelte';
import Toaster from '$lib/Toaster.svelte';
import { toaster } from '$lib/toaster.svelte';
import ChannelOverlay from '$lib/plugin-ui/ChannelOverlay.svelte';
import PermissionsDialog from '$lib/plugin-ui/PermissionsDialog.svelte';
import { pluginUi } from '$lib/plugin-ui/pluginUi.svelte';
import type { AppConfig, AudioBackendStatus, ChannelConfig, ChannelKind, ChannelVolumes, KeybindAction, Mix, SinkInfo } from '$lib/types';

type View = 'mixes' | 'input' | 'output' | 'mix';

let config = $state<AppConfig>({
    mixes: [],
    channels: [],
    keybinds: {},
    ptt: {
        mode: 'open',
        mode_toggle_binding: null,
        hold_binding: null,
        input_device: '',
        led_enabled: true,
    },
});
let mixEnabled = $state<Record<string, boolean>>({});
let settingsOpen = $state(false);
let pipewireOk = $state(true);
let pipewireError = $state<string>('');

let activeView = $state<View>('mixes');
let selectedInput = $state<string | null>(null);
let selectedOutput = $state<string | null>(null);
let selectedMix = $state<string | null>(null);

let outputs = $state<SinkInfo[]>([]);
let appIcons = $state<Record<string, string | null>>({});
let channelVolumes = $state<Record<string, ChannelVolumes>>({});

async function fetchMissingAppIcons(binaries: string[]) {
    const missing = binaries.filter(b => b && !(b in appIcons));
    if (missing.length === 0) return;
    try {
        const resolved = await invoke<Record<string, string | null>>('resolve_app_icons', { binaries: missing });
        appIcons = { ...appIcons, ...resolved };
    } catch (_) {}
}

$effect(() => {
    const all = new Set<string>();
    for (const ch of config.channels) {
        for (const p of ch.programs ?? []) all.add(p);
    }
    fetchMissingAppIcons(Array.from(all));
});

let inputs = $derived(config.channels.filter(c => c.kind === 'physical_input' || c.kind === 'input'));
let matrixChannels = $derived(
    config.channels.map((ch, configIndex) => ({ ch, configIndex })),
);
let activeInput = $derived(
    activeView === 'input' && selectedInput
        ? inputs.find(i => i.name === selectedInput) ?? null
        : null,
);
let activeOutput = $derived(
    activeView === 'output' && selectedOutput
        ? outputs.find(o => o.name === selectedOutput) ?? null
        : null,
);
let activeMix = $derived(
    activeView === 'mix' && selectedMix
        ? config.mixes.find(m => m.id === selectedMix) ?? null
        : null,
);

function overlaysFor(channelId: string, placement: 'detail' | 'sidebar_badge' | 'header_chip') {
    return pluginUi.contributions.channel_overlays.filter((o) => {
        if (o.placement !== placement) return false;
        if (o.channel_filter.kind === 'all') return true;
        return o.channel_filter.ids.includes(channelId);
    });
}

let activeChannelId = $derived(
    activeView === 'input' && selectedInput
        ? inputs.find(i => i.name === selectedInput)?.uuid ?? null
        : null,
);
let detailOverlays = $derived(activeChannelId ? overlaysFor(activeChannelId, 'detail') : []);
let headerChips = $derived(activeChannelId ? overlaysFor(activeChannelId, 'header_chip') : []);

$effect(() => {
    if (activeView === 'input' && selectedInput && !inputs.some(i => i.name === selectedInput)) {
        activeView = 'mixes';
        selectedInput = null;
    }
});

$effect(() => {
    if (activeView === 'output' && selectedOutput && !outputs.some(o => o.name === selectedOutput)) {
        activeView = 'mixes';
        selectedOutput = null;
    }
});

$effect(() => {
    if (activeView === 'mix' && selectedMix && !config.mixes.some(m => m.id === selectedMix)) {
        activeView = 'mixes';
        selectedMix = null;
    }
});

let addInputOpen = $state(false);
let addChannelOpen = $state(false);
let addMixOpen = $state(false);
let settingsForIndex = $state<number | null>(null);
let settingsForChannel = $derived(settingsForIndex !== null ? config.channels[settingsForIndex] ?? null : null);

onMount(() => {
    void pluginUi.init();
    invoke<AppConfig>('get_config').then(c => { config = c; });
    invoke<Record<string, boolean>>('get_mix_enabled').then(m => { mixEnabled = m; });
    invoke<SinkInfo[]>('list_sinks').then(s => { outputs = s; }).catch(() => {});
    invoke<Record<string, ChannelVolumes>>('get_all_channel_volumes')
        .then(v => { channelVolumes = v; })
        .catch(() => {});

    invoke<AudioBackendStatus | null>('get_initial_backend_status').then(status => {
        if (!status) return;
        if (status.started_services.length > 0) {
            const apps = status.affected_apps.length > 0
                ? status.affected_apps.join(', ')
                : 'any browser, voice chat, or media app you have open';
            toaster.push({
                kind: 'warning',
                title: 'PipeWire was just started',
                body: `Restart these apps so they pick up audio: ${apps}.`,
                timeoutMs: 8000,
            });
        } else if (status.errors.length > 0 && !status.server_name) {
            toaster.push({
                kind: 'error',
                title: 'Audio backend not detected',
                body: status.errors.join('\n'),
                timeoutMs: 0,
            });
        }
    }).catch(() => {});

    const poll = setInterval(async () => {
        try {
            const m = await invoke<Record<string, boolean>>('get_mix_enabled');
            mixEnabled = m;
            const s = await invoke<SinkInfo[]>('list_sinks');
            outputs = s;
            if (!pipewireOk) { pipewireOk = true; pipewireError = ''; }
        } catch (e) {
            pipewireOk = false;
            pipewireError = String(e);
        }
    }, 3000);

    return () => clearInterval(poll);
});

async function toggleOutputMute(out: SinkInfo) {
    const prev = outputs;
    const next = !out.muted;
    outputs = outputs.map(o => o.name === out.name ? { ...o, muted: next } : o);
    try {
        await invoke('set_sink_mute', { name: out.name, muted: next });
    } catch (e) {
        outputs = prev;
        pipewireOk = false;
        pipewireError = String(e);
    }
}

async function setOutputVolume(out: SinkInfo, vol: number) {
    const prev = outputs;
    outputs = outputs.map(o => o.name === out.name ? { ...o, volume_percent: vol } : o);
    try {
        await invoke('set_sink_volume', { name: out.name, pct: vol });
    } catch (e) {
        outputs = prev;
        pipewireOk = false;
        pipewireError = String(e);
    }
}

async function toggleMixEnabled(mixId: string) {
    const next = !(mixEnabled[mixId] !== false);
    mixEnabled = { ...mixEnabled, [mixId]: next };
    try {
        await invoke('set_mix_enabled', { id: mixId, enabled: next });
    } catch (e) {
        pipewireOk = false;
        pipewireError = String(e);
    }
}

function slug(name: string): string {
    return name.toLowerCase().replace(/[^a-z0-9]/g, '_').replace(/_+/g, '_').replace(/^_|_$/g, '');
}

function meterSourceFor(ch: ChannelConfig): string {
    if (ch.kind === 'physical_input') return ch.physical_source;
    return `sink.${slug(ch.name)}.monitor`;
}

let meterSources = $derived([
    ...config.channels.map(meterSourceFor).filter(Boolean),
    ...outputs.filter(o => !o.muted).map(o => `${o.name}.monitor`),
]);
let pageVisible = $state(typeof document !== 'undefined' ? !document.hidden : true);

$effect(() => {
    function onVis() { pageVisible = !document.hidden; }
    document.addEventListener('visibilitychange', onVis);
    return () => document.removeEventListener('visibilitychange', onVis);
});

$effect(() => {
    const sources = pageVisible ? meterSources : [];
    invoke('watch_levels', { sources }).catch(() => {});
});

function buildChannel(name: string, kind: ChannelKind, physical_source = ''): ChannelConfig {
    const s = slug(name);
    const uuid = crypto.randomUUID();
    if (kind === 'output') {
        return { uuid, name, kind, hp_node: `playback.${s}-hp`, sp_node: `playback.${s}-sp`, programs: [], sources: [], physical_source: '', icon: '' };
    }
    if (kind === 'physical_input') {
        return { uuid, name, kind, hp_node: '', sp_node: '', programs: [], sources: [], physical_source, icon: '' };
    }
    return { uuid, name, kind, hp_node: '', sp_node: '', programs: [], sources: [], physical_source: '', icon: '' };
}

async function applyConfig(next: AppConfig) {
    try {
        await invoke('save_config', { config: $state.snapshot(next) });
        config = next;
    } catch (e) {
        pipewireOk = false;
        pipewireError = String(e);
        throw e;
    }
}

async function applyConfigQuiet(next: AppConfig) {
    const prev = config;
    config = next;
    try {
        await invoke('save_config_quiet', { config: $state.snapshot(next) });
    } catch (e) {
        config = prev;
        pipewireOk = false;
        pipewireError = String(e);
    }
}

async function addChannel(name: string) {
    const next: AppConfig = { ...config, channels: [...config.channels, buildChannel(name, 'output')] };
    await applyConfig(next);
}

async function addInput(kind: 'input' | 'physical_input', name: string, physicalSource: string) {
    const next: AppConfig = {
        ...config,
        channels: [...config.channels, buildChannel(name, kind, physicalSource)],
    };
    await applyConfig(next);
    activeView = 'input';
    selectedInput = name;
}

async function deleteChannel(index: number) {
    const next: AppConfig = { ...config, channels: config.channels.filter((_, i) => i !== index) };
    await applyConfig(next);
}

async function reorderChannel(fromIndex: number, toIndex: number) {
    if (fromIndex === toIndex || fromIndex < 0 || toIndex < 0) return;
    if (fromIndex >= config.channels.length || toIndex > config.channels.length) return;
    const arr = [...config.channels];
    const [moved] = arr.splice(fromIndex, 1);
    const insertAt = toIndex > fromIndex ? toIndex - 1 : toIndex;
    arr.splice(insertAt, 0, moved);
    config = { ...config, channels: arr };
    try {
        await invoke('reorder_channels', { order: arr.map(c => c.name) });
    } catch (e) {
        pipewireOk = false;
        pipewireError = String(e);
    }
}

let dragIndex = $state<number | null>(null);
let dragOverIndex = $state<number | null>(null);

function handleDragStart(index: number, e: DragEvent) {
    dragIndex = index;
    if (e.dataTransfer) {
        e.dataTransfer.effectAllowed = 'move';
        e.dataTransfer.setData('text/plain', String(index));
    }
}
function handleDragEnd() {
    dragIndex = null;
    dragOverIndex = null;
}
function handleDragEnter(index: number) {
    if (dragIndex === null) return;
    dragOverIndex = index;
}
function handleDragOver(e: DragEvent) {
    if (dragIndex === null) return;
    e.preventDefault();
    if (e.dataTransfer) e.dataTransfer.dropEffect = 'move';
}
async function handleDrop(targetIndex: number, e: DragEvent) {
    e.preventDefault();
    if (dragIndex === null) return;
    const from = dragIndex;
    dragIndex = null;
    dragOverIndex = null;
    await reorderChannel(from, targetIndex);
}

function updateChannel(index: number, patch: Partial<ChannelConfig>): AppConfig {
    return {
        ...config,
        channels: config.channels.map((c, i) => i === index ? { ...c, ...patch } : c),
    };
}

async function addProgram(index: number, binary: string) {
    const ch = config.channels[index];
    if (!ch || ch.programs.includes(binary)) return;
    await applyConfigQuiet(updateChannel(index, { programs: [...ch.programs, binary] }));
}

async function removeProgram(index: number, binary: string) {
    const ch = config.channels[index];
    if (!ch) return;
    await applyConfigQuiet(updateChannel(index, { programs: ch.programs.filter(p => p !== binary) }));
}

async function addSource(index: number, src: string) {
    const ch = config.channels[index];
    if (!ch || ch.sources.includes(src)) return;
    await applyConfigQuiet(updateChannel(index, { sources: [...ch.sources, src] }));
}

async function removeSource(index: number, src: string) {
    const ch = config.channels[index];
    if (!ch) return;
    await applyConfigQuiet(updateChannel(index, { sources: ch.sources.filter(s => s !== src) }));
}

async function renameChannel(index: number, newName: string) {
    const ch = config.channels[index];
    if (!ch || ch.name === newName) return;
    if (config.channels.some((c, i) => i !== index && c.name === newName)) {
        throw new Error('Name already taken');
    }
    await applyConfig(updateChannel(index, { name: newName }));
}

async function changeChannelIcon(index: number, icon: string) {
    const ch = config.channels[index];
    if (!ch) return;
    config = updateChannel(index, { icon });
    try {
        await invoke('set_channel_icon', { name: ch.name, icon });
    } catch (e) {
        pipewireOk = false;
        pipewireError = String(e);
    }
}

function mixIdFromName(name: string): string {
    const base = slug(name) || 'mix';
    const taken = new Set(config.mixes.map(m => m.id));
    if (!taken.has(base)) return base;
    let i = 2;
    while (taken.has(`${base}_${i}`)) i++;
    return `${base}_${i}`;
}

async function addMix(name: string, sinks: string[]) {
    const id = mixIdFromName(name);
    const next: AppConfig = { ...config, mixes: [...config.mixes, { uuid: crypto.randomUUID(), id, name, sinks }] };
    await applyConfig(next);
    activeView = 'mix';
    selectedMix = id;
}

async function updateMix(id: string, patch: Partial<Mix>) {
    const next: AppConfig = {
        ...config,
        mixes: config.mixes.map(m => m.id === id ? { ...m, ...patch } : m),
    };
    await applyConfig(next);
}

async function deleteMix(id: string) {
    const next: AppConfig = { ...config, mixes: config.mixes.filter(m => m.id !== id) };
    await applyConfig(next);
    activeView = 'mixes';
    selectedMix = null;
}

async function setKeybind(accelerator: string, action: KeybindAction) {
    await invoke('set_keybind', { accelerator, action });
    config = { ...config, keybinds: { ...config.keybinds, [accelerator]: action } };
}

async function clearKeybind(accelerator: string) {
    await invoke('clear_keybind', { accelerator });
    const { [accelerator]: _, ...rest } = config.keybinds;
    config = { ...config, keybinds: rest };
}

onDestroy(() => pluginUi.teardown());
</script>

<div class="flex flex-col h-full bg-base-100">
    <ResizeEdges />
    <TitleBar>
        <StatusPill />
    </TitleBar>
    {#if !pipewireOk}
        <div class="flex items-center gap-2 px-3 py-1.5 bg-error/15 border-b border-error text-sm text-error flex-shrink-0" role="alert">
            <span class="w-2 h-2 rounded-full bg-error shadow-[0_0_6px_var(--color-error)] flex-shrink-0 animate-pulse" aria-hidden="true"></span>
            <span class="font-bold tracking-wider uppercase">Audio engine offline</span>
            <span class="text-base-content/55 overflow-hidden text-ellipsis whitespace-nowrap min-w-0" title={pipewireError}>{pipewireError.slice(0, 80)}</span>
        </div>
    {/if}
    {#snippet inputBadgeFor(input: ChannelConfig)}
        {@const overlays = overlaysFor(input.uuid, 'sidebar_badge')}
        {#each overlays as o (o.plugin_id + ':' + o.surface_id)}
            <ChannelOverlay overlay={o} emit={(e) => pluginUi.emit(o.plugin_id, e)} />
        {/each}
    {/snippet}
    {#snippet outputBadgeFor(output: SinkInfo)}
        {@const overlays = overlaysFor(output.name, 'sidebar_badge')}
        {#each overlays as o (o.plugin_id + ':' + o.surface_id)}
            <ChannelOverlay overlay={o} emit={(e) => pluginUi.emit(o.plugin_id, e)} />
        {/each}
    {/snippet}

    <div class="flex flex-1 min-h-0">
        <Sidebar
            {inputs}
            {outputs}
            mixes={config.mixes}
            {selectedInput}
            {selectedOutput}
            {selectedMix}
            {activeView}
            onSelectMixes={() => { activeView = 'mixes'; selectedInput = null; selectedOutput = null; selectedMix = null; }}
            onSelectInput={(name) => { activeView = 'input'; selectedInput = name; selectedOutput = null; selectedMix = null; }}
            onSelectOutput={(name) => { activeView = 'output'; selectedOutput = name; selectedInput = null; selectedMix = null; }}
            onSelectMix={(id) => { activeView = 'mix'; selectedMix = id; selectedInput = null; selectedOutput = null; }}
            onAddInput={() => addInputOpen = true}
            onAddMix={() => addMixOpen = true}
            onToggleOutputMute={toggleOutputMute}
            onSetOutputVolume={setOutputVolume}
            inputMeterSource={meterSourceFor}
            inputSinkName={(ch) => ch.kind === 'physical_input' ? '' : `sink.${slug(ch.name)}`}
            onSettings={() => settingsOpen = true}
            {inputBadgeFor}
            {outputBadgeFor}
        />

        <main class="flex-1 flex flex-col min-w-0 min-h-0 bg-base-100">
            {#if activeView === 'mixes'}
                <header class="flex items-center justify-between gap-3 px-4 py-3 border-b border-base-content/15 flex-shrink-0">
                    <h1 class="m-0 text-lg font-semibold text-base-content tracking-wide">Mixes</h1>
                    {#if config.mixes.length > 0}
                        <div class="flex gap-1.5 flex-wrap" role="group" aria-label="Mix enable">
                            {#each config.mixes as mix (mix.id)}
                                {@const enabled = mixEnabled[mix.id] !== false}
                                <button
                                    class="inline-flex items-center gap-1.5 px-2.5 py-1 pl-2 rounded-full border text-sm font-semibold tracking-wide cursor-pointer transition-colors
                                           {enabled
                                             ? 'bg-primary/15 text-primary border-primary'
                                             : 'bg-base-100 text-base-content/55 border-base-content/15 hover:bg-base-300 hover:text-base-content hover:border-base-content/25'}"
                                    aria-pressed={enabled}
                                    onclick={() => toggleMixEnabled(mix.id)}
                                    title={enabled ? `Disable ${mix.name}` : `Enable ${mix.name}`}
                                >
                                    <span class="w-2 h-2 rounded-full flex-shrink-0 transition-all {enabled ? 'bg-primary shadow-[0_0_0_2px_var(--color-primary)]/15' : 'bg-base-content/45'}"></span>
                                    <span class="overflow-hidden text-ellipsis whitespace-nowrap max-w-[140px]">{mix.name}</span>
                                </button>
                            {/each}
                        </div>
                    {/if}
                </header>

                {#if config.mixes.length === 0}
                    <div class="flex-1 flex flex-col items-center justify-center gap-3 p-4 text-center">
                        <div class="flex items-center justify-center w-14 h-14 rounded-full bg-primary/15 text-primary mb-1">
                            <Icon name="mixer" size={28} />
                        </div>
                        <h2 class="m-0 text-lg font-semibold text-base-content">No mixes yet</h2>
                        <p class="m-0 max-w-[38ch] text-sm text-base-content/55 leading-relaxed">
                            A mix is a destination — like Headphones, Speakers, or a Stream Bus —
                            that one or more output channels can route to.
                        </p>
                        <button class="btn btn-primary mt-1 gap-2" onclick={() => addMixOpen = true}>
                            <Icon name="plus" size={16} />
                            <span>Add your first mix</span>
                        </button>
                    </div>
                {:else}
                    <div class="flex flex-col flex-1 pb-4 min-h-0 min-w-0 overflow-auto gap-0">
                        <div
                            class="grid gap-2 px-6 pt-3 pb-2 border-b border-base-content/15 items-stretch sticky top-0 bg-base-100 z-[5] w-max min-w-full"
                            style:grid-template-columns="380px repeat({config.mixes.length}, 240px) 80px"
                        >
                            <div class="flex flex-col items-start justify-end pl-9 pb-2 pr-2.5 min-w-0">
                                <span class="text-[15px] font-semibold tracking-wide leading-tight text-base-content">Channels</span>
                                <span class="text-[13px] font-bold tracking-widest uppercase text-base-content/45 leading-tight">
                                    {config.channels.length === 1 ? '1 Channel' : `${config.channels.length} Channels`}
                                </span>
                            </div>
                            {#each config.mixes as mix (mix.id)}
                                {@const enabled = mixEnabled[mix.id] !== false}
                                {@const empty = mix.sinks.length === 0}
                                <button
                                    type="button"
                                    class="relative flex flex-col items-center justify-center gap-0.5 px-2.5 pt-2 pb-2.5 min-w-0 bg-transparent border-none rounded text-current cursor-pointer text-center transition-colors hover:bg-base-300"
                                    onclick={() => { activeView = 'mix'; selectedMix = mix.id; selectedInput = null; selectedOutput = null; }}
                                    title="{mix.name} — click to configure"
                                >
                                    <span class="w-[22px] h-[22px] flex items-center justify-center mb-0.5 {enabled && !empty ? 'text-primary' : empty ? 'text-base-content/45' : 'text-base-content/55'}" aria-hidden="true">
                                        <Icon name="mixer" size={14} />
                                    </span>
                                    <span class="text-[15px] font-semibold tracking-wide leading-tight overflow-hidden text-ellipsis whitespace-nowrap max-w-full {empty ? 'text-base-content/55' : 'text-base-content'}">{mix.name}</span>
                                    <span class="text-[13px] font-bold tracking-widest uppercase text-base-content/45 leading-tight">
                                        {empty ? 'No outputs' : mix.sinks.length === 1 ? '1 Output' : `${mix.sinks.length} Outputs`}
                                    </span>
                                </button>
                            {/each}
                            <button
                                type="button"
                                class="flex flex-col items-center justify-center gap-1 py-2 px-1.5 bg-transparent border-[1.5px] border-dashed border-base-content/15 rounded text-base-content/45 cursor-pointer transition-colors hover:bg-primary/15 hover:border-primary hover:text-primary"
                                onclick={() => addMixOpen = true}
                                aria-label="Create mix"
                                title="Create mix"
                            >
                                <Icon name="plus" size={16} />
                                <span class="text-[13px] font-bold tracking-widest">MIX</span>
                            </button>
                        </div>

                        <div class="flex flex-col gap-1.5 mt-4 w-max min-w-full">
                            {#each matrixChannels as { ch, configIndex } (ch.name)}
                                <MatrixRow
                                    name={ch.name}
                                    kind={ch.kind ?? 'output'}
                                    icon={ch.icon ?? ''}
                                    mixes={config.mixes}
                                    {mixEnabled}
                                    meterSource={meterSourceFor(ch)}
                                    sourceName={ch.kind === 'physical_input' ? ch.physical_source : ch.kind === 'input' ? `sink.${slug(ch.name)}.monitor` : undefined}
                                    programs={ch.programs ?? []}
                                    {appIcons}
                                    initialVolumes={channelVolumes[ch.name]}
                                    onSettings={() => settingsForIndex = configIndex}
                                    draggable={true}
                                    isDragging={dragIndex === configIndex}
                                    isDragOver={dragOverIndex === configIndex && dragIndex !== configIndex}
                                    onDragStart={(e) => handleDragStart(configIndex, e)}
                                    onDragEnd={handleDragEnd}
                                    onDragEnter={() => handleDragEnter(configIndex)}
                                    onDragOver={handleDragOver}
                                    onDrop={(e) => handleDrop(configIndex, e)}
                                />
                            {/each}
                            <button
                                class="flex items-center justify-center gap-2 w-[380px] ml-6 px-3 py-[11px] mt-2 bg-transparent border border-dashed rounded cursor-pointer transition-colors flex-shrink-0 self-start
                                       {dragOverIndex === config.channels.length
                                         ? 'border-primary border-solid text-primary bg-primary/30 shadow-[inset_0_0_0_2px_var(--color-primary)]'
                                         : dragIndex !== null
                                           ? 'border-primary border-solid text-primary bg-primary/15'
                                           : 'border-base-content/15 text-base-content/55 hover:border-primary hover:text-primary hover:bg-primary/15'}"
                                onclick={() => addChannelOpen = true}
                                ondragenter={() => handleDragEnter(config.channels.length)}
                                ondragover={handleDragOver}
                                ondrop={(e) => handleDrop(config.channels.length, e)}
                                aria-label="Create channel"
                                title="Create channel"
                            >
                                <Icon name="plus" size={14} />
                                <span class="text-[15px] font-semibold tracking-wide">Create channel</span>
                            </button>
                        </div>
                    </div>
                {/if}
            {:else if activeInput}
                {@const inputIndex = config.channels.findIndex(c => c.name === activeInput.name)}
                <header class="flex items-center justify-between gap-3 px-4 py-3 border-b border-base-content/15 flex-shrink-0">
                    <h1 class="m-0 text-lg font-semibold text-base-content tracking-wide">{activeInput.name}</h1>
                    <span class="mr-auto px-2.5 py-0.5 text-[13px] font-bold tracking-widest uppercase rounded-full border
                                 {activeInput.kind === 'physical_input'
                                   ? 'text-error bg-error/15 border-error'
                                   : 'text-primary bg-primary/15 border-primary'}">
                        {activeInput.kind === 'physical_input' ? 'Hardware mic' : 'Virtual mic'}
                    </span>
                    {#if headerChips.length > 0}
                        <div class="flex items-center gap-1">
                            {#each headerChips as o (o.plugin_id + ':' + o.surface_id)}
                                <ChannelOverlay overlay={o} emit={(e) => pluginUi.emit(o.plugin_id, e)} />
                            {/each}
                        </div>
                    {/if}
                </header>
                <div class="flex-1 p-4 overflow-y-auto">
                    <InputDetail
                        name={activeInput.name}
                        kind={activeInput.kind}
                        sinkName={`sink.${slug(activeInput.name)}`}
                        physicalSource={activeInput.physical_source ?? ''}
                        meterSource={meterSourceFor(activeInput)}
                        sources={activeInput.sources ?? []}
                        onAddSource={(s) => addSource(inputIndex, s)}
                        onRemoveSource={(s) => removeSource(inputIndex, s)}
                        onDelete={() => deleteChannel(inputIndex)}
                    />
                    {#each detailOverlays as o (o.plugin_id + ':' + o.surface_id)}
                        <ChannelOverlay overlay={o} emit={(e) => pluginUi.emit(o.plugin_id, e)} />
                    {/each}
                </div>
            {:else if activeOutput}
                <header class="flex items-center justify-between gap-3 px-4 py-3 border-b border-base-content/15 flex-shrink-0">
                    <h1 class="m-0 text-lg font-semibold text-base-content tracking-wide">{activeOutput.description}</h1>
                    <span class="mr-auto px-2.5 py-0.5 text-[13px] font-bold tracking-widest uppercase rounded-full border text-base-content/55 bg-base-300 border-base-content/15">Output</span>
                </header>
                <div class="flex-1 p-4 overflow-y-auto">
                    <OutputDetail
                        sinkName={activeOutput.name}
                        description={activeOutput.description}
                    />
                </div>
            {:else if activeMix}
                {@const mixId = activeMix.id}
                <header class="flex items-center justify-between gap-3 px-4 py-3 border-b border-base-content/15 flex-shrink-0">
                    <h1 class="m-0 text-lg font-semibold text-base-content tracking-wide">{activeMix.name}</h1>
                    <span class="mr-auto px-2.5 py-0.5 text-[13px] font-bold tracking-widest uppercase rounded-full border text-primary bg-primary/15 border-primary">Mix</span>
                </header>
                <div class="flex-1 p-4 overflow-y-auto">
                    <MixDetail
                        mix={activeMix}
                        enabled={mixEnabled[mixId] !== false}
                        onUpdate={(patch) => updateMix(mixId, patch)}
                        onToggleEnabled={() => toggleMixEnabled(mixId)}
                        onDelete={() => deleteMix(mixId)}
                    />
                </div>
            {/if}
        </main>
    </div>
</div>

<Toaster />

<Settings
    bind:open={settingsOpen}
    {config}
    {outputs}
    onSetKeybind={setKeybind}
    onClearKeybind={clearKeybind}
    onConfigUpdate={(next) => config = next}
    onClose={() => settingsOpen = false}
/>

{#if addInputOpen}
    <AddInputModal
        existingNames={config.channels.map(c => c.name)}
        onCreate={addInput}
        onClose={() => addInputOpen = false}
    />
{/if}

{#if addChannelOpen}
    <AddChannelModal
        existingNames={config.channels.map(c => c.name)}
        onCreate={addChannel}
        onClose={() => addChannelOpen = false}
    />
{/if}

{#if addMixOpen}
    <AddMixModal
        existingNames={config.mixes.map(m => m.name)}
        onCreate={addMix}
        onClose={() => addMixOpen = false}
    />
{/if}

{#if settingsForChannel && settingsForIndex !== null}
    {@const idx = settingsForIndex}
    <ChannelSettingsModal
        name={settingsForChannel.name}
        kind={settingsForChannel.kind ?? 'output'}
        icon={settingsForChannel.icon ?? ''}
        programs={settingsForChannel.programs ?? []}
        {appIcons}
        existingNames={config.channels.map(c => c.name)}
        onRename={(n) => renameChannel(idx, n)}
        onChangeIcon={(i) => changeChannelIcon(idx, i)}
        onAddProgram={(b) => addProgram(idx, b)}
        onRemoveProgram={(b) => removeProgram(idx, b)}
        onDelete={() => deleteChannel(idx)}
        onClose={() => settingsForIndex = null}
    />
{/if}

{#if pluginUi.permissionPrompt}
    <PermissionsDialog
        request={pluginUi.permissionPrompt}
        onResolve={(grant) => pluginUi.respondPermission(grant)}
    />
{/if}
