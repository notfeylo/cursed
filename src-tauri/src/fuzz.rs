//! Deterministic mutation testing for every parser this app owns.
//!
//! ## What this is, and what it is not
//!
//! It is not `cargo-fuzz`. libFuzzer on Windows MSVC needs a nightly toolchain
//! and sanitiser support that this project's pinned toolchain does not have, and
//! a fuzzing setup that only runs on a machine nobody here has is a fuzzing
//! setup that never runs. `SECURITY.md` records how to point `cargo-fuzz` at
//! these same entry points from a Linux box, for anybody who has one.
//!
//! What it is: a seeded mutator over valid inputs, run in the ordinary test
//! suite, on every push, for ever. Thousands of malformed files per parser per
//! run, with a fixed seed so a failure is reproducible from the line that
//! reports it.
//!
//! ## The property under test
//!
//! **No input may panic.** Not "no input may be accepted" — a parser is allowed,
//! and expected, to reject nearly all of this. What it may never do is index out
//! of bounds, unwrap a `None`, subtract past zero, or allocate on a length field
//! it was handed. PRD §19 rule 4 says there is no `unwrap`, `expect` or `panic!`
//! in a command path, and this is the thing that checks it rather than asserting
//! it in a comment.
//!
//! A panic in a Tauri command takes the whole app with it, which on a cursor
//! tool means the watchdog stops and the pointer is left wherever it was.
//!
//! ## What is deliberately not fuzzed here
//!
//! **`.cur` and `.ani` decoding.** `build::cur_reader` does not parse them —
//! it hands the path to `LoadImageW` and lets Windows do it. Feeding malformed
//! cursors to that would be fuzzing Windows' own image loader, from a test
//! suite, with the results landing on the developer's session. The bytes this
//! project *writes* are covered by the round-trip tests in `build::cur_writer`
//! and `build::ani_writer`.

/// xorshift64*, so a run is reproducible from its seed and the suite has no
/// dependency on a random-number crate.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Zero is a fixed point of xorshift and would produce a stream of
        // zeroes — which is a valid input to fuzz with, and a useless one to
        // fuzz *only* with.
        Self(seed | 1)
    }

    fn next(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    fn below(&mut self, bound: usize) -> usize {
        if bound == 0 {
            0
        } else {
            (self.next() % bound as u64) as usize
        }
    }

    fn byte(&mut self) -> u8 {
        (self.next() >> 24) as u8
    }
}

/// Damages `seed` in one of the ways a real file gets damaged.
///
/// Every arm is a failure that has actually happened to a file somewhere: a
/// truncated download, a flipped bit on a failing disk, an editor that inserted
/// a byte, a length field somebody edited by hand. The last is the one that
/// finds allocation bugs — a header claiming four billion frames is not a
/// corrupted file, it is a crafted one.
fn mutate(seed: &[u8], rng: &mut Rng) -> Vec<u8> {
    let mut out = seed.to_vec();
    if out.is_empty() {
        return out;
    }

    match rng.next() % 6 {
        // Flip some bits.
        0 => {
            for _ in 0..1 + rng.below(8) {
                let at = rng.below(out.len());
                out[at] ^= 1 << (rng.below(8));
            }
        }
        // Truncate.
        1 => {
            let keep = rng.below(out.len());
            out.truncate(keep);
        }
        // Overwrite a run.
        2 => {
            let at = rng.below(out.len());
            for index in at..(at + 1 + rng.below(32)).min(out.len()) {
                out[index] = rng.byte();
            }
        }
        // Insert bytes, which shifts every offset after it.
        3 => {
            let at = rng.below(out.len());
            let byte = rng.byte();
            for _ in 0..1 + rng.below(16) {
                out.insert(at, byte);
            }
        }
        // Remove bytes.
        4 => {
            let at = rng.below(out.len());
            let count = (1 + rng.below(16)).min(out.len() - at);
            out.drain(at..at + count);
        }
        // A hostile length field: write 0xFF across four bytes somewhere early,
        // which is where headers live in every format here.
        _ => {
            let at = rng.below(out.len().min(64));
            for index in at..(at + 4).min(out.len()) {
                out[index] = 0xFF;
            }
        }
    }
    out
}

/// Runs `parse` over mutations of every seed and fails on the first panic.
///
/// `catch_unwind` rather than letting the panic through, so the report names the
/// parser, the seed, the iteration and the bytes — a bare panic in a fuzz loop
/// tells you something broke and nothing about what to feed it to see it again.
///
/// The release profile is `panic = "abort"`, so this can only work in a test
/// build. That is fine: the point is to find the panic here, where it is a
/// failing test, rather than there, where it is an app that vanishes.
#[cfg(test)]
fn hammer(what: &str, seeds: &[Vec<u8>], iterations: usize, parse: impl Fn(&[u8]) + std::panic::RefUnwindSafe) {
    // The default hook prints a backtrace per panic, which for a loop that is
    // *expecting* to find one would bury the report. Silenced for the loop and
    // restored after.
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));

    let mut failure = None;
    'outer: for (index, seed) in seeds.iter().enumerate() {
        let mut rng = Rng::new(0x5EED_0000 + index as u64);
        for iteration in 0..iterations {
            let input = mutate(seed, &mut rng);
            let attempt = std::panic::catch_unwind(|| parse(&input));
            if attempt.is_err() {
                failure = Some((index, iteration, input));
                break 'outer;
            }
        }
    }

    std::panic::set_hook(previous);

    if let Some((seed_index, iteration, input)) = failure {
        let preview: String = input
            .iter()
            .take(64)
            .map(|b| format!("{b:02x}"))
            .collect::<Vec<_>>()
            .join("");
        panic!(
            "{what} panicked on seed {seed_index}, iteration {iteration}, \
             {} bytes beginning {preview}",
            input.len()
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Enough to be worth running and quick enough that nobody turns it off.
    /// The whole module is a couple of seconds.
    const ITERATIONS: usize = 4_000;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([12, 34, 56, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Png)
            .expect("a png we just made must encode");
        out.into_inner()
    }

    fn gif(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([200, 30, 30, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Gif)
            .expect("a gif we just made must encode");
        out.into_inner()
    }

    fn bmp(width: u32, height: u32) -> Vec<u8> {
        let image = image::RgbaImage::from_pixel(width, height, image::Rgba([9, 9, 9, 255]));
        let mut out = std::io::Cursor::new(Vec::new());
        image::DynamicImage::ImageRgba8(image)
            .write_to(&mut out, image::ImageFormat::Bmp)
            .expect("a bmp we just made must encode");
        out.into_inner()
    }

    /// A real `.cur`, and a real `.ani` with three frames.
    ///
    /// Seeded from this app's own writers rather than hand-assembled, so the
    /// mutations start from a structurally valid file and spend their budget on
    /// the interesting failures — a length that overruns, a frame count that
    /// disagrees with the frames, a directory entry pointing at itself.
    fn cur(size: u32) -> Vec<u8> {
        let art = crate::build::bitmap::Bitmap::new(size, size);
        crate::build::cur_writer::write_cur(&[crate::build::cur_writer::CursorImage::new(
            art,
            (1, 1),
        )])
        .expect("a cursor we just made must encode")
    }

    fn ani(size: u32) -> Vec<u8> {
        let frames: Vec<crate::build::ani_writer::AniFrame> = (0..3)
            .map(|_| crate::build::ani_writer::AniFrame {
                images: vec![crate::build::cur_writer::CursorImage::new(
                    crate::build::bitmap::Bitmap::new(size, size),
                    (0, 0),
                )],
                delay_ms: 100,
            })
            .collect();
        crate::build::ani_writer::write_ani(
            &frames,
            1.0,
            &crate::build::ani_writer::AniMetadata::default(),
        )
        .expect("an animation we just made must encode")
    }

    /// The cursor-file readers, which parse offsets and lengths a stranger
    /// wrote.
    ///
    /// Every other decoder in this app belongs to a crate that has been fuzzed
    /// by its own maintainers. These two are ours, they are reached by dragging
    /// a file onto the window, and every field in both formats is an index into
    /// the buffer — which is the shape of bug that ends in a panic in a release
    /// build with no console to print it.
    #[test]
    fn reading_a_cursor_file_never_panics() {
        let seeds = vec![cur(32), ani(32), cur(1), b"RIFF\x00\x00\x00\x00ACON".to_vec()];
        hammer("icon_reader::decode_icon", &seeds, ITERATIONS, |bytes| {
            let _ = crate::build::icon_reader::decode_icon(bytes);
            let _ = crate::build::icon_reader::decode_ani(bytes);
            let _ = crate::build::icon_reader::hotspot_fraction(bytes);
        });
    }

    /// The format identifier. Everything downstream trusts its answer, so it is
    /// the first thing that has to survive nonsense.
    #[test]
    fn sniffing_a_format_never_panics() {
        let seeds = vec![png(8, 8), gif(8, 8), bmp(8, 8), b"MZ\x90\x00".to_vec()];
        hammer("pipeline::sniff", &seeds, ITERATIONS, |bytes| {
            let _ = crate::build::pipeline::sniff(bytes);
        });
    }

    /// The image decoder, through our guards.
    ///
    /// Fewer iterations because each one may decode a real image, and the point
    /// is the guards rather than the throughput. This is the entry point every
    /// dropped file reaches.
    #[test]
    fn decoding_an_image_never_panics() {
        let seeds = vec![png(16, 16), gif(16, 16), bmp(16, 16), cur(16), ani(16)];
        hammer("pipeline::decode", &seeds, 600, |bytes| {
            let _ = crate::build::pipeline::decode(bytes.to_vec());
        });
    }

    /// The `.cfpack` manifest — the one structure this app accepts from a
    /// stranger, by design.
    #[test]
    fn a_cfpack_manifest_never_panics() {
        let manifest = br##"{
            "format": 1,
            "name": "PLASMA",
            "basePack": "precision-gap-cross",
            "tint": "#2E8BFF",
            "size": 48,
            "outline": true,
            "overrides": { "Arrow": "my-cursor" },
            "author": "someone",
            "created": "2026-08-15T00:00:00Z"
        }"##;
        let seeds = vec![manifest.to_vec(), b"{}".to_vec(), b"[]".to_vec()];
        hammer("cfpack manifest", &seeds, ITERATIONS, |bytes| {
            let Ok(text) = std::str::from_utf8(bytes) else {
                return;
            };
            let _ = serde_json::from_str::<crate::packs::cfpack::Manifest>(text);
        });
    }

    /// Every state file the app reads at startup. A panic in one of these is a
    /// launch that fails before there is a window to say why.
    #[test]
    fn the_state_files_never_panic() {
        let settings = br##"{"launchOnStartup":true,"tint":"#2E8BFF","cursorSize":48,
            "hotkeyPresets":["Ctrl+Alt+1"],"animationSpeed":1.0,"applyMode":"Blend"}"##;
        let presets = br##"[{"id":"a","name":"P","created":"x","basePack":"p",
            "tint":"#2E8BFF","size":48,"outline":true,"hotkey":null}]"##;
        let scheme = br#"{"values":{"Arrow":"a.cur"},"cursorBaseSize":32,
            "schemeName":"Windows Aero","capturedAt":"x","provenance":"captured"}"#;

        let settings = vec![settings.to_vec()];
        let presets = vec![presets.to_vec()];
        let scheme = vec![scheme.to_vec()];

        hammer("settings.json", &settings, ITERATIONS, |bytes| {
            if let Ok(text) = std::str::from_utf8(bytes) {
                if let Ok(parsed) = serde_json::from_str::<crate::state::settings::Settings>(text) {
                    // Sanitising is where the clamps live, and a clamp on a
                    // NaN or a wildly out-of-range value is a real place to
                    // panic.
                    let _ = parsed.sanitised();
                }
            }
        });

        hammer("presets.json", &presets, ITERATIONS, |bytes| {
            if let Ok(text) = std::str::from_utf8(bytes) {
                let _ = serde_json::from_str::<Vec<crate::state::presets::Preset>>(text);
            }
        });

        hammer("original_scheme.json", &scheme, ITERATIONS, |bytes| {
            if let Ok(text) = std::str::from_utf8(bytes) {
                let _ = serde_json::from_str::<crate::cursor::restore::OriginalScheme>(text);
            }
        });
    }

    /// Names that arrive from a zip entry, and decide where a byte is written.
    #[test]
    fn validating_a_relative_path_never_panics() {
        let seeds = vec![
            br"custom\a-cursor\32.cur".to_vec(),
            br"..\..\Windows\System32\x.dll".to_vec(),
            b"NUL.cur".to_vec(),
            b"a/b/c/d/e/f.png".to_vec(),
        ];
        hammer("paths::validate_relative", &seeds, ITERATIONS, |bytes| {
            let text = String::from_utf8_lossy(bytes);
            let _ = crate::paths::validate_relative(&text);
        });
    }

    /// The mutator has to actually change things, or every test above is
    /// running four thousand copies of a valid file and proving nothing.
    #[test]
    fn the_mutator_mutates() {
        let seed = png(8, 8);
        let mut rng = Rng::new(1);
        let mut different = 0;
        for _ in 0..200 {
            if mutate(&seed, &mut rng) != seed {
                different += 1;
            }
        }
        assert!(different > 190, "only {different}/200 mutations changed anything");
    }

    /// And it has to be reproducible, or a failure report names a seed that
    /// produces something else on the next run.
    #[test]
    fn the_same_seed_produces_the_same_mutations() {
        let seed = png(8, 8);
        let first: Vec<Vec<u8>> = {
            let mut rng = Rng::new(42);
            (0..50).map(|_| mutate(&seed, &mut rng)).collect()
        };
        let second: Vec<Vec<u8>> = {
            let mut rng = Rng::new(42);
            (0..50).map(|_| mutate(&seed, &mut rng)).collect()
        };
        assert_eq!(first, second);
    }

    /// The harness has to be able to fail. A `hammer` that swallowed panics
    /// would pass every test above while proving nothing at all — which is the
    /// same class of mistake as a guard that inspects no files and reports
    /// success.
    #[test]
    fn the_harness_reports_a_panic_rather_than_hiding_it() {
        let caught = std::panic::catch_unwind(|| {
            hammer("a parser that always panics", &[vec![1, 2, 3, 4]], 1, |_| {
                panic!("boom")
            });
        });
        assert!(caught.is_err(), "hammer must not swallow a panic");
    }
}
