<script lang="ts">
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'spacer' }>; }
let { node }: Props = $props();
let dim = $derived(node.size ? `${node.size}px` : undefined);
</script>
{#if dim}
    <div style:height={dim} style:width={dim} aria-hidden="true"></div>
{:else}
    <div class="flex-1 self-stretch" aria-hidden="true"></div>
{/if}
