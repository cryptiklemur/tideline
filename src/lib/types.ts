export interface SinkInput {
    index: number;
    node_name: string;
    muted: boolean;
    volume: number;
}

export type OutputMode = 'headphones' | 'speakers' | 'both';

export type ChannelKind = 'output' | 'input' | 'physical_input';

export interface ChannelConfig {
    uuid: string;
    name: string;
    kind: ChannelKind;
    hp_node: string;
    sp_node: string;
    programs: string[];
    sources: string[];
    physical_source: string;
    icon: string;
    plugin_data?: Record<string, unknown>;
}

export interface Mix {
    uuid: string;
    id: string;
    name: string;
    sinks: string[];
    plugin_data?: Record<string, unknown>;
}

export type KeybindAction =
    | { type: 'toggle_output_mute'; sink: string }
    | { type: 'toggle_channel_mute'; channel: string }
    | { type: 'toggle_mix_enabled'; mix_id: string }
    | { type: 'plugin'; plugin_id: string; action_id: string };

export interface AppConfig {
    mixes: Mix[];
    channels: ChannelConfig[];
    keybinds: Record<string, KeybindAction>;
    ptt: PttConfig;
}

export interface SinkInfo {
    name: string;
    description: string;
    muted: boolean;
    volume_percent: number;
}

export interface SourceInfo {
    name: string;
    description: string;
}

export interface RunningApp {
    binary: string;
    application_name: string;
    sink: string;
}

export interface ChannelVolumes {
    master: number;
    mixes: Record<string, number>;
}

export interface AudioBackendStatus {
    server_name: string | null;
    on_pipewire: boolean;
    started_services: string[];
    errors: string[];
    affected_apps: string[];
}

export interface CardControl {
    name: string;
    volume_percent: number;
    muted: boolean;
    has_volume: boolean;
    has_switch: boolean;
    is_capture: boolean;
    is_playback: boolean;
    current_db: number | null;
}

export type Modifier = 'ctrl' | 'shift' | 'alt' | 'super';

export type Binding =
    | { kind: 'keyboard'; mods: Modifier[]; key: string }
    | { kind: 'mouse'; mods: Modifier[]; button: string };

export type Mode = 'open' | 'ptt';

export interface PttConfig {
    mode: Mode;
    mode_toggle_binding: Binding | null;
    hold_binding: Binding | null;
    input_device: string;
    tones_enabled: boolean;
    tones_volume: number;
    led_enabled: boolean;
}

export type CaptureMethod = 'none' | 'portal' | 'evdev';

export interface PttState {
    mode: Mode;
    hold_active: boolean;
    transmitting: boolean; // mode==='open' || hold_active
    error: string | null;  // e.g., evdev permission failure
    capture_method: CaptureMethod;
}
