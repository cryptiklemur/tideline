// Per-hardware blacklist of ALSA card controls that should be hidden from the
// "Device controls" list display. Note: the primary gain control (used for the
// dB readout at the top of InputDetail and the sidebar tooltip) is still picked
// from the *unfiltered* control list — this only suppresses the duplicate row
// in the device-controls list.
//
// Match rules use a regex against the source/sink identifier (typically the
// alsa_input.* / alsa_output.* string) plus the literal control name.

interface BlacklistRule {
    matchSource: RegExp;
    controlName: string;
}

const RULES: BlacklistRule[] = [
    { matchSource: /Elgato.?Wave.?XLR/i, controlName: 'Mic' },
];

export function isControlBlacklisted(deviceId: string, controlName: string): boolean {
    if (!deviceId) return false;
    return RULES.some(r => r.matchSource.test(deviceId) && r.controlName === controlName);
}
