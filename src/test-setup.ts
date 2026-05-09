import '@testing-library/jest-dom/vitest';
import { vi } from 'vitest';

// Tauri IPC isn't available under jsdom. Stub the modules our components import
// so onMount() listeners and invoke() calls don't crash the test runner with
// unhandled rejections. Individual tests can still vi.mock() to override.
vi.mock('@tauri-apps/api/core', () => ({
    invoke: vi.fn(async () => null),
}));

vi.mock('@tauri-apps/api/event', () => ({
    listen: vi.fn(async () => () => {}),
    emit: vi.fn(async () => {}),
}));
