import { render, screen } from '@testing-library/svelte';
import { describe, expect, it, vi } from 'vitest';
import PluginSettingsSection from '../PluginSettingsSection.svelte';
import type { SettingsSectionContribution } from '../types';

describe('PluginSettingsSection', () => {
    it('renders title and tree heading', () => {
        const section: SettingsSectionContribution = {
            plugin_id: 'plug-a',
            surface_id: 'sec',
            title: 'Plug A',
            priority: 0,
            tree: { kind: 'section', id: 'root', title: 'Root', children: [] },
        };
        render(PluginSettingsSection, { props: { section, emit: vi.fn() } });
        expect(screen.getByText('Root')).toBeInTheDocument();
    });
});
