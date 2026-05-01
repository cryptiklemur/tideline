import { render } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import UiBindingCapture from '../UiBindingCapture.svelte';

describe('UiBindingCapture', () => {
    it('renders the inner BindingCapture with label', () => {
        const { container } = render(UiBindingCapture, {
            props: {
                node: { kind: 'binding_capture', id: 'bc1', label: 'Push key', binding: null },
                surfaceId: 'sec',
                emit: vi.fn(),
            },
        });
        expect(container.textContent).toContain('Push key');
    });
});
