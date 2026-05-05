import type { Binding } from '$lib/types';

export type UiIconRef = { name: string };

export type UiNode =
    | { kind: 'section'; id: string; title?: string; subtitle?: string; children: UiNode[] }
    | { kind: 'row'; id: string; gap?: number; align?: 'start' | 'center' | 'end'; children: UiNode[] }
    | { kind: 'col'; id: string; gap?: number; children: UiNode[] }
    | { kind: 'label'; id: string; text: string; muted?: boolean }
    | { kind: 'heading'; id: string; text: string; level?: 3 | 4 | 5 }
    | { kind: 'icon'; id: string; icon: UiIconRef; size?: number }
    | { kind: 'badge'; id: string; text: string; variant?: 'neutral' | 'primary' | 'success' | 'warning' | 'error' | 'info' }
    | { kind: 'toggle'; id: string; label?: string; value: boolean }
    | { kind: 'slider'; id: string; label?: string; min: number; max: number; step?: number; value: number; suffix?: string }
    | { kind: 'input'; id: string; label?: string; placeholder?: string; value: string }
    | { kind: 'select'; id: string; label?: string; value: string; options: { value: string; label: string }[] }
    | { kind: 'button'; id: string; text: string; icon?: UiIconRef; variant?: 'soft' | 'primary' | 'ghost' | 'warning'; disabled?: boolean }
    | { kind: 'list'; id: string; items: { id: string; label: string; icon?: UiIconRef }[]; sortable?: boolean }
    | { kind: 'binding_capture'; id: string; label: string; sublabel?: string; binding: Binding | null }
    | { kind: 'banner'; id: string; tone: 'info' | 'success' | 'warning' | 'error'; text: string }
    | { kind: 'divider'; id: string }
    | { kind: 'spacer'; id: string; size?: number }
    | { kind: 'icon_picker'; id: string; label?: string; value: string; choices: string[] }
    | { kind: 'iframe'; id: string; src_id: string; height?: number };

export type UiEventValue =
    | { type: 'bool'; value: boolean }
    | { type: 'number'; value: number }
    | { type: 'string'; value: string }
    | { type: 'binding'; value: Binding | null }
    | { type: 'order'; value: string[] }
    | { type: 'click' };

export type UiEventContext =
    | { kind: 'input'; source_name: string };

export interface UiEvent {
    surface_id: string;
    node_id: string;
    value: UiEventValue;
    context?: UiEventContext;
}

export interface SettingsSectionContribution {
    plugin_id: string;
    surface_id: string;
    title: string;
    icon?: UiIconRef;
    priority: number;
    parent_surface_id?: string;
    tree: UiNode;
}

export interface StatusPillContribution {
    plugin_id: string;
    surface_id: string;
    label: string;
    tone?: 'neutral' | 'success' | 'warning' | 'error' | 'info';
    icon?: UiIconRef;
    priority: number;
    tooltip?: string;
}

export interface ChannelOverlayContribution {
    plugin_id: string;
    surface_id: string;
    placement: 'detail' | 'sidebar_badge' | 'header_chip' | 'channel_card';
    channel_filter: { kind: 'all' } | { kind: 'channel_ids'; ids: string[] };
    tree: UiNode;
}

export interface TrayItemContribution {
    plugin_id: string;
    item_id: string;
    label: string;
    accelerator?: string;
    icon?: UiIconRef;
    priority: number;
}

export interface KeybindActionContribution {
    plugin_id: string;
    action_id: string;
    label: string;
}

export interface IframeSurface {
    plugin_id: string;
    surface_id: string;
    entry_path: string;
    initial_data?: unknown;
}

export type InputFilter =
    | { kind: 'all' }
    | { kind: 'physical_only' }
    | { kind: 'source_names'; names: string[] };

export interface InputOverlayContribution {
    plugin_id: string;
    surface_id: string;
    input_filter: InputFilter;
    tree: UiNode;
    values_by_source?: Record<string, Record<string, unknown>>;
}

export interface Contributions {
    settings_sections: SettingsSectionContribution[];
    status_pills: StatusPillContribution[];
    channel_overlays: ChannelOverlayContribution[];
    tray_items: TrayItemContribution[];
    keybind_actions: KeybindActionContribution[];
    iframe_surfaces: IframeSurface[];
    input_overlays: InputOverlayContribution[];
}

export interface PermissionRequest {
    plugin_id: string;
    capability: string;
    rationale: string;
}
