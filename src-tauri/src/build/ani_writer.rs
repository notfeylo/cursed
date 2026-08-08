//! A `.ani` writer: a real RIFF `ACON` container (PRD §6.2).
//!
//! The structural point worth stating plainly, because it is the thing people
//! get wrong: **each frame inside an `.ani` is a complete, valid `.cur` file** —
//! header, directory, DIB and all. It is not raw pixels, and it is not a
//! stripped-down image record. So this module builds its frames with the exact
//! same [`write_cur`](crate::build::cur_writer::write_cur) used for static
//! cursors, which means a bug can never exist in one path and not the other.
//!
//! ```text
//! RIFF <size> ACON
//!   anih <36>        header: step count, frame count, default rate, flags
//!   rate <4*steps>   per-step delay, in jiffies       (optional)
//!   seq  <4*steps>   playback order                   (optional)
//!   LIST <size> INFO INAM/IART                        (optional)
//!   LIST <size> fram
//!     icon <size> <a whole .cur file>  x N
//! ```

use crate::build::cur_writer::{write_cur, CursorImage};
use crate::error::{AppError, AppResult};

/// Jiffies are sixtieths of a second — the unit the `.ani` format counts in.
pub const JIFFIES_PER_SECOND: u32 = 60;

/// PRD §6.2: beyond roughly 60 frames the shell's own animation cost becomes
/// visible, so the cap is a design decision rather than a limitation.
pub const MAX_FRAMES: usize = 60;
/// Four seconds keeps files small and loops readable.
pub const MAX_DURATION_MS: u32 = 4_000;

const AF_ICON: u32 = 0x0000_0001;
const AF_SEQUENCE: u32 = 0x0000_0002;

/// One animation frame: a full set of resolutions, plus how long it is shown.
#[derive(Debug, Clone)]
pub struct AniFrame {
    pub images: Vec<CursorImage>,
    pub delay_ms: u32,
}

#[derive(Debug, Clone, Default)]
pub struct AniMetadata {
    pub name: Option<String>,
    pub author: Option<String>,
}

/// Milliseconds to jiffies, clamped to the range Windows will actually honour.
/// A zero-jiffy frame makes the shell spin; a huge one stalls the pointer.
pub fn ms_to_jiffies(delay_ms: u32) -> u32 {
    ((delay_ms as u64 * JIFFIES_PER_SECOND as u64 + 500) / 1_000).clamp(1, 100) as u32
}

fn chunk(id: &[u8; 4], payload: &[u8], out: &mut Vec<u8>) {
    out.extend_from_slice(id);
    out.extend_from_slice(&(payload.len() as u32).to_le_bytes());
    out.extend_from_slice(payload);
    // RIFF chunks are word-aligned; the pad byte is not counted in the size.
    if payload.len() % 2 == 1 {
        out.push(0);
    }
}

/// Trims a frame list to the caps, preserving the animation's shape by dropping
/// from the end rather than resampling — a truncated loop still plays; a
/// resampled one changes the artwork's timing in ways the author did not choose.
fn enforce_caps(frames: &[AniFrame], speed: f32) -> Vec<AniFrame> {
    let speed = speed.clamp(0.5, 2.0);
    let mut out = Vec::with_capacity(frames.len().min(MAX_FRAMES));
    let mut total = 0u32;

    for frame in frames.iter().take(MAX_FRAMES) {
        let scaled = ((frame.delay_ms as f32) / speed).round().max(1.0) as u32;
        if total + scaled > MAX_DURATION_MS && !out.is_empty() {
            break;
        }
        total += scaled;
        out.push(AniFrame {
            images: frame.images.clone(),
            delay_ms: scaled,
        });
    }
    out
}

/// Writes a complete `.ani` file.
pub fn write_ani(
    frames: &[AniFrame],
    speed: f32,
    metadata: &AniMetadata,
) -> AppResult<Vec<u8>> {
    if frames.is_empty() {
        return Err(AppError::invalid("an animated cursor needs at least one frame"));
    }

    let frames = enforce_caps(frames, speed);
    if frames.is_empty() {
        return Err(AppError::invalid("every frame was longer than the duration cap"));
    }

    // Encode each frame as a standalone .cur first — if any frame is malformed
    // we fail before writing a container that claims to hold it.
    let encoded: Vec<Vec<u8>> = frames
        .iter()
        .map(|frame| write_cur(&frame.images))
        .collect::<AppResult<_>>()?;

    let count = frames.len() as u32;
    let rates: Vec<u32> = frames.iter().map(|f| ms_to_jiffies(f.delay_ms)).collect();
    let uniform = rates.windows(2).all(|pair| pair[0] == pair[1]);
    let default_rate = rates.first().copied().unwrap_or(6);

    let mut body = Vec::new();
    body.extend_from_slice(b"ACON");

    // ── anih ───────────────────────────────────────────────────
    let mut anih = Vec::with_capacity(36);
    anih.extend_from_slice(&36u32.to_le_bytes()); // cbSize
    anih.extend_from_slice(&count.to_le_bytes()); // cSteps
    anih.extend_from_slice(&count.to_le_bytes()); // cFrames
    // cx / cy / cBitCount / cPlanes are zero when the frames are icon data:
    // each embedded .cur already describes its own dimensions and depth.
    anih.extend_from_slice(&0u32.to_le_bytes()); // cx
    anih.extend_from_slice(&0u32.to_le_bytes()); // cy
    anih.extend_from_slice(&0u32.to_le_bytes()); // cBitCount
    anih.extend_from_slice(&0u32.to_le_bytes()); // cPlanes
    anih.extend_from_slice(&default_rate.to_le_bytes()); // jifRate
    let flags = AF_ICON | if uniform { 0 } else { AF_SEQUENCE };
    anih.extend_from_slice(&flags.to_le_bytes());
    chunk(b"anih", &anih, &mut body);

    // ── rate / seq (only when the frames are not evenly timed) ─
    if !uniform {
        let rate_payload: Vec<u8> = rates.iter().flat_map(|r| r.to_le_bytes()).collect();
        chunk(b"rate", &rate_payload, &mut body);

        let seq_payload: Vec<u8> = (0..count).flat_map(|i| i.to_le_bytes()).collect();
        chunk(b"seq ", &seq_payload, &mut body);
    }

    // ── LIST INFO ──────────────────────────────────────────────
    if metadata.name.is_some() || metadata.author.is_some() {
        let mut info = Vec::new();
        info.extend_from_slice(b"INFO");
        if let Some(name) = &metadata.name {
            chunk(b"INAM", &nul_terminated(name), &mut info);
        }
        if let Some(author) = &metadata.author {
            chunk(b"IART", &nul_terminated(author), &mut info);
        }
        chunk(b"LIST", &info, &mut body);
    }

    // ── LIST fram ──────────────────────────────────────────────
    let mut fram = Vec::new();
    fram.extend_from_slice(b"fram");
    for cur in &encoded {
        chunk(b"icon", cur, &mut fram);
    }
    chunk(b"LIST", &fram, &mut body);

    let mut out = Vec::with_capacity(body.len() + 8);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    Ok(out)
}

/// Info chunks are NUL-terminated text, and anything a user or an imported pack
/// supplied is treated as inert data: control characters are stripped and the
/// length is capped. Nothing read from a pack is ever interpreted (PRD §13.6).
fn nul_terminated(text: &str) -> Vec<u8> {
    let cleaned: String = text
        .chars()
        .filter(|c| !c.is_control())
        .take(120)
        .collect();
    let mut bytes = cleaned.into_bytes();
    bytes.push(0);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::build::bitmap::Bitmap;

    fn frame(delay_ms: u32) -> AniFrame {
        let mut bitmap = Bitmap::new(32, 32);
        bitmap.set_pixel(1, 1, [255, 255, 255, 255]);
        AniFrame {
            images: vec![CursorImage::new(bitmap, (0, 0))],
            delay_ms,
        }
    }

    fn find(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack.windows(needle.len()).position(|w| w == needle)
    }

    fn read_u32(bytes: &[u8], at: usize) -> u32 {
        u32::from_le_bytes([bytes[at], bytes[at + 1], bytes[at + 2], bytes[at + 3]])
    }

    #[test]
    fn container_is_a_riff_acon_with_a_correct_size_field() {
        let file = write_ani(&[frame(100), frame(100)], 1.0, &AniMetadata::default()).unwrap();
        assert_eq!(&file[0..4], b"RIFF");
        assert_eq!(&file[8..12], b"ACON");
        assert_eq!(read_u32(&file, 4) as usize, file.len() - 8, "RIFF size");
    }

    #[test]
    fn anih_reports_the_frame_count_and_the_icon_flag() {
        let file = write_ani(
            &[frame(100), frame(100), frame(100)],
            1.0,
            &AniMetadata::default(),
        )
        .unwrap();
        let at = find(&file, b"anih").unwrap();
        assert_eq!(read_u32(&file, at + 4), 36, "chunk size");
        assert_eq!(read_u32(&file, at + 8), 36, "cbSize");
        assert_eq!(read_u32(&file, at + 12), 3, "cSteps");
        assert_eq!(read_u32(&file, at + 16), 3, "cFrames");
        assert_eq!(read_u32(&file, at + 36), 6, "100ms is 6 jiffies");
        assert_eq!(read_u32(&file, at + 40) & AF_ICON, AF_ICON);
    }

    #[test]
    fn every_frame_is_a_complete_cur_file() {
        let file = write_ani(&[frame(80), frame(80)], 1.0, &AniMetadata::default()).unwrap();
        let at = find(&file, b"icon").unwrap();
        let len = read_u32(&file, at + 4) as usize;
        let cur = &file[at + 8..at + 8 + len];
        assert_eq!(u16::from_le_bytes([cur[0], cur[1]]), 0, "idReserved");
        assert_eq!(u16::from_le_bytes([cur[2], cur[3]]), 2, "embedded idType is 2");
        assert_eq!(u16::from_le_bytes([cur[4], cur[5]]), 1, "idCount");
    }

    #[test]
    fn uneven_timing_emits_rate_and_seq_and_sets_the_sequence_flag() {
        let file = write_ani(&[frame(50), frame(200)], 1.0, &AniMetadata::default()).unwrap();
        let at = find(&file, b"anih").unwrap();
        assert_eq!(read_u32(&file, at + 40) & AF_SEQUENCE, AF_SEQUENCE);

        let rate = find(&file, b"rate").unwrap();
        assert_eq!(read_u32(&file, rate + 4), 8, "two u32 delays");
        assert_eq!(read_u32(&file, rate + 8), 3, "50ms -> 3 jiffies");
        assert_eq!(read_u32(&file, rate + 12), 12, "200ms -> 12 jiffies");
        assert!(find(&file, b"seq ").is_some());
    }

    #[test]
    fn evenly_timed_animations_skip_the_optional_chunks() {
        let file = write_ani(
            &[frame(100), frame(100), frame(100), frame(100)],
            1.0,
            &AniMetadata::default(),
        )
        .unwrap();
        assert!(find(&file, b"rate").is_none());
        assert!(find(&file, b"seq ").is_none());
    }

    #[test]
    fn jiffy_conversion_rounds_and_clamps() {
        assert_eq!(ms_to_jiffies(0), 1, "never zero");
        assert_eq!(ms_to_jiffies(16), 1);
        assert_eq!(ms_to_jiffies(100), 6);
        assert_eq!(ms_to_jiffies(1_000), 60);
        assert_eq!(ms_to_jiffies(10_000), 100, "clamped");
    }

    #[test]
    fn frame_and_duration_caps_are_enforced() {
        let many = vec![frame(100); 200];
        let file = write_ani(&many, 1.0, &AniMetadata::default()).unwrap();
        let at = find(&file, b"anih").unwrap();
        let steps = read_u32(&file, at + 12);
        assert!(steps <= MAX_FRAMES as u32, "frame cap");
        assert!(steps * 100 <= MAX_DURATION_MS, "duration cap");
    }

    #[test]
    fn speed_multiplier_shortens_delays() {
        let fast = enforce_caps(&[frame(100)], 2.0);
        let slow = enforce_caps(&[frame(100)], 0.5);
        assert_eq!(fast[0].delay_ms, 50);
        assert_eq!(slow[0].delay_ms, 200);
    }

    #[test]
    fn metadata_is_written_as_inert_nul_terminated_text() {
        let file = write_ani(
            &[frame(100)],
            1.0,
            &AniMetadata {
                name: Some("PULSE\u{7}\nRING".into()),
                author: Some("feylo".into()),
            },
        )
        .unwrap();
        let at = find(&file, b"INAM").unwrap();
        let len = read_u32(&file, at + 4) as usize;
        assert_eq!(&file[at + 8..at + 8 + len], b"PULSERING\0", "controls stripped");
        assert!(find(&file, b"IART").is_some());
    }

    #[test]
    fn odd_length_chunks_are_word_aligned() {
        let mut out = Vec::new();
        chunk(b"test", &[1, 2, 3], &mut out);
        assert_eq!(out.len(), 12, "8 header + 3 payload + 1 pad");
        assert_eq!(read_u32(&out, 4), 3, "size excludes the pad byte");
        assert_eq!(out[11], 0);
    }

    #[test]
    fn an_empty_animation_is_refused() {
        assert!(write_ani(&[], 1.0, &AniMetadata::default()).is_err());
    }

    /// Same gate as the `.cur` writer: the container is only correct if Windows
    /// will load it (PRD §19 rule 3). An `.ani` that parses in a test but not in
    /// `LoadImageW` is worth nothing.
    #[test]
    fn a_generated_animation_loads_in_windows() {
        let frames: Vec<AniFrame> = (0..8)
            .map(|i| {
                let mut bitmap = Bitmap::new(32, 32);
                // Move a block per frame, so the frames genuinely differ.
                for y in 8..24 {
                    for x in (i * 2)..(i * 2 + 12) {
                        bitmap.set_pixel(x.min(31), y, [255, 255, 255, 255]);
                    }
                }
                AniFrame {
                    images: vec![CursorImage::new(bitmap, (0, 0))],
                    delay_ms: 60,
                }
            })
            .collect();

        let bytes = write_ani(
            &frames,
            1.0,
            &AniMetadata {
                name: Some("ROUNDTRIP".into()),
                author: Some("feylo".into()),
            },
        )
        .unwrap();

        let dir = std::env::temp_dir().join("cursorforge-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("roundtrip.ani");
        std::fs::write(&path, &bytes).unwrap();

        let loaded = crate::cursor::engine::verify_loadable(&path);
        let _ = std::fs::remove_file(&path);
        assert!(loaded.is_ok(), "Windows refused the animation: {loaded:?}");
    }
}
