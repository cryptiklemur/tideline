<script lang="ts">
import type { ChannelOverlayContribution, UiEvent } from './types';
import ChannelOverlay from './ChannelOverlay.svelte';

interface Props {
    overlay: ChannelOverlayContribution;
    emit: (e: UiEvent) => void;
}
let { overlay, emit }: Props = $props();
</script>

<ChannelOverlay {overlay} {emit} />
