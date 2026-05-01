(function () {
    'use strict';
    const ctx = window.__TIDELINE_PLUGIN__ || {};
    const PLUGIN_ID = ctx.plugin_id;
    const SURFACE_ID = ctx.surface_id;

    const subs = new Set();
    const themeSubs = new Set();

    function readTheme() {
        const root = document.documentElement;
        const cs = getComputedStyle(root);
        const theme = root.getAttribute('data-theme') || 'dark';
        return {
            theme,
            colors: {
                primary: cs.getPropertyValue('--color-primary').trim(),
                base100: cs.getPropertyValue('--color-base-100').trim(),
                base200: cs.getPropertyValue('--color-base-200').trim(),
                base300: cs.getPropertyValue('--color-base-300').trim(),
                baseContent: cs.getPropertyValue('--color-base-content').trim(),
            },
        };
    }

    function postViaTauri(message) {
        if (!window.__TAURI_INTERNALS__) return;
        window.__TAURI_INTERNALS__.invoke('tideline_plugin_iframe_send', {
            pluginId: PLUGIN_ID,
            surfaceId: SURFACE_ID,
            message,
        });
    }

    window.tideline = {
        plugin_id: PLUGIN_ID,
        surface_id: SURFACE_ID,
        send(message) {
            postViaTauri(message);
        },
        onMessage(fn) {
            subs.add(fn);
            return () => subs.delete(fn);
        },
        get theme() {
            return readTheme();
        },
        onThemeChange(fn) {
            themeSubs.add(fn);
            return () => themeSubs.delete(fn);
        },
        open_native(target) {
            postViaTauri({ kind: 'open_native', target });
        },
    };

    window.addEventListener('message', (e) => {
        if (!e.data || e.data.__tideline !== true) return;
        for (const fn of subs) {
            try { fn(e.data.payload); } catch (_) {}
        }
    });

    const mo = new MutationObserver(() => {
        const t = readTheme();
        for (const fn of themeSubs) {
            try { fn(t); } catch (_) {}
        }
    });
    mo.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme', 'style', 'class'] });
})();
