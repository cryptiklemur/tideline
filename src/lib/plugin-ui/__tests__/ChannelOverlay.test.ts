import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import ChannelOverlay from '../ChannelOverlay.svelte';

describe('ChannelOverlay', () => {
    it('renders detail body', () => {
        render(ChannelOverlay, {
            props: {
                overlay: {
                    plugin_id: 'p',
                    surface_id: 's',
                    placement: 'detail',
                    channel_filter: { kind: 'all' },
                    tree: { kind: 'label', id: 'l', text: 'Detail content' },
                },
                emit: vi.fn(),
            },
        });
        expect(screen.getByText('Detail content')).toBeInTheDocument();
    });
});
