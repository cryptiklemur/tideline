import { render, screen } from '@testing-library/svelte';
import { describe, expect, it } from 'vitest';
import PluginStatusPill from '../PluginStatusPill.svelte';

describe('PluginStatusPill', () => {
    it('renders label and tone class', () => {
        render(PluginStatusPill, {
            props: {
                pill: {
                    plugin_id: 'p', surface_id: 's', label: 'REC', tone: 'error',
                    priority: 0,
                },
            },
        });
        const el = screen.getByText('REC');
        expect(el.closest('.badge')!.className).toContain('badge-error');
    });
});
