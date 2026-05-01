<script lang="ts">
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'badge' }>; }
let { node }: Props = $props();
let cls = $derived(
    node.variant === 'primary' ? 'badge badge-primary badge-soft' :
    node.variant === 'success' ? 'badge badge-success badge-soft' :
    node.variant === 'warning' ? 'badge badge-warning badge-soft' :
    node.variant === 'error' ? 'badge badge-error badge-soft' :
    node.variant === 'info' ? 'badge badge-info badge-soft' :
    'badge badge-soft'
);
</script>
<span class="{cls} px-2 py-0.5 text-xs">{node.text}</span>
