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
    },
});
