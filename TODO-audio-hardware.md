# TODO: Fix onboard audio (Realtek ALC1220)

Motherboard 3.5mm speakers don't show up in PipeWire because the onboard
codec isn't being detected by the kernel.

## Symptom

`dmesg` reports:
```
snd_hda_intel 0000:17:00.6: no codecs found!
```

Controller (PCI 17:00.6, AMD HD Audio Controller) is alive, but the
ALC1220 codec attached to it isn't answering on the HDA bus.

## What's been ruled out

- `modprobe -r snd_hda_intel && modprobe snd_hda_intel probe_mask=0xfff model=auto`
  reloads cleanly but the "no codecs found!" message is unchanged. This
  isn't a driver/probe configuration issue — there's nothing on the bus
  for the kernel to probe.

## To try (in order)

1. **BIOS check** (most likely fix):
   - HD Audio Controller -> Enabled
   - Onboard Audio -> Enabled
   - Front Panel Audio Type -> HD Audio (not AC97)
   - Anything under Advanced -> Onboard Devices related to audio
2. **Reseat front panel audio cable** in the case.
3. **Clear CMOS** if the codec used to work and just stopped.
4. **Look up board-specific ACPI/firmware quirks** (need exact mobo model).

## Separate workstream: level meters

Critique flagged the absence of per-channel level meters as the single
biggest gap (Visibility of System Status: 1/4). Adding them needs:

- `pipewire` crate dependency in `src-tauri/Cargo.toml`
- Long-lived subscription to each virtual-sink's monitor source
- Peak/RMS calculation throttled to ~30 Hz
- Tauri event emission per channel
- Frontend listener and LED-ladder render in `Fader.svelte`

This is not a UI fix — it's a backend audio-engineering task. Schedule
separately. UI scaffolding will be added when the backend is ready.

## Workaround until fixed

Use any non-HDMI sink as the "speaker" output in Tideline settings
(e.g., Wave XLR or Arctis Nova Pro Wireless) to test the Apply flow.
