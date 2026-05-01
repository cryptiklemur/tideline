<script lang="ts">
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'label' }>; }
let { node }: Props = $props();
</script>
<span class="text-sm {node.muted ? 'text-base-content/55' : ''}">{node.text}</span>
