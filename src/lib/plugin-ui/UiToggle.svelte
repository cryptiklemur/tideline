<script lang="ts">
import Icon from '$lib/Icon.svelte';
import type { UiEvent, UiNode } from './types';
interface Props {
    node: Extract<UiNode, { kind: 'toggle' }>;
    surfaceId: string;
    emit: (e: UiEvent) => void;
}
let { node, surfaceId, emit }: Props = $props();

let style = $derived(node.style ?? 'checkbox');
let isSwitch = $derived(style === 'switch');
let isPower = $derived(style === 'power');

let sizeClass = $derived(
    node.size === 'lg' ? (isSwitch ? 'toggle-lg' : 'checkbox-lg')
    : node.size === 'md' ? (isSwitch ? 'toggle-md' : 'checkbox-md')
    : (isSwitch ? 'toggle-sm' : 'checkbox-sm')
);
let inputClass = $derived(
    isSwitch
        ? `toggle ${sizeClass} toggle-primary shrink-0`
        : `checkbox ${sizeClass} checkbox-primary shrink-0 border border-base-content/40`
);

let powerSize = $derived(node.size === 'lg' ? 44 : node.size === 'md' ? 36 : 28);
let powerIconSize = $derived(node.size === 'lg' ? 20 : node.size === 'md' ? 16 : 14);

let localChecked = $state(false);
$effect(() => { localChecked = node.value; });

function emitValue(v: boolean) {
    emit({
        surface_id: surfaceId,
        node_id: node.id,
        value: { type: 'bool', value: v },
    });
}

function onChange() {
    emitValue(localChecked);
}

function togglePower() {
    localChecked = !localChecked;
    emitValue(localChecked);
}

let tooltipWrapper = $derived(node.tooltip ? 'tooltip tooltip-right' : '');
</script>

{#if isPower}
    <div class={tooltipWrapper} data-tip={node.tooltip ?? undefined}>
        <button
            type="button"
            aria-pressed={localChecked}
            aria-label={node.label ?? 'Toggle'}
            onclick={togglePower}
            class="power-btn"
            class:is-on={localChecked}
            style:--btn-size="{powerSize}px"
        >
            <span class="power-glow" aria-hidden="true"></span>
            <Icon name="power" size={powerIconSize} />
        </button>
    </div>
{:else}
    <label class="flex items-center gap-3 cursor-pointer w-fit">
        <input
            type="checkbox"
            class={inputClass}
            bind:checked={localChecked}
            onchange={onChange}
        />
        {#if node.label}<span class="text-sm">{node.label}</span>{/if}
    </label>
{/if}

<style>
.power-btn {
    position: relative;
    width: var(--btn-size);
    height: var(--btn-size);
    border-radius: 9999px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
    cursor: pointer;
    border: 1px solid color-mix(in oklab, currentColor 18%, transparent);
    background:
        radial-gradient(circle at 50% 30%,
            color-mix(in oklab, var(--color-base-100) 90%, transparent) 0%,
            color-mix(in oklab, var(--color-base-300) 100%, transparent) 100%);
    color: color-mix(in oklab, currentColor 45%, transparent);
    box-shadow:
        inset 0 1px 0 color-mix(in oklab, white 8%, transparent),
        inset 0 -2px 4px color-mix(in oklab, black 25%, transparent),
        0 1px 2px color-mix(in oklab, black 20%, transparent);
    transition: color 160ms ease, box-shadow 220ms ease, transform 120ms ease, border-color 160ms ease;
}
.power-btn:hover {
    color: color-mix(in oklab, currentColor 70%, transparent);
    border-color: color-mix(in oklab, currentColor 30%, transparent);
}
.power-btn:active {
    transform: scale(0.96);
}
.power-btn:focus-visible {
    outline: 2px solid var(--color-primary);
    outline-offset: 2px;
}
.power-btn.is-on {
    color: var(--color-primary);
    border-color: color-mix(in oklab, var(--color-primary) 55%, transparent);
    background:
        radial-gradient(circle at 50% 30%,
            color-mix(in oklab, var(--color-primary) 22%, var(--color-base-100)) 0%,
            color-mix(in oklab, var(--color-primary) 8%, var(--color-base-300)) 100%);
    box-shadow:
        inset 0 1px 0 color-mix(in oklab, white 14%, transparent),
        inset 0 -2px 6px color-mix(in oklab, var(--color-primary) 35%, transparent),
        0 0 0 1px color-mix(in oklab, var(--color-primary) 30%, transparent),
        0 0 18px color-mix(in oklab, var(--color-primary) 45%, transparent);
}
.power-glow {
    position: absolute;
    inset: -6px;
    border-radius: 9999px;
    background: radial-gradient(circle, color-mix(in oklab, var(--color-primary) 35%, transparent) 0%, transparent 65%);
    opacity: 0;
    transition: opacity 240ms ease;
    pointer-events: none;
}
.power-btn.is-on .power-glow { opacity: 1; }
</style>
