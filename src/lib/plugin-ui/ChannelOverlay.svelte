<script lang="ts">
import type { ChannelOverlayContribution, UiEvent } from './types';
import UiTree from './UiTree.svelte';

interface Props {
    overlay: ChannelOverlayContribution;
    emit: (e: UiEvent) => void;
}
let { overlay, emit }: Props = $props();
</script>

<div
    class="flex flex-col gap-1"
    data-overlay-placement={overlay.placement}
    data-plugin-id={overlay.plugin_id}
    data-surface-id={overlay.surface_id}
>
    <UiTree node={overlay.tree} surfaceId={overlay.surface_id} pluginId={overlay.plugin_id} {emit} />
</div>
