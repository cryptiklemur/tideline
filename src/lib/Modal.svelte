<script lang="ts">
import { onMount, onDestroy } from 'svelte';
import type { Snippet } from 'svelte';

interface Props {
    label: string;
    maxWidth?: string;
    onClose: () => void;
    children: Snippet;
}

let { label, maxWidth = '520px', onClose, children }: Props = $props();

let modalEl = $state<HTMLDivElement>();
let triggerEl: HTMLElement | null = null;

onMount(() => {
    triggerEl = (document.activeElement as HTMLElement) ?? null;
    queueMicrotask(() => {
        const first = modalEl?.querySelector<HTMLElement>(
            'select, input, textarea, button:not([data-modal-close])'
        );
        (first ?? modalEl)?.focus();
    });
});

onDestroy(() => {
    triggerEl?.focus();
    triggerEl = null;
});

function onWindowKey(e: KeyboardEvent) {
    if (e.key === 'Escape') {
        e.preventDefault();
        onClose();
    }
}

function onModalKey(e: KeyboardEvent) {
    e.stopPropagation();
    if (e.key !== 'Tab' || !modalEl) return;
    const focusable = modalEl.querySelectorAll<HTMLElement>(
        'button:not([disabled]), select:not([disabled]), input:not([disabled]), textarea:not([disabled]), [tabindex]:not([tabindex="-1"])'
    );
    if (focusable.length === 0) return;
    const first = focusable[0];
    const last = focusable[focusable.length - 1];
    if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
    } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
    }
}
</script>

<svelte:window onkeydown={onWindowKey} />

<div
    class="fixed inset-0 z-50 flex items-center justify-center p-3 bg-[var(--color-modal-scrim)] backdrop-blur-sm"
    onclick={onClose}
    role="presentation"
>
    <div
        class="w-full max-h-[90vh] overflow-auto flex flex-col bg-base-200 border border-base-content/20 rounded-lg shadow-2xl"
        bind:this={modalEl}
        onclick={(e) => e.stopPropagation()}
        onkeydown={onModalKey}
        role="dialog"
        aria-modal="true"
        aria-label={label}
        tabindex="-1"
        style="max-width: {maxWidth}"
    >
        {@render children()}
    </div>
</div>
