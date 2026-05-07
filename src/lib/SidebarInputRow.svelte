<script lang="ts">
import { invoke } from '@tauri-apps/api/core';
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onMount, onDestroy, type Snippet } from 'svelte';
import Icon from './Icon.svelte';
import HFader from './HFader.svelte';
import { portal } from './portal';
import type { CardControl, ChannelConfig } from './types';
import { pluginUi } from './plugin-ui/pluginUi.svelte';

interface Props {
    inp: ChannelConfig;
    isActive: boolean;
    meterSource: string;
    sinkName: string;
    onSelect: (name: string) => void;
    extra?: Snippet;
}

let { inp, isActive, meterSource, sinkName, onSelect, extra }: Props = $props();

const METER_FLOOR_DB = -60;
const PEAK_HOLD_MS = 1200;
const PEAK_FALL_PER_SEC = 0.4;
const RMS_FALL_PER_SEC = 1.6;

function ampToBar(amp: number): number {
    if (amp <= 0) return 0;
    const db = 20 * Math.log10(amp);
    if (db <= METER_FLOOR_DB) return 0;
    if (db >= 0) return 1;
    return (db - METER_FLOOR_DB) / -METER_FLOOR_DB;
}

let displayLevel = $state(0);
let peakHold = $state(0);
let lastPeakAt = 0;

let vol = $state(100);
let muted = $state(false);
let cardId = $state<number | null>(null);
let cardControls = $state<CardControl[]>([]);

let primaryCapture = $derived(
    inp.kind === 'physical_input'
        ? cardControls.find(c => c.is_capture && c.has_volume) ?? null
        : null
);
let gainDb = $derived(primaryCapture?.current_db ?? null);

function formatDb(db: number): string {
    const sign = db > 0 ? '+' : '';
    return `${sign}${db.toFixed(1)} dB`;
}

let rowEl: HTMLElement | undefined = $state();
let popoverEl: HTMLElement | undefined = $state();
let selectBtnEl: HTMLButtonElement | undefined = $state();
let hovered = $state(false);
let popoverTop = $state(0);
let popoverLeft = $state(0);
let hideTimer: ReturnType<typeof setTimeout> | null = null;

function onRowKeyDown(e: KeyboardEvent) {
    if (e.key !== 'Tab' || e.shiftKey) return;
    if (!hovered) return;
    const slider = popoverEl?.querySelector<HTMLElement>('[role="slider"]');
    if (!slider) return;
    e.preventDefault();
    slider.focus();
}

function onPopoverKeyDown(e: KeyboardEvent) {
    if (e.key !== 'Tab' || !e.shiftKey) return;
    const slider = popoverEl?.querySelector<HTMLElement>('[role="slider"]');
    if (e.target !== slider) return;
    e.preventDefault();
    selectBtnEl?.focus();
}

function computePos() {
    if (!rowEl) return;
    const rect = rowEl.getBoundingClientRect();
    popoverTop = rect.top + rect.height / 2;
    popoverLeft = rect.right + 8;
}

function showPopover() {
    if (hideTimer) { clearTimeout(hideTimer); hideTimer = null; }
    computePos();
    hovered = true;
}

function scheduleHide() {
    if (hideTimer) clearTimeout(hideTimer);
    hideTimer = setTimeout(() => { hovered = false; }, 120);
}

async function refreshState() {
    try {
        if (inp.kind === 'physical_input' && inp.physical_source) {
            const s = await invoke<[number, boolean] | null>('get_source_state', { name: inp.physical_source });
            if (s) { vol = s[0]; muted = s[1]; }
            if (cardId === null) {
                cardId = await invoke<number | null>('get_card_for_source', { source: inp.physical_source });
            }
            if (cardId !== null) {
                cardControls = await invoke<CardControl[]>('list_card_controls', { card: cardId });
                const primary = cardControls.find(c => c.is_capture && c.has_volume);
                if (primary) vol = primary.volume_percent;
            }
        } else if (sinkName) {
            const s = await invoke<[number, boolean] | null>('get_sink_state', { name: sinkName });
            if (s) { vol = s[0]; muted = s[1]; }
        }
    } catch { /* ignore */ }
}

function closeNow() {
    if (hideTimer) { clearTimeout(hideTimer); hideTimer = null; }
    hovered = false;
    rowEl?.querySelector<HTMLElement>('button')?.focus();
}

$effect(() => {
    if (muted) return;
    const eventName = `level:${meterSource.replace(/\./g, '_')}`;
    let unlisten: UnlistenFn | null = null;
    let cancelled = false;
    listen<{ peak: number; rms: number }>(eventName, e => {
        const rmsBar = ampToBar(e.payload.rms);
        const peakBar = ampToBar(e.payload.peak);
        if (rmsBar > displayLevel) displayLevel = rmsBar;
        if (peakBar >= peakHold) {
            peakHold = peakBar;
            lastPeakAt = performance.now();
        }
    }).then(fn => {
        if (cancelled) { fn(); return; }
        unlisten = fn;
    });
    let prev = performance.now();
    let raf: number | null = null;
    const tick = (now: number) => {
        const dt = Math.min(0.1, (now - prev) / 1000);
        prev = now;
        if (displayLevel > 0) displayLevel = Math.max(0, displayLevel - RMS_FALL_PER_SEC * dt);
        if (now - lastPeakAt > PEAK_HOLD_MS) {
            peakHold = Math.max(displayLevel, peakHold - PEAK_FALL_PER_SEC * dt);
        }
        raf = requestAnimationFrame(tick);
    };
    raf = requestAnimationFrame(tick);
    return () => {
        cancelled = true;
        if (unlisten) unlisten();
        if (raf !== null) cancelAnimationFrame(raf);
        displayLevel = 0;
        peakHold = 0;
        lastPeakAt = 0;
    };
});

$effect(() => {
    if (!hovered) return;
    function onKey(e: KeyboardEvent) {
        if (e.key === 'Escape') { e.preventDefault(); closeNow(); }
    }
    function onScrollOrResize() { closeNow(); }
    window.addEventListener('keydown', onKey);
    window.addEventListener('resize', onScrollOrResize);
    document.addEventListener('scroll', onScrollOrResize, true);
    return () => {
        window.removeEventListener('keydown', onKey);
        window.removeEventListener('resize', onScrollOrResize);
        document.removeEventListener('scroll', onScrollOrResize, true);
    };
});

let muteUnlisten: UnlistenFn | null = null;
let volUnlisten: UnlistenFn | null = null;
let cardCtrlUnlisten: UnlistenFn | null = null;

onMount(() => {
    refreshState();
    const muteEvent =
        inp.kind === 'physical_input' ? 'tideline:source_mute_changed' : 'tideline:sink_mute_changed';
    const muteKey = inp.kind === 'physical_input' ? 'source_name' : 'sink_name';
    const targetName = inp.kind === 'physical_input' ? inp.physical_source : sinkName;
    listen<Record<string, unknown>>(muteEvent, e => {
        const payload = e.payload;
        if (!payload || typeof payload !== 'object') return;
        if ((payload as Record<string, unknown>)[muteKey] !== targetName) return;
        const m = (payload as { muted?: unknown }).muted;
        if (typeof m === 'boolean') muted = m;
    }).then(fn => { muteUnlisten = fn; });

    const volEvent =
        inp.kind === 'physical_input' ? 'tideline:source_volume_changed' : 'tideline:sink_volume_changed';
    listen<Record<string, unknown>>(volEvent, e => {
        const payload = e.payload;
        if (!payload || typeof payload !== 'object') return;
        if ((payload as Record<string, unknown>)[muteKey] !== targetName) return;
        const v = (payload as { volume_pct?: unknown }).volume_pct;
        if (typeof v === 'number') {
            // card-control reads override source volume when present, so only
            // overwrite vol if there is no primary capture control.
            if (!primaryCapture) vol = v;
        }
    }).then(fn => { volUnlisten = fn; });

    listen<{ card: number; name: string; volume_pct: number }>('tideline:card_control_volume_changed', e => {
        if (cardId === null || e.payload.card !== cardId) return;
        const idx = cardControls.findIndex(c => c.name === e.payload.name);
        if (idx >= 0) {
            cardControls = cardControls.map((c, i) =>
                i === idx ? { ...c, volume_percent: e.payload.volume_pct } : c
            );
            if (primaryCapture && primaryCapture.name === e.payload.name) {
                vol = e.payload.volume_pct;
            }
        }
    }).then(fn => { cardCtrlUnlisten = fn; });
});

onDestroy(() => {
    if (hideTimer !== null) clearTimeout(hideTimer);
    muteUnlisten?.();
    volUnlisten?.();
    cardCtrlUnlisten?.();
});

async function toggleMute() {
    const prev = muted;
    const next = !muted;
    muted = next;
    try {
        if (inp.kind === 'physical_input' && inp.physical_source) {
            await invoke('set_source_mute', { name: inp.physical_source, muted: next });
        } else if (sinkName) {
            await invoke('set_sink_mute', { name: sinkName, muted: next });
        }
    } catch {
        muted = prev;
    }
}

async function onVolChange(v: number) {
    const prevVol = vol;
    const prevControls = cardControls;
    vol = v;
    try {
        if (inp.kind === 'physical_input') {
            if (primaryCapture && cardId !== null) {
                const name = primaryCapture.name;
                cardControls = cardControls.map(c => c.name === name ? { ...c, volume_percent: v } : c);
                await invoke('set_card_control_volume', { card: cardId, name, pct: v });
            } else if (inp.physical_source) {
                await invoke('set_source_volume', { name: inp.physical_source, pct: v });
            }
        } else if (sinkName) {
            await invoke('set_sink_volume', { name: sinkName, pct: v });
        }
    } catch {
        vol = prevVol;
        cardControls = prevControls;
    }
}

let sourceLabel = $derived(inp.kind === 'physical_input' ? inp.physical_source : sinkName);
let kindLabel = $derived(inp.kind === 'physical_input' ? 'Hardware mic' : 'Virtual mic');

type PttState = { mode?: 'open' | 'ptt'; transmitting?: boolean };
let pttEnabledForRow = $derived(
    inp.kind === 'physical_input' && inp.physical_source
        ? pluginUi.contributions.input_overlays.some(
              (o) =>
                  o.plugin_id === 'tideline-ptt' &&
                  !!o.values_by_source?.[inp.physical_source!]?.['tideline-ptt:input_enabled'],
          )
        : false,
);
let pttState = $derived(
    (pluginUi.pluginEventBySource(
        'tideline-ptt:state_changed',
        inp.kind === 'physical_input' ? (inp.physical_source ?? '') : '',
    ) as PttState | undefined) ?? {},
);
let pttBadge = $derived.by<{ label: string; tone: string } | null>(() => {
    if (!pttEnabledForRow) return null;
    if (pttState.mode === 'ptt') {
        return pttState.transmitting
            ? { label: 'LIVE', tone: 'bg-success/20 text-success border-success/40' }
            : { label: 'PTT', tone: 'bg-error/20 text-error border-error/40' };
    }
    return { label: 'OPEN', tone: 'bg-base-content/10 text-base-content/70 border-base-content/25' };
});
</script>

<li
    bind:this={rowEl}
    class="relative flex items-stretch border-l-2 transition-colors {isActive ? 'border-l-primary bg-primary/15 [&_.nav-icon]:text-primary' : 'border-l-transparent hover:bg-base-content/5'}"
    onmouseenter={showPopover}
    onmouseleave={scheduleHide}
    onfocusin={showPopover}
    onfocusout={scheduleHide}
>
    <button
        bind:this={selectBtnEl}
        class="flex-1 flex items-center gap-2 pl-3 py-1.5 bg-transparent border-none text-sm font-medium text-left cursor-pointer min-w-0
               {isActive ? 'text-base-content' : 'text-base-content/70 hover:text-base-content'}"
        aria-current={isActive ? 'page' : undefined}
        onclick={() => onSelect(inp.name)}
        onkeydown={onRowKeyDown}
        title={inp.name}
    >
        <div class="flex flex-col min-w-0 flex-1 gap-1">
            <span class="flex items-center gap-1.5 overflow-hidden whitespace-nowrap leading-tight">
                <span class="overflow-hidden text-ellipsis min-w-0">{inp.name}</span>
                {#if pttBadge}
                    <span
                        class="flex-shrink-0 px-1 py-px text-[8px] font-bold tracking-widest uppercase border rounded {pttBadge.tone}"
                        title={pttState.mode === 'ptt' ? (pttState.transmitting ? 'PTT live' : 'PTT muted') : 'Open mic'}
                    >{pttBadge.label}</span>
                {/if}
            </span>
            {#if !muted}
                <div class="relative h-1 rounded-sm overflow-hidden bg-base-300" aria-hidden="true">
                    <div class="absolute inset-y-0 left-0 bg-primary/40" style:width="{vol}%"></div>
                    <div class="absolute inset-0 origin-left will-change-transform bg-[linear-gradient(90deg,var(--color-success)_0%,var(--color-success)_70%,var(--color-warning)_75%,var(--color-warning)_90%,var(--color-error)_95%,var(--color-error)_100%)]" style:transform="scaleX({Math.max(0, Math.min(1, displayLevel))})"></div>
                    {#if peakHold > 0.01}
                        <div class="absolute top-0 bottom-0 w-px bg-base-content/80 pointer-events-none" style:left="{Math.min(99.5, Math.max(0, peakHold * 100 - 0.5))}%"></div>
                    {/if}
                </div>
            {/if}
        </div>
    </button>
    <button
        class="w-6 h-6 mx-1 self-center flex-shrink-0 flex items-center justify-center rounded bg-transparent border-none cursor-pointer transition-colors hover:bg-base-300
               {muted ? 'text-error' : 'text-base-content/55 hover:text-base-content'}"
        onclick={(e) => { e.stopPropagation(); toggleMute(); }}
        aria-label="{inp.name} mute"
        aria-pressed={muted}
        title={muted ? 'Unmute' : 'Mute'}
    >
        <Icon name={muted ? 'volume-mute' : 'volume'} size={12} />
    </button>
    {#if extra}<span class="ml-1 self-center flex-shrink-0">{@render extra()}</span>{/if}
</li>

{#if hovered}
    <div
        bind:this={popoverEl}
        use:portal
        class="fixed z-[1000] -translate-y-1/2"
        style:top="{popoverTop}px"
        style:left="{popoverLeft}px"
        onmouseenter={showPopover}
        onmouseleave={scheduleHide}
        onfocusin={showPopover}
        onfocusout={scheduleHide}
        onkeydown={onPopoverKeyDown}
        role="presentation"
    >
        <div class="w-72 p-3 flex flex-col gap-2.5 bg-base-200 border border-base-content/20 rounded-md shadow-2xl">
            <div class="flex flex-col gap-0.5 min-w-0">
                <div class="flex items-center justify-between gap-2 min-w-0">
                    <span class="text-sm font-semibold text-base-content overflow-hidden text-ellipsis whitespace-nowrap min-w-0" title={inp.name}>{inp.name}</span>
                    <span class="text-[9px] uppercase tracking-widest text-base-content/55 flex-shrink-0">{kindLabel}</span>
                </div>
                {#if sourceLabel}
                    <span class="font-mono text-[10px] text-base-content/55 overflow-hidden text-ellipsis whitespace-nowrap" title={sourceLabel}>{sourceLabel}</span>
                {/if}
            </div>

            <div class="flex flex-col gap-1">
                <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">
                    {inp.kind === 'physical_input' ? 'Input gain' : 'Volume'}
                </span>
                <HFader
                    label="{inp.name} volume"
                    value={vol}
                    disabled={muted}
                    valueLabel={gainDb !== null ? formatDb(gainDb) : `${vol}%`}
                    valueText={gainDb !== null ? formatDb(gainDb) : undefined}
                    onchange={onVolChange}
                />
            </div>

            <button
                class="w-full py-1.5 border rounded-md text-[10px] font-bold tracking-widest cursor-pointer transition-colors
                       {muted ? 'bg-error border-error text-error-content' : 'bg-base-100 border-base-content/15 text-base-content/70 hover:bg-base-content/5 hover:text-base-content hover:border-base-content/25'}"
                aria-label="{inp.name} mute"
                aria-pressed={muted}
                onclick={toggleMute}
            >{muted ? 'MUTED' : 'MUTE'}</button>
        </div>
    </div>
{/if}
