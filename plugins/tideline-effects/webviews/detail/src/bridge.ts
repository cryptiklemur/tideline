export type PluginFormat = 'lv2' | 'vst3' | 'vst2' | 'clap';
export type Category = 'eq' | 'dynamics' | 'reverb' | 'modulation' | 'utility' | 'other';

export interface Effect {
  id: string;
  format: PluginFormat;
  uri: string;
  display_name: string;
  bypassed: boolean;
  state_b64: string | null;
}

export interface ChannelEffectsData {
  effects: Effect[];
  chain_bypassed: boolean;
}

export interface PluginInfo {
  format: PluginFormat;
  uri: string;
  name: string;
  vendor: string;
  category: Category;
}

export type Inbound =
  | { kind: 'hello'; channel_uuid: string }
  | { kind: 'list_plugins' }
  | { kind: 'add_effect'; channel_uuid: string; plugin_uri: string; format: PluginFormat; display_name: string }
  | { kind: 'remove_effect'; channel_uuid: string; effect_id: string }
  | { kind: 'toggle_bypass'; channel_uuid: string; effect_id: string; bypassed: boolean }
  | { kind: 'toggle_chain_bypass'; channel_uuid: string; bypassed: boolean }
  | { kind: 'reorder'; channel_uuid: string; new_order: string[] }
  | { kind: 'open_plugin_gui'; channel_uuid: string; effect_id: string }
  | { kind: 'save_all_state'; channel_uuid: string }
  | { kind: 'install_probe' }
  | { kind: 'install_run' }
  | { kind: 'dismiss_banner' };

export type Outbound =
  | { kind: 'hello'; plugin_version: string }
  | { kind: 'plugin_list'; plugins: PluginInfo[] }
  | { kind: 'channel_effects'; channel_uuid: string; data: ChannelEffectsData }
  | { kind: 'install_probe'; needs_install: boolean }
  | { kind: 'install_result'; success: boolean; message: string }
  | { kind: 'toast'; tone: string; title: string; body: string };

declare global {
  interface Window {
    tideline: {
      send(message: unknown): void;
      onMessage(cb: (msg: unknown) => void): void;
      theme(): { mode: 'light' | 'dark'; tokens: Record<string, string> };
      onThemeChange(cb: (theme: { mode: 'light' | 'dark'; tokens: Record<string, string> }) => void): void;
      open_native(window: { kind: 'plugin_gui'; uri: string }): void;
    };
  }
}

export class Bridge {
  private listeners: ((msg: Outbound) => void)[] = [];
  private surfaceId: string;
  private channelUuid: string;

  constructor(surfaceId: string, channelUuid: string) {
    this.surfaceId = surfaceId;
    this.channelUuid = channelUuid;
    window.tideline.onMessage((msg) => {
      const m = msg as Outbound;
      for (const l of this.listeners) l(m);
    });
  }

  send(msg: Inbound) {
    window.tideline.send({
      surface_id: this.surfaceId,
      channel_uuid: this.channelUuid,
      message: msg,
    });
  }

  on(cb: (msg: Outbound) => void) {
    this.listeners.push(cb);
  }
}
