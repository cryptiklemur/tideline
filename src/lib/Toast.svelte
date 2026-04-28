<script lang="ts">
import { onMount } from 'svelte';
import Icon, { type IconName } from './Icon.svelte';
import { toaster, type ToastKind } from './toaster.svelte';

interface Props {
    id: number;
    kind: ToastKind;
    title: string;
    body?: string;
    timeoutMs: number;
}

let { id, kind, title, body, timeoutMs }: Props = $props();

let timer: ReturnType<typeof setTimeout> | null = null;
let progress = $state(100);
let progressTimer: ReturnType<typeof setInterval> | null = null;
let startedAt = 0;
let pausedAt = 0;
let elapsed = 0;

function dismiss() {
    if (timer) { clearTimeout(timer); timer = null; }
    if (progressTimer) { clearInterval(progressTimer); progressTimer = null; }
    toaster.dismiss(id);
}

function start() {
    if (timeoutMs <= 0) return;
    startedAt = performance.now();
    const remaining = timeoutMs - elapsed;
    timer = setTimeout(dismiss, remaining);
    progressTimer = setInterval(() => {
        const ran = elapsed + (performance.now() - startedAt);
        progress = Math.max(0, 100 - (ran / timeoutMs) * 100);
    }, 50);
}

function pause() {
    if (timer) { clearTimeout(timer); timer = null; }
    if (progressTimer) { clearInterval(progressTimer); progressTimer = null; }
    pausedAt = performance.now();
    elapsed += pausedAt - startedAt;
}

onMount(() => {
    start();
    return () => {
        if (timer) clearTimeout(timer);
        if (progressTimer) clearInterval(progressTimer);
    };
});

const ICON_BY_KIND: Record<ToastKind, IconName> = {
    info: 'info',
    success: 'check',
    warning: 'alert',
    error: 'alert',
};

const STYLE_BY_KIND: Record<ToastKind, { border: string; bg: string; accent: string; bar: string }> = {
    info:    { border: 'border-info/40',    bg: 'bg-info/10',    accent: 'text-info',    bar: 'bg-info' },
    success: { border: 'border-success/40', bg: 'bg-success/10', accent: 'text-success', bar: 'bg-success' },
    warning: { border: 'border-warning/40', bg: 'bg-warning/10', accent: 'text-warning', bar: 'bg-warning' },
    error:   { border: 'border-error/40',   bg: 'bg-error/10',   accent: 'text-error',   bar: 'bg-error' },
};

const s = $derived(STYLE_BY_KIND[kind]);
</script>

<div
    class="pointer-events-auto rounded-md border shadow-lg overflow-hidden bg-base-200 {s.border}"
    role="status"
    onmouseenter={pause}
    onmouseleave={start}
>
    <div class="flex items-start gap-2.5 px-3 py-2.5 {s.bg}">
        <span class="mt-0.5 flex-shrink-0 {s.accent}" aria-hidden="true">
            <Icon name={ICON_BY_KIND[kind]} size={14} />
        </span>
        <div class="flex-1 min-w-0">
            <p class="text-sm font-semibold m-0 leading-snug">{title}</p>
            {#if body}
                <p class="text-xs text-base-content/70 m-0 mt-0.5 leading-snug whitespace-pre-line">{body}</p>
            {/if}
        </div>
        <button
            type="button"
            class="flex-shrink-0 -mt-0.5 -mr-1 w-6 h-6 flex items-center justify-center rounded text-base-content/55 hover:bg-base-content/10 hover:text-base-content transition-colors cursor-pointer"
            onclick={dismiss}
            aria-label="Dismiss"
        >
            <Icon name="close" size={11} />
        </button>
    </div>
    {#if timeoutMs > 0}
        <div class="h-0.5 {s.bar} transition-[width] duration-100 ease-linear" style="width: {progress}%" aria-hidden="true"></div>
    {/if}
</div>
