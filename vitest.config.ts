import { svelte } from '@sveltejs/vite-plugin-svelte';
import { defineConfig } from 'vitest/config';
import path from 'node:path';

export default defineConfig({
    plugins: [svelte({ hot: false })],
    resolve: {
        alias: {
            $lib: path.resolve(__dirname, 'src/lib'),
        },
        conditions: ['browser'],
    },
    test: {
        environment: 'jsdom',
        globals: true,
        setupFiles: ['src/test-setup.ts'],
        include: ['src/**/*.test.ts'],
        // vite 6 + vite-plugin-svelte 5.1 + lightningcss (a transitive optional
        // dep) crashes inside preprocessCSS when compiling <style> blocks
        // under vitest. Force postcss to side-step the lightningcss path.
        css: { transformer: 'postcss' },
    },
});
