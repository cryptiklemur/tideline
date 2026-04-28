<script lang="ts">
import Toast from './Toast.svelte';
import { toaster } from './toaster.svelte';
</script>

<div
    class="fixed top-3 right-3 z-[100] flex flex-col gap-2 w-[340px] max-w-[calc(100vw-1.5rem)] pointer-events-none"
    aria-live="polite"
    aria-atomic="false"
>
    {#each toaster.items as t (t.id)}
        <Toast id={t.id} kind={t.kind} title={t.title} body={t.body} timeoutMs={t.timeoutMs} />
    {/each}
</div>
