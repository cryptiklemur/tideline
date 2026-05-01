<script lang="ts">
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'banner' }>; }
let { node }: Props = $props();
let cls = $derived(
    node.tone === 'success' ? 'alert alert-success alert-soft' :
    node.tone === 'warning' ? 'alert alert-warning alert-soft' :
    node.tone === 'error' ? 'alert alert-error alert-soft' :
    'alert alert-info alert-soft'
);
</script>
<div class="{cls} py-2"><span class="text-sm">{node.text}</span></div>
