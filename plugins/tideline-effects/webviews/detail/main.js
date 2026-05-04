var B = Object.defineProperty;
var I = (e, t, n) => t in e ? B(e, t, { enumerable: !0, configurable: !0, writable: !0, value: n }) : e[t] = n;
var _ = (e, t, n) => I(e, typeof t != "symbol" ? t + "" : t, n);
class U {
  constructor(t, n) {
    _(this, "listeners", []);
    _(this, "surfaceId");
    _(this, "channelUuid");
    this.surfaceId = t, this.channelUuid = n, window.tideline.onMessage((i) => {
      const s = i;
      for (const p of this.listeners) p(s);
    });
  }
  send(t) {
    window.tideline.send({
      surface_id: this.surfaceId,
      channel_uuid: this.channelUuid,
      message: t
    });
  }
  on(t) {
    this.listeners.push(t);
  }
}
const E = new URLSearchParams(window.location.search), S = E.get("surface_id") ?? "rack", c = E.get("channel_uuid") ?? "", a = new U(S, c), h = document.getElementById("app");
let g = { effects: [], chain_bypassed: !1 }, L = [], y = !1, w = !1, m = !1, C = "", k = null;
a.on((e) => {
  switch (e.kind) {
    case "channel_effects":
      g = e.data, o();
      break;
    case "plugin_list":
      L = e.plugins, o();
      break;
    case "install_probe":
      y = e.needs_install, o();
      break;
    case "install_result":
      e.success && (y = !1), o();
      break;
    case "hello":
      a.send({ kind: "list_plugins" });
      break;
  }
});
a.send({ kind: "hello", channel_uuid: c });
function d(e, t, n) {
  const i = document.createElement(e);
  return t && (i.className = t), n !== void 0 && (i.textContent = n), i;
}
function o() {
  h.replaceChildren(), y && !w && h.appendChild(P()), h.appendChild(A()), h.appendChild(D()), h.appendChild(O()), m && h.appendChild(T());
}
function P() {
  const e = d("div", "banner"), t = d("div", "banner-body");
  t.appendChild(d("strong", void 0, "Install audio plugins to use Effects.")), t.appendChild(d("span", void 0, "Tideline will install Carla + LSP Plugins (~80 MB) via your package manager.")), e.appendChild(t);
  const n = d("div", "banner-actions"), i = d("button", "btn-primary", "Install");
  i.addEventListener("click", () => {
    a.send({ kind: "install_run" });
  });
  const s = d("button", "btn-ghost", "Skip");
  return s.addEventListener("click", () => {
    w = !0, a.send({ kind: "dismiss_banner" }), o();
  }), n.appendChild(i), n.appendChild(s), e.appendChild(n), e;
}
function A() {
  const e = d("div", "chain-bypass"), t = d("label"), n = d("input");
  return n.type = "checkbox", n.checked = g.chain_bypassed, n.addEventListener("change", () => {
    a.send({
      kind: "toggle_chain_bypass",
      channel_uuid: c,
      bypassed: n.checked
    });
  }), t.appendChild(n), t.appendChild(document.createTextNode(" Bypass entire chain")), e.appendChild(t), e;
}
function D() {
  const e = d("div", "effects-list");
  return g.effects.length === 0 ? (e.appendChild(d("div", "empty", 'No effects yet. Click "+ Add effect" below.')), e) : (g.effects.forEach((t, n) => e.appendChild(M(t, n))), e);
}
function M(e, t) {
  const n = d("div", "effect-tile");
  n.draggable = !0, n.dataset.index = String(t), n.appendChild(d("span", "drag-handle", "::")), n.appendChild(d("span", `format-badge format-${e.format}`, e.format.toUpperCase())), n.appendChild(d("span", "effect-name", e.display_name));
  const i = d(
    "button",
    `bypass-toggle ${e.bypassed ? "bypassed" : ""}`.trim(),
    e.bypassed ? "Off" : "On"
  );
  i.addEventListener("click", () => {
    a.send({
      kind: "toggle_bypass",
      channel_uuid: c,
      effect_id: e.id,
      bypassed: !e.bypassed
    });
  }), n.appendChild(i);
  const s = d("button", "edit-btn", "Edit");
  s.addEventListener("click", () => {
    a.send({ kind: "open_plugin_gui", channel_uuid: c, effect_id: e.id });
  }), n.appendChild(s);
  const p = d("button", "delete-btn", "x");
  return p.addEventListener("click", () => {
    a.send({ kind: "remove_effect", channel_uuid: c, effect_id: e.id });
  }), n.appendChild(p), n.addEventListener("dragstart", () => {
    k = t;
  }), n.addEventListener("dragover", (l) => l.preventDefault()), n.addEventListener("drop", () => {
    if (k !== null && k !== t) {
      const l = g.effects.map((r) => r.id), [f] = l.splice(k, 1);
      l.splice(t, 0, f), a.send({
        kind: "reorder",
        channel_uuid: c,
        new_order: l
      });
    }
    k = null;
  }), n;
}
function O() {
  const e = d("button", "add-effect-btn", "+ Add effect");
  return e.addEventListener("click", () => {
    m = !0, a.send({ kind: "list_plugins" }), o();
  }), e;
}
function T() {
  const e = d("div", "picker-overlay"), t = d("div", "picker-panel"), n = d("input");
  n.type = "text", n.placeholder = "Search plugins...", n.value = C, n.addEventListener("input", () => {
    C = n.value, p();
  }), t.appendChild(n);
  const i = d("div", "picker-list");
  t.appendChild(i);
  const s = d("button", "btn-ghost", "Cancel");
  s.addEventListener("click", () => {
    m = !1, C = "", o();
  }), t.appendChild(s), e.appendChild(t);
  function p() {
    i.replaceChildren();
    const l = C.toLowerCase(), f = /* @__PURE__ */ new Map();
    for (const r of L) {
      if (l && !r.name.toLowerCase().includes(l) && !r.vendor.toLowerCase().includes(l)) continue;
      const v = f.get(r.category) ?? [];
      v.push(r), f.set(r.category, v);
    }
    for (const [r, v] of f) {
      i.appendChild(d("h3", void 0, r.toUpperCase()));
      for (const u of v) {
        const b = d("button", "picker-item");
        b.appendChild(d("span", `format-badge format-${u.format}`, u.format.toUpperCase())), b.appendChild(d("span", "picker-name", u.name)), b.appendChild(d("span", "picker-vendor", u.vendor)), b.addEventListener("click", () => {
          a.send({
            kind: "add_effect",
            channel_uuid: c,
            plugin_uri: u.uri,
            format: u.format,
            display_name: u.name
          }), m = !1, C = "", o();
        }), i.appendChild(b);
      }
    }
  }
  return p(), e;
}
