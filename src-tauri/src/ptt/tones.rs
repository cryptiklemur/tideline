use crate::ptt::state::Tone;
use rodio::{OutputStream, Sink, Source};
use std::io::Cursor;

const MIC_UNMUTE_WAV: &[u8] = include_bytes!("../../assets/mic-unmute.wav");
const MIC_MUTE_WAV:   &[u8] = include_bytes!("../../assets/mic-mute.wav");

/// Play a single short cue. Tones are ~90ms (single beep), short enough that
/// a rapid press/release sequence plays both cleanly without overlap or
/// debounce-induced drops. Each call spawns its own thread + sink so back-to-
/// back tones can run concurrently if the user really hammers the key.
pub fn play(tone: Tone, enabled: bool, volume_pct: u32) {
    if !enabled { return; }
    let bytes = match tone { Tone::Up => MIC_UNMUTE_WAV, Tone::Down => MIC_MUTE_WAV };
    let v = (volume_pct as f32 / 100.0).clamp(0.0, 1.0);
    std::thread::spawn(move || {
        let (_stream, handle) = match OutputStream::try_default() {
            Ok(s) => s,
            Err(e) => { eprintln!("ptt tone: no audio output: {}", e); return; }
        };
        let sink = match Sink::try_new(&handle) {
            Ok(s) => s,
            Err(e) => { eprintln!("ptt tone: sink err: {}", e); return; }
        };
        let cursor = Cursor::new(bytes);
        let source = match rodio::Decoder::new(cursor) {
            Ok(d) => d.amplify(v),
            Err(e) => { eprintln!("ptt tone: decode err: {}", e); return; }
        };
        sink.append(source);
        sink.sleep_until_end();
    });
}
