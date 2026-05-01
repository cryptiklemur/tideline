<script lang="ts">
import Icon, { ICONS, type IconName } from '$lib/Icon.svelte';
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'icon' }>; }
let { node }: Props = $props();
let known = $derived(node.icon.name in ICONS);
let size = $derived(node.size ?? 14);
</script>

{#if known}
    <Icon name={node.icon.name as IconName} size={size} />
{:else}
    <span data-testid="ui-icon-fallback" class="inline-block bg-base-content/30 rounded" style:width="{size}px" style:height="{size}px"></span>
{/if}
