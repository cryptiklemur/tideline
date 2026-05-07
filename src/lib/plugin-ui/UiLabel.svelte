<script lang="ts">
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'label' }>; }
let { node }: Props = $props();

let mutedClass = $derived(node.muted ? 'text-base-content/55' : '');
let hintClass = $derived(
    node.hint === 'dotted' ? 'underline decoration-dotted decoration-base-content/40 underline-offset-4 cursor-help'
    : ''
);
let placement = $derived(node.tooltip_placement ?? 'right');
let wrapperClass = $derived(
    node.tooltip
        ? placement === 'top' ? 'tooltip tooltip-top'
        : placement === 'left' ? 'tooltip tooltip-left'
        : placement === 'bottom' ? 'tooltip tooltip-bottom'
        : 'tooltip tooltip-right'
        : ''
);
</script>

{#if node.tooltip}
    <span class={wrapperClass} data-tip={node.tooltip}>
        <span class="text-sm {mutedClass} {hintClass}">{node.text}{#if node.hint === 'question'}<sup class="ml-0.5 text-[10px] text-base-content/55">?</sup>{/if}</span>
    </span>
{:else}
    <span class="text-sm {mutedClass} {hintClass}">{node.text}{#if node.hint === 'question'}<sup class="ml-0.5 text-[10px] text-base-content/55">?</sup>{/if}</span>
{/if}
