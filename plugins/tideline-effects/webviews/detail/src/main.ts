import { Bridge, type ChannelEffectsData, type PluginInfo, type Effect } from './bridge';

const params = new URLSearchParams(window.location.search);
const surfaceId = params.get('surface_id') ?? 'rack';
const channelUuid = params.get('channel_uuid') ?? '';

const bridge = new Bridge(surfaceId, channelUuid);

const root = document.getElementById('app')!;
let currentData: ChannelEffectsData = { effects: [], chain_bypassed: false };
let pluginCatalog: PluginInfo[] = [];
let needsInstall = false;
let bannerDismissed = false;
let pickerOpen = false;
let pickerSearch = '';
let dragSrc: number | null = null;

bridge.on((msg) => {
  switch (msg.kind) {
    case 'channel_effects':
      currentData = msg.data;
      render();
      break;
    case 'plugin_list':
      pluginCatalog = msg.plugins;
      render();
      break;
    case 'install_probe':
      needsInstall = msg.needs_install;
      render();
      break;
    case 'install_result':
      if (msg.success) {
        needsInstall = false;
      }
      render();
      break;
    case 'hello':
      bridge.send({ kind: 'list_plugins' });
      break;
  }
});

bridge.send({ kind: 'hello', channel_uuid: channelUuid });

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  className?: string,
  text?: string,
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  if (className) node.className = className;
  if (text !== undefined) node.textContent = text;
  return node;
}

function render() {
  root.replaceChildren();
  if (needsInstall && !bannerDismissed) {
    root.appendChild(renderBanner());
  }
  root.appendChild(renderChainBypass());
  root.appendChild(renderEffectsList());
  root.appendChild(renderAddButton());
  if (pickerOpen) {
    root.appendChild(renderPicker());
  }
}

function renderBanner(): HTMLElement {
  const div = el('div', 'banner');
  const body = el('div', 'banner-body');
  body.appendChild(el('strong', undefined, 'Install audio plugins to use Effects.'));
  body.appendChild(el('span', undefined, 'Tideline will install Carla + LSP Plugins (~80 MB) via your package manager.'));
  div.appendChild(body);

  const actions = el('div', 'banner-actions');
  const installBtn = el('button', 'btn-primary', 'Install');
  installBtn.addEventListener('click', () => {
    bridge.send({ kind: 'install_run' });
  });
  const skipBtn = el('button', 'btn-ghost', 'Skip');
  skipBtn.addEventListener('click', () => {
    bannerDismissed = true;
    bridge.send({ kind: 'dismiss_banner' });
    render();
  });
  actions.appendChild(installBtn);
  actions.appendChild(skipBtn);
  div.appendChild(actions);
  return div;
}

function renderChainBypass(): HTMLElement {
  const div = el('div', 'chain-bypass');
  const label = el('label');
  const input = el('input');
  input.type = 'checkbox';
  input.checked = currentData.chain_bypassed;
  input.addEventListener('change', () => {
    bridge.send({
      kind: 'toggle_chain_bypass',
      channel_uuid: channelUuid,
      bypassed: input.checked,
    });
  });
  label.appendChild(input);
  label.appendChild(document.createTextNode(' Bypass entire chain'));
  div.appendChild(label);
  return div;
}

function renderEffectsList(): HTMLElement {
  const list = el('div', 'effects-list');
  if (currentData.effects.length === 0) {
    list.appendChild(el('div', 'empty', 'No effects yet. Click "+ Add effect" below.'));
    return list;
  }
  currentData.effects.forEach((e, idx) => list.appendChild(renderEffectTile(e, idx)));
  return list;
}

function renderEffectTile(effect: Effect, idx: number): HTMLElement {
  const tile = el('div', 'effect-tile');
  tile.draggable = true;
  tile.dataset.index = String(idx);

  tile.appendChild(el('span', 'drag-handle', '::'));
  tile.appendChild(el('span', `format-badge format-${effect.format}`, effect.format.toUpperCase()));
  tile.appendChild(el('span', 'effect-name', effect.display_name));

  const bypassBtn = el(
    'button',
    `bypass-toggle ${effect.bypassed ? 'bypassed' : ''}`.trim(),
    effect.bypassed ? 'Off' : 'On',
  );
  bypassBtn.addEventListener('click', () => {
    bridge.send({
      kind: 'toggle_bypass',
      channel_uuid: channelUuid,
      effect_id: effect.id,
      bypassed: !effect.bypassed,
    });
  });
  tile.appendChild(bypassBtn);

  const editBtn = el('button', 'edit-btn', 'Edit');
  editBtn.addEventListener('click', () => {
    bridge.send({ kind: 'open_plugin_gui', channel_uuid: channelUuid, effect_id: effect.id });
  });
  tile.appendChild(editBtn);

  const deleteBtn = el('button', 'delete-btn', 'x');
  deleteBtn.addEventListener('click', () => {
    bridge.send({ kind: 'remove_effect', channel_uuid: channelUuid, effect_id: effect.id });
  });
  tile.appendChild(deleteBtn);

  tile.addEventListener('dragstart', () => {
    dragSrc = idx;
  });
  tile.addEventListener('dragover', (e) => e.preventDefault());
  tile.addEventListener('drop', () => {
    if (dragSrc !== null && dragSrc !== idx) {
      const ids = currentData.effects.map((e) => e.id);
      const [moved] = ids.splice(dragSrc, 1);
      ids.splice(idx, 0, moved);
      bridge.send({
        kind: 'reorder',
        channel_uuid: channelUuid,
        new_order: ids,
      });
    }
    dragSrc = null;
  });

  return tile;
}

function renderAddButton(): HTMLElement {
  const btn = el('button', 'add-effect-btn', '+ Add effect');
  btn.addEventListener('click', () => {
    pickerOpen = true;
    bridge.send({ kind: 'list_plugins' });
    render();
  });
  return btn;
}

function renderPicker(): HTMLElement {
  const overlay = el('div', 'picker-overlay');
  const panel = el('div', 'picker-panel');
  const search = el('input');
  search.type = 'text';
  search.placeholder = 'Search plugins...';
  search.value = pickerSearch;
  search.addEventListener('input', () => {
    pickerSearch = search.value;
    renderPickerList();
  });
  panel.appendChild(search);

  const list = el('div', 'picker-list');
  panel.appendChild(list);

  const close = el('button', 'btn-ghost', 'Cancel');
  close.addEventListener('click', () => {
    pickerOpen = false;
    pickerSearch = '';
    render();
  });
  panel.appendChild(close);
  overlay.appendChild(panel);

  function renderPickerList() {
    list.replaceChildren();
    const q = pickerSearch.toLowerCase();
    const grouped = new Map<string, PluginInfo[]>();
    for (const p of pluginCatalog) {
      if (q && !p.name.toLowerCase().includes(q) && !p.vendor.toLowerCase().includes(q)) continue;
      const arr = grouped.get(p.category) ?? [];
      arr.push(p);
      grouped.set(p.category, arr);
    }
    for (const [cat, plugins] of grouped) {
      list.appendChild(el('h3', undefined, cat.toUpperCase()));
      for (const p of plugins) {
        const row = el('button', 'picker-item');
        row.appendChild(el('span', `format-badge format-${p.format}`, p.format.toUpperCase()));
        row.appendChild(el('span', 'picker-name', p.name));
        row.appendChild(el('span', 'picker-vendor', p.vendor));
        row.addEventListener('click', () => {
          bridge.send({
            kind: 'add_effect',
            channel_uuid: channelUuid,
            plugin_uri: p.uri,
            format: p.format,
            display_name: p.name,
          });
          pickerOpen = false;
          pickerSearch = '';
          render();
        });
        list.appendChild(row);
      }
    }
  }
  renderPickerList();

  return overlay;
}
