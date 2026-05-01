<script lang="ts">
import { listen, type UnlistenFn } from '@tauri-apps/api/event';
import { onDestroy, type Snippet } from 'svelte';
import Icon from './Icon.svelte';
import HFader from './HFader.svelte';
import { portal } from './portal';
import type { SinkInfo } from './types';

interface Props {
    out: SinkInfo;
    isActive: boolean;
    onSelect: (name: string) => void;
    onToggleMute: (out: SinkInfo) => void;
    onSetVolume: (out: SinkInfo, vol: number) => void;
    extra?: Snippet;
}

let { out, isActive, onSelect, onToggleMute, onSetVolume, extra }: Props = $props();

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

let meterSource = $derived(`${out.name}.monitor`);

let rowEl: HTMLElement | undefined = $state();
let popoverEl: HTMLElement | undefined = $state();
let selectBtnEl: HTMLButtonElement | undefined = $state();
let hovered = $state(false);
let popoverTop = $state(0);
let popoverLeft = $state(0);
let hideTimer: ReturnType<typeof setTimeout> | null = null;

function focusFirstInPopover() {
    const slider = popoverEl?.querySelector<HTMLElement>('[role="slider"]');
    slider?.focus();
}

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

function closeNow() {
    if (hideTimer) { clearTimeout(hideTimer); hideTimer = null; }
    hovered = false;
    rowEl?.querySelector<HTMLElement>('button')?.focus();
}

$effect(() => {
    if (out.muted) return;
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

onDestroy(() => {
    if (hideTimer !== null) clearTimeout(hideTimer);
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
        onclick={() => onSelect(out.name)}
        onkeydown={onRowKeyDown}
        title={out.name}
    >
        <span class="nav-icon flex items-center justify-center flex-shrink-0">
            <Icon name="speaker" size={14} />
        </span>
        <div class="flex flex-col min-w-0 flex-1 gap-1">
            <span class="overflow-hidden text-ellipsis whitespace-nowrap leading-tight">{out.description}</span>
            {#if !out.muted}
                <div class="relative h-1 rounded-sm overflow-hidden bg-base-300" aria-hidden="true">
                    <div class="absolute inset-y-0 left-0 bg-primary/40" style:width="{out.volume_percent}%"></div>
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
               {out.muted ? 'text-error' : 'text-base-content/55 hover:text-base-content'}"
        onclick={(e) => { e.stopPropagation(); onToggleMute(out); }}
        aria-label="{out.description} mute"
        aria-pressed={out.muted}
        title={out.muted ? 'Unmute' : 'Mute'}
    >
        <Icon name={out.muted ? 'volume-mute' : 'volume'} size={12} />
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
                <span class="text-sm font-semibold text-base-content overflow-hidden text-ellipsis whitespace-nowrap" title={out.description}>{out.description}</span>
                <span class="font-mono text-[10px] text-base-content/55 overflow-hidden text-ellipsis whitespace-nowrap" title={out.name}>{out.name}</span>
            </div>

            <div class="flex flex-col gap-1">
                <span class="text-[10px] font-bold uppercase tracking-widest text-base-content/55">Volume</span>
                <HFader
                    label="{out.description} volume"
                    value={out.volume_percent}
                    valueLabel={`${out.volume_percent}%`}
                    disabled={out.muted}
                    onchange={(v) => onSetVolume(out, v)}
                />
            </div>

            <button
                class="w-full py-1.5 border rounded-md text-[10px] font-bold tracking-widest cursor-pointer transition-colors
                       {out.muted ? 'bg-error border-error text-error-content' : 'bg-base-100 border-base-content/15 text-base-content/70 hover:bg-base-content/5 hover:text-base-content hover:border-base-content/25'}"
                aria-label="{out.description} mute"
                aria-pressed={out.muted}
                onclick={() => onToggleMute(out)}
            >{out.muted ? 'MUTED' : 'MUTE'}</button>
        </div>
    </div>
{/if}
