<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { open as openDialog, save as saveDialog } from '@tauri-apps/plugin-dialog';
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
    physicalSource?: string;
    onClose: () => void;
}
let { pluginId, channelUuid, channelName, channelKind, physicalSource, onClose }: Props = $props();

type AuditionPhase = 'idle' | 'recording' | 'looping' | 'paused';
let auditionPhase = $state<AuditionPhase>('idle');
let auditionHasSample = $state(false);
let auditionError = $state<string | null>(null);
let auditionBusy = $state(false);
let recordingStartedAt = $state<number | null>(null);
let recordingElapsed = $state(0);
let recordTimer: ReturnType<typeof setInterval> | null = null;
let auditionVisible = $derived(channelKind === 'physical_input' && (physicalSource ?? '').length > 0);

let lowcutOn = $state(false);
let clipguardOn = $state(false);
let dspBusy = $state(false);

async function pullDspState() {
    try {
        const s = await invoke<{ lowcut: boolean; clipguard: boolean }>('tideline_plugin_request', {
            pluginId,
            method: 'effects.get_dsp_state',
            params: { channel_uuid: channelUuid },
        });
        lowcutOn = s.lowcut;
        clipguardOn = s.clipguard;
    } catch (e) {
        console.warn('get_dsp_state failed', e);
    }
}

async function toggleLowcut() {
    if (dspBusy) return;
    dspBusy = true;
    const next = !lowcutOn;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.set_lowcut',
            params: { channel_uuid: channelUuid, enabled: next },
        });
        lowcutOn = next;
    } catch (e) {
        console.warn('set_lowcut failed', e);
    } finally {
        dspBusy = false;
    }
}

async function toggleClipguard() {
    if (dspBusy) return;
    dspBusy = true;
    const next = !clipguardOn;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.set_clipguard',
            params: { channel_uuid: channelUuid, enabled: next },
        });
        clipguardOn = next;
    } catch (e) {
        console.warn('set_clipguard failed', e);
    } finally {
        dspBusy = false;
    }
}

async function pullAuditionState() {
    try {
        const s = await invoke<{ phase: AuditionPhase; has_sample: boolean }>('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_state',
            params: { channel_uuid: channelUuid },
        });
        auditionPhase = s.phase;
        auditionHasSample = s.has_sample;
        if (auditionPhase !== 'recording') stopRecordTimer();
    } catch (e) {
        // Soft-fail — audition is a non-critical surface.
        console.warn('audition_state failed', e);
    }
}

function startRecordTimer() {
    stopRecordTimer();
    recordingStartedAt = performance.now();
    recordingElapsed = 0;
    recordTimer = setInterval(() => {
        if (recordingStartedAt != null) {
            recordingElapsed = (performance.now() - recordingStartedAt) / 1000;
        }
    }, 100);
}

function stopRecordTimer() {
    if (recordTimer != null) {
        clearInterval(recordTimer);
        recordTimer = null;
    }
    recordingStartedAt = null;
}

async function auditionRecord() {
    if (auditionBusy) return;
    auditionBusy = true;
    auditionError = null;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_record_start',
            params: {
                channel_uuid: channelUuid,
                channel_name: channelName ?? '',
                physical_source: physicalSource ?? '',
            },
        });
        auditionPhase = 'recording';
        startRecordTimer();
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

async function auditionStopRecord() {
    if (auditionBusy) return;
    auditionBusy = true;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_record_stop',
            params: { channel_uuid: channelUuid },
        });
        auditionPhase = 'idle';
        auditionHasSample = true;
        stopRecordTimer();
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

async function auditionPlay() {
    if (auditionBusy) return;
    auditionBusy = true;
    auditionError = null;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_loop_start',
            params: { channel_uuid: channelUuid },
        });
        auditionPhase = 'looping';
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

async function auditionStop() {
    if (auditionBusy) return;
    auditionBusy = true;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_loop_stop',
            params: { channel_uuid: channelUuid },
        });
        auditionPhase = 'idle';
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

async function auditionPause() {
    if (auditionBusy) return;
    auditionBusy = true;
    auditionError = null;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_loop_pause',
            params: { channel_uuid: channelUuid },
        });
        auditionPhase = 'paused';
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

async function auditionLoadFile() {
    if (auditionBusy) return;
    auditionError = null;
    let picked: string | null = null;
    try {
        picked = await openDialog({
            multiple: false,
            directory: false,
            filters: [{ name: 'Audio', extensions: ['wav', 'flac', 'ogg', 'mp3'] }],
        });
    } catch (e) {
        auditionError = String(e);
        return;
    }
    if (!picked) return;
    auditionBusy = true;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_load_file',
            params: { channel_uuid: channelUuid, path: picked },
        });
        auditionPhase = 'idle';
        auditionHasSample = true;
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

async function auditionSaveRecording() {
    if (auditionBusy || !auditionHasSample) return;
    auditionError = null;
    let dest: string | null = null;
    const safeName = (channelName ?? 'channel').replace(/[^A-Za-z0-9._-]+/g, '_');
    try {
        dest = await saveDialog({
            defaultPath: `tideline-audition-${safeName}.wav`,
            filters: [{ name: 'WAV', extensions: ['wav'] }],
        });
    } catch (e) {
        auditionError = String(e);
        return;
    }
    if (!dest) return;
    auditionBusy = true;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_save_recording',
            params: { channel_uuid: channelUuid, path: dest },
        });
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

async function auditionResume() {
    if (auditionBusy) return;
    auditionBusy = true;
    auditionError = null;
    try {
        await invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_loop_resume',
            params: { channel_uuid: channelUuid },
        });
        auditionPhase = 'looping';
    } catch (e) {
        auditionError = String(e);
    } finally {
        auditionBusy = false;
    }
}

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
                    value: e.value && 'value' in e.value
                        ? (e.value as { value: unknown }).value
                        : e.value && e.value.type !== 'click'
                            ? e.value
                            : null,
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
    void pullDspState();
    if (auditionVisible) {
        await pullAuditionState();
    }
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
    stopRecordTimer();
    if (auditionVisible) {
        // Fire-and-forget cleanup so external unmounts (rackOpen = null
        // from the parent without going through persistAndClose) cant
        // leave a loop running or a record proc dangling.
        void invoke('tideline_plugin_request', {
            pluginId,
            method: 'effects.audition_discard',
            params: { channel_uuid: channelUuid },
        }).catch(() => { /* best-effort */ });
    }
});

async function persistAndClose() {
    // Synchronous-ish cleanup before close so the user gets immediate
    // feedback that audio stopped. onDestroy also fires audition_discard
    // as a backstop for paths that bypass this function.
    if (auditionVisible && auditionPhase !== 'idle') {
        try {
            await invoke('tideline_plugin_request', {
                pluginId,
                method: 'effects.audition_discard',
                params: { channel_uuid: channelUuid },
            });
        } catch { /* best-effort */ }
    }
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

        <div class="px-5 py-2 border-b border-base-content/10 bg-base-200/40 flex items-center gap-2 shrink-0">
            <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">
                Cleanup
            </span>
            <div class="flex items-center gap-2 flex-1">
                <button
                    type="button"
                    class="btn btn-xs {lowcutOn ? 'btn-primary' : 'btn-ghost'}"
                    onclick={() => void toggleLowcut()}
                    disabled={dspBusy}
                    title="80Hz high-pass filter, runs before the FX chain — removes rumble and handling noise"
                    aria-pressed={lowcutOn}
                >
                    <Icon name="audio-lines" size={12} />
                    Lowcut
                    <span class="text-[10px] opacity-70 tabular-nums">80Hz</span>
                </button>
                <button
                    type="button"
                    class="btn btn-xs {clipguardOn ? 'btn-primary' : 'btn-ghost'}"
                    onclick={() => void toggleClipguard()}
                    disabled={dspBusy}
                    title="Soft-clip ceiling at -1dBFS, runs after the FX chain — catches any plugin pushing signal too hot"
                    aria-pressed={clipguardOn}
                >
                    <Icon name="shield-check" size={12} />
                    Clipguard
                    <span class="text-[10px] opacity-70 tabular-nums">-1dBFS</span>
                </button>
                <span class="text-[11px] text-base-content/45 ml-1">
                    {#if !lowcutOn && !clipguardOn}
                        helps with rumble and clipping
                    {:else if lowcutOn && clipguardOn}
                        rumble cut, ceiling armed
                    {:else if lowcutOn}
                        rumble cut active
                    {:else}
                        ceiling armed
                    {/if}
                </span>
            </div>
        </div>

        {#if auditionVisible}
            <div class="px-5 py-2 border-b border-base-content/10 bg-base-200/40 flex items-center gap-2 shrink-0">
                <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">
                    Audition
                </span>
                <div class="flex items-center gap-2 flex-1">
                    {#if auditionPhase === 'idle'}
                        <button
                            type="button"
                            class="btn btn-xs btn-error"
                            onclick={() => void auditionRecord()}
                            disabled={auditionBusy}
                            title="Record a short voice sample"
                        >
                            <span class="size-2 rounded-full bg-error-content/90 animate-none"></span>
                            {auditionHasSample ? 'Re-record' : 'Record'}
                        </button>
                        <button
                            type="button"
                            class="btn btn-xs btn-ghost"
                            onclick={() => void auditionLoadFile()}
                            disabled={auditionBusy}
                            title="Load an audio file (wav/flac/ogg/mp3) to audition through the chain"
                        >
                            <Icon name="folder" size={12} />
                            Load file
                        </button>
                        {#if auditionHasSample}
                            <button
                                type="button"
                                class="btn btn-xs btn-primary"
                                onclick={() => void auditionPlay()}
                                disabled={auditionBusy}
                                title="Loop the sample through the FX chain"
                            >
                                <Icon name="play" size={12} />
                                Play loop
                            </button>
                            <button
                                type="button"
                                class="btn btn-xs btn-ghost"
                                onclick={() => void auditionSaveRecording()}
                                disabled={auditionBusy}
                                title="Save a copy of the recorded sample"
                            >
                                <Icon name="download" size={12} />
                                Save
                            </button>
                            <span class="text-[11px] text-base-content/55">sample ready</span>
                        {:else}
                            <span class="text-[11px] text-base-content/45">
                                record or load a sample to audition effects without talking
                            </span>
                        {/if}
                    {:else if auditionPhase === 'recording'}
                        <button
                            type="button"
                            class="btn btn-xs btn-warning"
                            onclick={() => void auditionStopRecord()}
                            disabled={auditionBusy}
                        >
                            <span class="size-2 rounded-full bg-error animate-pulse"></span>
                            Stop
                        </button>
                        <span class="text-[11px] tabular-nums text-base-content/70">
                            recording {recordingElapsed.toFixed(1)}s
                        </span>
                    {:else if auditionPhase === 'looping'}
                        <button
                            type="button"
                            class="btn btn-xs btn-primary"
                            onclick={() => void auditionPause()}
                            disabled={auditionBusy}
                            title="Pause playback (keep routing)"
                        >
                            <Icon name="square" size={12} />
                            Pause
                        </button>
                        <button
                            type="button"
                            class="btn btn-xs btn-ghost"
                            onclick={() => void auditionStop()}
                            disabled={auditionBusy}
                            title="Stop loop and tear down audition routing"
                        >
                            <Icon name="close" size={12} />
                            Stop
                        </button>
                        <span class="text-[11px] text-primary inline-flex items-center gap-1">
                            <span class="size-1.5 rounded-full bg-primary animate-pulse"></span>
                            looping through chain
                        </span>
                        <button
                            type="button"
                            class="btn btn-xs btn-ghost ml-auto"
                            onclick={async () => { await auditionStop(); await auditionRecord(); }}
                            disabled={auditionBusy}
                            title="Stop loop and record a new sample"
                        >
                            Re-record
                        </button>
                    {:else}
                        <button
                            type="button"
                            class="btn btn-xs btn-primary"
                            onclick={() => void auditionResume()}
                            disabled={auditionBusy}
                            title="Resume playback"
                        >
                            <Icon name="play" size={12} />
                            Resume
                        </button>
                        <button
                            type="button"
                            class="btn btn-xs btn-ghost"
                            onclick={() => void auditionStop()}
                            disabled={auditionBusy}
                            title="Stop loop and tear down audition routing"
                        >
                            <Icon name="close" size={12} />
                            Stop
                        </button>
                        <span class="text-[11px] text-base-content/60 inline-flex items-center gap-1">
                            <span class="size-1.5 rounded-full bg-base-content/40"></span>
                            paused
                        </span>
                        <button
                            type="button"
                            class="btn btn-xs btn-ghost ml-auto"
                            onclick={async () => { await auditionStop(); await auditionRecord(); }}
                            disabled={auditionBusy}
                            title="Stop loop and record a new sample"
                        >
                            Re-record
                        </button>
                    {/if}
                </div>
                {#if auditionError}
                    <span class="text-[11px] text-error truncate max-w-[40%]" title={auditionError}>
                        {auditionError}
                    </span>
                {/if}
            </div>
        {/if}

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
