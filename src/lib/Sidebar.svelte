<script lang="ts">
import Icon from './Icon.svelte';
import SidebarOutputRow from './SidebarOutputRow.svelte';
import SidebarInputRow from './SidebarInputRow.svelte';
import type { ChannelConfig, Mix, SinkInfo } from './types';

interface Props {
    inputs: ChannelConfig[];
    outputs: SinkInfo[];
    mixes: Mix[];
    selectedInput: string | null;
    selectedOutput: string | null;
    selectedMix: string | null;
    activeView: 'mixes' | 'input' | 'output' | 'mix';
    onSelectMixes: () => void;
    onSelectInput: (name: string) => void;
    onSelectOutput: (name: string) => void;
    onSelectMix: (id: string) => void;
    onAddInput: () => void;
    onAddMix: () => void;
    onToggleOutputMute: (out: SinkInfo) => void;
    onSetOutputVolume: (out: SinkInfo, vol: number) => void;
    inputMeterSource: (inp: ChannelConfig) => string;
    inputSinkName: (inp: ChannelConfig) => string;
    onSettings: () => void;
}

let {
    inputs,
    outputs,
    mixes,
    selectedInput,
    selectedOutput,
    selectedMix,
    activeView,
    onSelectMixes,
    onSelectInput,
    onSelectOutput,
    onSelectMix,
    onAddInput,
    onAddMix,
    onToggleOutputMute,
    onSetOutputVolume,
    inputMeterSource,
    inputSinkName,
    onSettings,
}: Props = $props();

const itemBase = 'w-full flex items-center gap-2 px-3 py-1.5 bg-transparent border-none border-l-2 text-sm font-medium text-left cursor-pointer transition-colors min-w-0';
const itemIdle = 'border-l-transparent text-base-content/70 hover:bg-base-content/5 hover:text-base-content';
const itemActive = 'border-l-primary bg-primary/15 text-base-content [&_.nav-icon]:text-primary [&_.dot]:bg-primary';
</script>

<aside class="flex flex-col w-[200px] flex-shrink-0 bg-base-200 border-r border-base-content/10" aria-label="Primary navigation">
    <nav class="flex-1 overflow-y-auto py-3 flex flex-col gap-3">
        <div>
            <div class="flex items-center justify-between px-3 mb-1 min-h-[18px]">
                <span class="text-xs font-bold uppercase tracking-widest text-base-content/55">Inputs</span>
                <button
                    class="flex items-center justify-center w-6 h-6 p-0 bg-transparent border border-transparent rounded text-base-content/55 cursor-pointer transition-colors hover:bg-primary/10 hover:text-primary hover:border-primary"
                    onclick={onAddInput}
                    aria-label="Add input"
                    title="Add input"
                >
                    <Icon name="plus" size={11} />
                </button>
            </div>
            {#if inputs.length === 0}
                <p class="mx-3 mb-1 px-2 py-1 text-xs text-base-content/55 leading-snug m-0">No inputs yet.</p>
            {:else}
                <ul class="list-none flex flex-col m-0 p-0">
                    {#each inputs as inp (inp.name)}
                        <SidebarInputRow
                            {inp}
                            isActive={activeView === 'input' && selectedInput === inp.name}
                            meterSource={inputMeterSource(inp)}
                            sinkName={inputSinkName(inp)}
                            onSelect={onSelectInput}
                        />
                    {/each}
                </ul>
            {/if}
        </div>

        <div>
            <div class="flex items-center justify-between px-3 mb-1 min-h-[18px]">
                <span class="text-xs font-bold uppercase tracking-widest text-base-content/55">Outputs</span>
            </div>
            {#if outputs.length === 0}
                <p class="mx-3 mb-1 px-2 py-1 text-xs text-base-content/55 leading-snug m-0">No physical outputs detected.</p>
            {:else}
                <ul class="list-none flex flex-col m-0 p-0">
                    {#each outputs as out (out.name)}
                        <SidebarOutputRow
                            {out}
                            isActive={activeView === 'output' && selectedOutput === out.name}
                            onSelect={onSelectOutput}
                            onToggleMute={onToggleOutputMute}
                            onSetVolume={onSetOutputVolume}
                        />
                    {/each}
                </ul>
            {/if}
        </div>

        <div>
            <div class="flex items-center justify-between px-3 mb-1 min-h-[18px]">
                <span class="text-xs font-bold uppercase tracking-widest text-base-content/55">Mixes</span>
                <button
                    class="flex items-center justify-center w-6 h-6 p-0 bg-transparent border border-transparent rounded text-base-content/55 cursor-pointer transition-colors hover:bg-primary/10 hover:text-primary hover:border-primary"
                    onclick={onAddMix}
                    aria-label="Add mix"
                    title="Add mix"
                >
                    <Icon name="plus" size={11} />
                </button>
            </div>
            <ul class="list-none flex flex-col m-0 p-0">
                <li>
                    <button
                        class="{itemBase} {activeView === 'mixes' ? itemActive : itemIdle}"
                        aria-current={activeView === 'mixes' ? 'page' : undefined}
                        onclick={onSelectMixes}
                    >
                        <span class="nav-icon flex items-center justify-center flex-shrink-0">
                            <Icon name="mixer" size={14} />
                        </span>
                        <span class="overflow-hidden text-ellipsis whitespace-nowrap min-w-0 flex-1">Matrix</span>
                    </button>
                </li>
                {#each mixes as mix (mix.id)}
                    {@const isActive = activeView === 'mix' && selectedMix === mix.id}
                    <li>
                        <button
                            class="{itemBase} pl-9 {isActive ? itemActive : itemIdle}"
                            aria-current={isActive ? 'page' : undefined}
                            onclick={() => onSelectMix(mix.id)}
                        >
                            <span class="nav-icon w-3.5 flex items-center justify-center flex-shrink-0">
                                <span class="dot w-1.5 h-1.5 rounded-full transition-colors {mix.sinks.length > 0 ? 'bg-primary' : 'bg-base-content/55'}"></span>
                            </span>
                            <span class="overflow-hidden text-ellipsis whitespace-nowrap min-w-0 flex-1">{mix.name}</span>
                            <span
                                class="ml-2 font-mono text-xs text-base-content/55 flex-shrink-0"
                                title={mix.sinks.length === 0 ? 'No outputs' : `${mix.sinks.length} output(s)`}
                            >
                                {mix.sinks.length}
                            </span>
                        </button>
                    </li>
                {/each}
            </ul>
        </div>
    </nav>

    <div class="border-t border-base-content/10 p-2">
        <button
            class="w-full flex items-center gap-2 px-2 py-1.5 bg-transparent border border-transparent rounded-md text-base-content/70 text-sm font-medium text-left cursor-pointer transition-colors hover:bg-base-content/5 hover:text-base-content hover:border-base-content/15"
            onclick={onSettings}
            aria-label="Open settings"
            title="Settings"
        >
            <Icon name="settings" size={14} />
            <span class="overflow-hidden text-ellipsis whitespace-nowrap">Settings</span>
        </button>
    </div>
</aside>
