<script lang="ts">
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'heading' }>; }
let { node }: Props = $props();
let level = $derived(node.level ?? 3);
</script>
{#if level === 3}
    <h3 class="text-base font-semibold m-0">{node.text}</h3>
{:else if level === 4}
    <h4 class="text-sm font-semibold m-0">{node.text}</h4>
{:else}
    <h5 class="text-xs font-semibold m-0">{node.text}</h5>
{/if}
