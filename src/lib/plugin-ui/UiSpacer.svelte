<script lang="ts">
import type { UiNode } from './types';
interface Props { node: Extract<UiNode, { kind: 'spacer' }>; }
let { node }: Props = $props();
let h = $derived(`${node.size ?? 4}px`);
</script>
<div style:height={h}></div>
