<script lang="ts">
import Icon from './Icon.svelte';
import type { OutputMode } from './types';

interface Props {
    mode: OutputMode;
    onModeChange: (m: OutputMode) => void;
    onSettings: () => void;
}

let { mode, onModeChange, onSettings }: Props = $props();

const options: { value: OutputMode; label: string; title: string; icon: 'headphones' | 'speaker' | 'split' }[] = [
    { value: 'headphones', label: 'Headphones', title: 'Route audio to headphones only',          icon: 'headphones' },
    { value: 'speakers',   label: 'Speakers',   title: 'Route audio to speakers only',            icon: 'speaker'    },
    { value: 'both',       label: 'HP + SP',    title: 'Route audio to headphones and speakers',  icon: 'split'      },
];
</script>

<div class="flex items-center gap-2 px-3 py-2 bg-base-200 border-t border-base-content/10 flex-shrink-0">
    <div class="join flex-1 border border-base-content/15 bg-base-100 overflow-hidden" role="group" aria-label="Output mode">
        {#each options as opt}
            <button
                class="join-item flex-1 flex items-center justify-center gap-2 py-2.5 px-3 border-none text-sm font-semibold cursor-pointer transition-colors relative
                       {mode === opt.value
                         ? 'bg-primary/15 text-primary shadow-[inset_0_-2px_0_var(--color-primary)]'
                         : 'bg-transparent text-base-content/70 hover:bg-base-content/5 hover:text-base-content'}
                       [&+button]:border-l [&+button]:border-l-base-content/15"
                aria-pressed={mode === opt.value}
                onclick={() => onModeChange(opt.value)}
                title={opt.title}
            >
                <Icon name={opt.icon} size={14} />
                <span class="tracking-wide">{opt.label}</span>
            </button>
        {/each}
    </div>
    <button
        class="settings-btn w-8 h-8 p-0 border border-base-content/15 rounded-md bg-base-100 text-base-content/70 cursor-pointer flex items-center justify-center flex-shrink-0 transition-all hover:bg-base-content/5 hover:text-base-content hover:border-base-content/25 hover:rotate-45"
        onclick={onSettings}
        aria-label="Open settings"
        title="Settings"
    ><Icon name="settings" size={16} /></button>
</div>
