//! Turning a pack definition into files Windows can load.
//!
//! Renders are cached by (pack, tint, size, outline). The first apply of a
//! combination does real work; every later one is a directory listing. That is
//! how the apply-latency budget in PRD §12 is met without pre-shipping a
//! gigabyte of bitmaps.

use crate::build::ani_writer::AniMetadata;
use crate::build::bitmap::Bitmap;
use crate::build::cur_writer::{self, CursorImage, TARGET_SIZES};
use crate::build::{pipeline, svg};
use crate::cursor::roles::{Role, ALL_ROLES};
use crate::cursor::scheme::CursorSet;
use crate::error::{AppError, AppResult};
use crate::packs::art;
use crate::packs::styles::{self, PackDef};
use crate::state::settings::HoverStyle;
use crate::paths;
use crate::util::parse_hex_color;
use serde::Serialize;
use std::path::PathBuf;

/// Frames per animated role. Twelve at 60 ms is one smooth revolution per
/// 720 ms — well inside the format's 60-frame ceiling (PRD §6.2).
const ANIMATION_FRAMES: usize = 12;
const ANIMATION_FRAME_MS: u32 = 60;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PackSummary {
    pub id: String,
    pub name: String,
    pub category: &'static str,
    pub author: &'static str,
    pub recolorable: bool,
    pub animated: bool,
    /// A ready-to-render `data:` URI, so the catalog grid needs no file access.
    pub preview: String,
}

/// How a pack is rendered for a particular user choice.
#[derive(Debug, Clone)]
pub struct RenderSpec {
    pub tint: String,
    pub size: u32,
    pub outline: bool,
}

/// Bumped whenever the renderer produces different pixels for the same inputs.
///
/// The cache key covers the *choices* a user makes — colour, size, outline — and
/// nothing about the code that turns them into a file. So a change to the
/// renderer leaves every existing entry stale but still matching its key, and
/// the fix silently never reaches anyone who had already applied that cursor.
/// That is invisible from the developer's side, where the cache is usually
/// empty, and permanent from the user's, where it is not.
///
/// v2: the hand and the I-beam stopped growing with the pointer.
const RENDER_VERSION: u32 = 2;

/// A fingerprint of the size ladder, folded into the cache key.
///
/// **This exists because the paragraph above came true.** 1.24.0 added the 192
/// and 256 rungs and did not touch this file, so every machine that had already
/// rendered a pack kept serving eight-rung cursors while a machine installing
/// fresh got ten — the same app, the same pack, two different results, and the
/// difference invisible to anyone whose cache was empty. It was found on a
/// machine whose cache entries were dated two days before the release that was
/// supposed to have changed them.
///
/// Bumping `RENDER_VERSION` by hand would have prevented it, and did not,
/// because remembering is not a mechanism. The ladder is the input most likely
/// to change and the easiest to change without thinking about the cache, so it
/// now keys itself: edit `TARGET_SIZES` and every cached cursor is invalidated
/// whether or not anybody remembered to say so.
///
/// FNV-1a, truncated to sixteen bits. This is a cache key and not a security
/// boundary — it only has to change when the ladder does.
const fn ladder_tag() -> u16 {
    fingerprint(&TARGET_SIZES)
}

/// Taken over a slice rather than over the constant directly, so a test can ask
/// what a *different* ladder would key to. A property this cache depends on is
/// worth being able to state as an assertion instead of by inspection.
const fn fingerprint(sizes: &[u32]) -> u16 {
    let mut hash: u32 = 0x811c_9dc5;
    let mut i = 0;
    while i < sizes.len() {
        let mut value = sizes[i];
        let mut byte = 0;
        while byte < 4 {
            hash ^= value & 0xff;
            hash = hash.wrapping_mul(0x0100_0193);
            value >>= 8;
            byte += 1;
        }
        i += 1;
    }
    (hash & 0xffff) as u16
}

/// Whether the user has asked for every role to follow the size control.
fn scale_all_roles() -> bool {
    crate::state::settings::get().scale_all_roles
}

/// The pixel size a role's artwork is drawn at, for a given ladder entry.
///
/// One place, so the ladder, the animated build and the live override cannot
/// disagree about how large a hand is — which is how a cursor ends up one size
/// until the watchdog reloads it and another size afterwards.
pub fn glyph_size(role: Role, entry: u32) -> u32 {
    if scale_all_roles() {
        entry
    } else {
        role.size_from(entry)
    }
}

impl RenderSpec {
    fn rgb(&self) -> AppResult<[u8; 3]> {
        parse_hex_color(&self.tint)
            .ok_or_else(|| AppError::invalid(format!("{} is not a color", self.tint)))
    }

    /// Cache directory name. Every input that changes a pixel is in the key, so
    /// a stale cache entry cannot be served for a different choice.
    ///
    /// **The size is only part of it for an `.ani`.** A static cursor's ladder is
    /// rendered over `TARGET_SIZES` and every rung's glyph is derived from the
    /// rung, never from `spec.size` — so the file produced at 32 px and the file
    /// produced at 33 px are byte for byte the same. Keying them apart anyway
    /// meant dragging the size slider from one end to the other wrote a fresh,
    /// identical copy of all seventeen roles at every pixel it passed through:
    /// one pack on this developer's machine had 150 such directories and the
    /// cache had reached 395 MB. An `.ani` genuinely is built at one size, so
    /// that one keeps it.
    fn key(&self, animated: bool) -> String {
        let size = if animated {
            format!(
                "-{}",
                self.size.clamp(
                    crate::state::settings::MIN_CURSOR_PX,
                    crate::state::settings::MAX_CURSOR_PX
                )
            )
        } else {
            String::new()
        };
        format!(
            "v{RENDER_VERSION}.{:04x}-{}{}-{}{}",
            ladder_tag(),
            self.tint.trim_start_matches('#').to_ascii_lowercase(),
            size,
            if self.outline { "o" } else { "n" },
            // Whether the hand and I-beam scale changes their pixels, so it has
            // to change their cache entry. Read here rather than carried through
            // every RenderSpec because it is one global preference, not a
            // property of the thing being rendered.
            if scale_all_roles() { "-a" } else { "" }
        )
    }
}

/// Every pack that ships inside the executable.
///
/// There is deliberately no switch here. A `const SHOW_GENERATED_PACKS: bool`
/// used to gate this, and setting it to `false` emptied the catalog on every
/// machine that had not imported a folder — which was all of them but one. The
/// app was not broken and nothing errored; there was simply nothing to show.
///
/// These packs are the only cursors that exist on a machine which has just
/// installed Cursed. They are defined in Rust and compiled into the binary, so
/// they need no download, no unpacking, no network and no database. An import is
/// an *addition* to this library. It is never the library.
fn built_in_summaries() -> AppResult<Vec<PackSummary>> {
    styles::all()
        .into_iter()
        .map(|pack| {
            let preview = preview_uri(&pack, pack.default_tint)?;
            Ok(PackSummary {
                id: pack.id.to_owned(),
                name: pack.name.to_owned(),
                // The built-ins are the clean, geometric, recolourable half of
                // the catalog — which is what MINIMAL CURSED was named for and
                // left empty waiting on.
                category: "MINIMAL CURSED",
                author: "feylo",
                recolorable: true,
                animated: pack.animated,
                preview,
            })
        })
        .collect()
}

pub fn list_summaries() -> AppResult<Vec<PackSummary>> {
    let mut out = built_in_summaries()?;

    // The user's own imports sit at the front: they went to the trouble of
    // adding them, so they should not have to scroll past 216 built-ins first.
    //
    // These are not recolourable. They are somebody's finished artwork, and
    // tinting a Batman logo blue would just break it — the tint pass is designed
    // for our own greyscale masters, not for arbitrary full-colour images.
    let mut imported: Vec<PackSummary> = crate::import::list()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|pack| {
            let preview = crate::import::preview(&pack).ok()?;
            Some(PackSummary {
                id: pack.id.clone(),
                name: pack.name.clone(),
                category: leak_category(&pack.category),
                author: "imported",
                recolorable: false,
                animated: pack.animated,
                preview,
            })
        })
        .collect();

    imported.append(&mut out);
    Ok(imported)
}

/// Imported categories are user data, but `PackSummary` carries a `&'static
/// str` for the built-ins. The set is small and closed, so map onto known
/// values and fall back rather than leaking memory for arbitrary strings.
fn leak_category(category: &str) -> &'static str {
    match category {
        "MINIMAL CURSED" => "MINIMAL CURSED",
        _ => "OPTIMAL CURSED",
    }
}

/// A tinted 64 px render of the pack's Arrow, as a `data:` URI.
pub fn preview_uri(pack: &PackDef, tint: &str) -> AppResult<String> {
    let rgb = parse_hex_color(tint).unwrap_or([0xed, 0xf1, 0xf7]);
    let markup = art::render_role(&pack.style, Role::Arrow, 0.0);
    svg::render(&markup, 64)?.tinted(rgb).to_png_data_uri()
}

fn finish_for(spec: &RenderSpec) -> AppResult<pipeline::Finish> {
    Ok(pipeline::Finish {
        tint: Some(spec.rgb()?),
        opacity: 1.0,
        outline: spec.outline,
    })
}

fn role_bitmap(pack: &PackDef, role: Role, size: u32, phase: f32) -> AppResult<Bitmap> {
    svg::render(&art::render_role(&pack.style, role, phase), size)
}

/// Builds one role's file, or returns the cached one.
fn build_role(pack: &PackDef, role: Role, spec: &RenderSpec) -> AppResult<PathBuf> {
    let animated = pack.animated && role.is_animatable();
    let extension = if animated { "ani" } else { "cur" };
    let dir = paths::cache_dir()?.join(pack.id).join(spec.key(animated));
    let file = dir.join(format!("{}.{extension}", role.file_stem()));
    if file.exists() {
        return Ok(file);
    }
    std::fs::create_dir_all(&dir)?;

    let finish = finish_for(spec)?;
    let (hx, hy) = art::hotspot(role);
    let bytes = if animated {
        // `.ani` has no directory of resolutions, so it is built at the one size
        // Windows is currently drawing (PRD §5.4) — which for the hand and the
        // I-beam is capped, since those do not scale with the pointer.
        //
        // At *exactly* that size, not snapped to a ladder rung. The rungs exist
        // so that one `.cur` can carry eight resolutions; an `.ani` carries one
        // and is drawn at whatever `CursorBaseSize` says. Snapping meant the
        // slider at 78 px built a 64 px animation and left the shell to stretch
        // it by a fifth — bilinear, unpremultiplied, no gamma correction — while
        // the static cursor beside it stayed sharp. This artwork is vector, so
        // the exact size costs nothing to render and reaches the screen 1:1.
        let size = glyph_size(role, spec.size);
        let frames: AppResult<Vec<(Bitmap, u32)>> = (0..ANIMATION_FRAMES)
            .map(|i| {
                let phase = i as f32 / ANIMATION_FRAMES as f32;
                Ok((role_bitmap(pack, role, size, phase)?, ANIMATION_FRAME_MS))
            })
            .collect();
        pipeline::build_ani(
            &frames?,
            (hx, hy),
            &finish,
            size,
            1.0,
            &AniMetadata {
                name: Some(pack.name.to_owned()),
                author: Some("feylo".to_owned()),
            },
        )?
    } else {
        // Rendered per size from the vector rather than resampled from one
        // bitmap: this is what keeps 256 px genuinely sharp (PRD §5).
        let mut images = Vec::with_capacity(TARGET_SIZES.len());
        for size in TARGET_SIZES {
            // The hand and the I-beam do not grow with the pointer, and this is
            // the only place that can honour it. `size_from` was applied on the
            // animated branch alone, so every static ladder was rendered at the
            // full pointer size and Windows drew a 128 px hand beside a 128 px
            // arrow — which is what made a large pointer unusable.
            //
            // The glyph is drawn at its own size and centred in the entry's
            // canvas, rather than the entry being made smaller. Windows picks an
            // entry by `CursorBaseSize` and scales it, so a short ladder would
            // just be scaled back up; a full-size canvas holding a small glyph
            // comes through untouched.
            let glyph = glyph_size(role, size);
            let rendered = role_bitmap(pack, role, glyph, 0.0)?;
            let coloured = rendered.tinted(finish.tint.unwrap_or([255, 255, 255]));
            let outlined = if finish.outline {
                coloured.with_contrast_outline()
            } else {
                coloured
            };
            let finished = outlined.centred_in(size);

            // The hotspot follows the glyph into the middle of the canvas. Left
            // as a fraction of the whole canvas it would drift toward the centre
            // as the pointer grew, so every click would land further from the
            // tip the larger the cursor got.
            let max = (size - 1) as f32;
            let offset = (size.saturating_sub(glyph) / 2) as f32;
            let span = glyph.saturating_sub(1) as f32;
            images.push(CursorImage::new(
                finished,
                (
                    (offset + hx * span).round().clamp(0.0, max) as u16,
                    (offset + hy * span).round().clamp(0.0, max) as u16,
                ),
            ));
        }
        cur_writer::write_cur(&images)?
    };

    // Write to a temporary name and rename into place: a reader that arrives
    // mid-write must never see a half-formed cursor file. The temp name is
    // unique per attempt because a hover preview and a commit can legitimately
    // build the same role at the same moment, and they must not collide.
    let temp = file.with_extension(format!(
        "{extension}.{}.tmp",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::write(&temp, &bytes)?;

    // Prove Windows accepts it *before* it is visible under its real name, so a
    // rejected cursor is never even briefly installable (PRD §6.1 step 6).
    if let Err(e) = crate::cursor::engine::verify_loadable(&temp) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }

    // A concurrent build may have won the race; its bytes are identical, so
    // either outcome is correct and the loser simply drops its copy.
    if let Err(e) = std::fs::rename(&temp, &file) {
        let _ = std::fs::remove_file(&temp);
        if !file.exists() {
            return Err(e.into());
        }
    }
    Ok(file)
}

/// Builds the given roles. They are independent, so they are rendered across
/// threads — the difference between a ~1 s first apply and a ~4 s one.
///
/// Only the roles that will actually be installed are built. Rendering all
/// seventeen and discarding sixteen would spend most of the apply-latency budget
/// on files nobody asked for.
pub fn build_roles(pack_id: &str, roles: &[Role], spec: &RenderSpec) -> AppResult<CursorSet> {
    let pack = styles::find(pack_id).ok_or(AppError::UnknownPack)?;
    if roles.is_empty() {
        return Err(AppError::invalid("a scheme with no roles would do nothing"));
    }

    let workers = std::thread::available_parallelism()
        .map(|n| n.get().clamp(1, 8))
        .unwrap_or(4)
        .min(roles.len());

    let results = std::thread::scope(|scope| {
        let handles: Vec<_> = (0..workers)
            .map(|worker| {
                // Stride rather than block: adjacent roles have wildly different
                // render costs, so interleaving keeps the threads even.
                let slice: Vec<Role> = roles.iter().copied().skip(worker).step_by(workers).collect();
                let pack = &pack;
                scope.spawn(move || {
                    slice
                        .into_iter()
                        .map(|role| (role, build_role(pack, role, spec)))
                        .collect::<Vec<_>>()
                })
            })
            .collect();

        handles
            .into_iter()
            .filter_map(|handle| handle.join().ok())
            .flatten()
            .collect::<Vec<_>>()
    });

    let mut set = CursorSet::default();
    for (role, result) in results {
        set.insert(role, result?);
    }
    if set.files.len() != roles.len() {
        return Err(AppError::invalid(format!(
            "{pack_id} produced {} of {} roles",
            set.files.len(),
            roles.len()
        )));
    }
    Ok(set)
}

/// The generated pack every stale id falls back to.
///
/// Deliberately the same id `Settings::sanitised` uses for a stale `blend_pack`.
/// One substitute, decided in one place.
pub const BLEND_BASE: &str = "precision-gap-cross";

/// The pack to fill the roles somebody else's artwork does not define.
///
/// Applies **only** to the backing pack, never to a pack the user actually
/// picked: asking for a cursor that does not exist is an error worth reporting,
/// and `build_roles` still reports it. A backing pack is different. It is not a
/// choice so much as a floor, and the right answer when the floor is missing is
/// to put a floor back.
///
/// This matters because the generated catalog was 291 packs and is now one.
/// Every descriptor, preset and `.cfpack` written before that names one of the
/// 290, and a stale id here made `custom::build_set` fail — which the user
/// experiences as the size, colour and outline controls doing nothing at all.
/// The setting saves, the redraw fails, and the only trace is one `warn` line in
/// a log nobody reads. It happened seven times on this machine before anyone
/// noticed, because there is nothing on screen to notice.
///
/// `Settings::sanitised` performs exactly this repair for `settings.blend_pack`.
/// This is the same repair for the copies that live outside settings — in the
/// applied descriptor and in every saved preset.
pub fn resolve_blend_base(pack_id: &str) -> String {
    if styles::find(pack_id).is_some() {
        return pack_id.to_owned();
    }
    log::warn!("the blend base {pack_id} is no longer in the catalog; using {BLEND_BASE}");
    BLEND_BASE.to_owned()
}

/// True for a pack the user imported rather than one we ship.
pub fn is_imported(pack_id: &str) -> bool {
    pack_id.starts_with("user:")
}

/// Builds the scheme for an imported pack.
///
/// An import usually defines one or two roles — an arrow and a hand. The other
/// fifteen come from a built-in pack so the pointer set stays coherent, which is
/// the same reasoning as Blend mode for custom images: a lone custom arrow next
/// to fifteen stock Windows cursors looks broken, not customised.
pub fn build_imported(pack_id: &str, base: &str, spec: &RenderSpec) -> AppResult<CursorSet> {
    let pack = crate::import::get(pack_id)?;
    let files = crate::import::role_files(&pack)?;

    // The base is a floor under somebody else's artwork, not a choice the user
    // made here, so a stale id is repaired rather than reported.
    let mut set = build_roles(&resolve_blend_base(base), &ALL_ROLES, spec)?;

    // Roles the import does not define, but which are still *the pointer*, take
    // the import's own arrow rather than the generated base pack's artwork.
    //
    // Most downloaded packs define an arrow and a hand and nothing else. Filling
    // the other fifteen from an unrelated pack meant that the moment anything on
    // the machine started working — a copy, a download, an app launching — the
    // pointer turned into a completely different design. From the outside that
    // reads as the cursor having reverted to the system default, because the one
    // shape the user recognises has gone.
    //
    // The directional and text roles are deliberately left alone. A resize
    // handle that looks like an arrow says nothing about which way to drag, and
    // an I-beam that is an arrow hides where text will land. Those keep the base
    // pack's purpose-built shapes.
    if let Some(arrow) = files.get(&Role::Arrow) {
        for role in POINTER_LIKE_ROLES {
            if !files.contains_key(&role) {
                set.insert(role, arrow.clone());
            }
        }
    }

    for (role, path) in files {
        set.insert(role, path);
    }
    Ok(set)
}

/// Roles that are the ordinary pointer wearing a different hat.
///
/// These read as "your cursor, busy" or "your cursor, over something odd"
/// rather than as a distinct tool, so an imported pack's own arrow serves them
/// better than a stranger's artwork does.
const POINTER_LIKE_ROLES: [Role; 6] = [
    Role::AppStarting,
    Role::Wait,
    Role::Help,
    Role::No,
    Role::UpArrow,
    Role::Person,
];

/// Replaces the hand in a finished set, if the user asked for something else.
///
/// **Applied here, to the assembled set, rather than inside `build_role`.** The
/// hand can arrive from three unrelated places — drawn from a generated pack,
/// copied out of an imported pack's own files, or built from the user's image —
/// and the choice has to mean the same thing in all three. Doing it once at the
/// end is the only way that is true by construction rather than by three
/// implementations agreeing.
///
/// A no-op for [`HoverStyle::Pack`], which is what every set already was.
pub fn apply_hover_style(
    set: &mut CursorSet,
    style: HoverStyle,
    spec: &RenderSpec,
) -> AppResult<()> {
    match style {
        HoverStyle::Pack => {}
        HoverStyle::Pointer => {
            // The same file, not a re-render of it. Whatever the arrow ended up
            // being — generated, imported, a photograph the user cut out — the
            // hand becomes exactly that, so hovering a link changes nothing.
            //
            // A set with no arrow is left alone rather than emptied of its hand:
            // that only happens for a partial apply, and a missing hand is worse
            // than an unexpected one.
            if let Some(arrow) = set.files.get(&Role::Arrow).cloned() {
                set.insert(Role::Hand, arrow);
            }
        }
        HoverStyle::Mark => {
            set.insert(Role::Hand, build_mark_hand(spec)?);
        }
    }
    Ok(())
}

/// The Cursed mark, built as a hand cursor.
///
/// Cached under its own pack id so it cannot collide with a real pack's
/// directory, and keyed by the same spec as everything else — the mark is tinted
/// and outlined like any other role, because a hand that ignored the accent
/// color would be the one part of the pointer set that did not match.
///
/// Rendered per rung from the vector for the same reason `build_role` does it:
/// resampling one bitmap is what makes a large cursor soft.
fn build_mark_hand(spec: &RenderSpec) -> AppResult<PathBuf> {
    let dir = paths::cache_dir()?.join("_mark-hand").join(spec.key(false));
    let file = dir.join("Hand.cur");
    if file.exists() {
        return Ok(file);
    }
    std::fs::create_dir_all(&dir)?;

    let finish = finish_for(spec)?;
    let mut images = Vec::with_capacity(TARGET_SIZES.len());
    for size in TARGET_SIZES {
        // Follows the same cap as any other hand: `glyph_size` is what decides
        // whether the hand grows with the pointer, and the mark is a hand.
        let glyph = glyph_size(Role::Hand, size);
        let rendered = svg::render(&crate::packs::brand::small_mark_svg("#ffffff"), glyph)?;
        let colored = rendered.tinted(finish.tint.unwrap_or([255, 255, 255]));
        let outlined = if finish.outline {
            colored.with_contrast_outline()
        } else {
            colored
        };
        let finished = outlined.centred_in(size);

        // The mark is a pointer seen almost edge-on, so its hotspot is the tip:
        // the top-left corner of the glyph, moved with it into the middle of the
        // canvas exactly as `build_role` does.
        let max = (size - 1) as f32;
        let offset = (size.saturating_sub(glyph) / 2) as f32;
        let span = glyph.saturating_sub(1) as f32;
        images.push(CursorImage::new(
            finished,
            (
                (offset + 0.06 * span).round().clamp(0.0, max) as u16,
                (offset + 0.04 * span).round().clamp(0.0, max) as u16,
            ),
        ));
    }

    let bytes = cur_writer::write_cur(&images)?;
    let temp = dir.join(format!("Hand.{}.tmp", std::process::id()));
    std::fs::write(&temp, &bytes)?;
    if let Err(e) = crate::cursor::engine::verify_loadable(&temp) {
        let _ = std::fs::remove_file(&temp);
        return Err(e);
    }
    if let Err(e) = std::fs::rename(&temp, &file) {
        let _ = std::fs::remove_file(&temp);
        if !file.exists() {
            return Err(e.into());
        }
    }
    Ok(file)
}

/// Builds a complete 17-role scheme.
pub fn build_set(pack_id: &str, spec: &RenderSpec) -> AppResult<CursorSet> {
    let set = build_roles(pack_id, &ALL_ROLES, spec)?;
    if !set.is_complete() {
        return Err(AppError::invalid(format!(
            "{pack_id} did not produce all 17 roles"
        )));
    }
    Ok(set)
}
pub fn display_name(pack_id: &str) -> Option<&'static str> {
    styles::find(pack_id).map(|pack| pack.name)
}

pub fn default_tint(pack_id: &str) -> Option<&'static str> {
    styles::find(pack_id).map(|pack| pack.default_tint)
}

/// Total bytes of rendered cursors on disk.
///
/// Read by the maintainer's report in `commands::get_diagnostics` and nothing
/// else. There is no longer a way for a user to see this or to clear it: the
/// cache is how the apply-latency budget in PRD §12 is met, emptying it only
/// makes the next apply of every pack slow again, and a number a user can do
/// nothing useful with does not belong in Settings.
pub fn cache_size() -> AppResult<u64> {
    fn walk(dir: &std::path::Path) -> u64 {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return 0;
        };
        entries
            .filter_map(Result::ok)
            .map(|entry| match entry.file_type() {
                Ok(kind) if kind.is_dir() => walk(&entry.path()),
                Ok(_) => entry.metadata().map(|m| m.len()).unwrap_or(0),
                Err(_) => 0,
            })
            .sum()
    }
    Ok(walk(&paths::cache_dir()?))
}

/// Deletes cache directories rendered by a different version of the renderer.
///
/// Invalidation by key is what makes a stale entry unreachable; it is not what
/// makes it go away. Without this, every renderer change leaves the previous
/// generation on disk forever — and the ladder change that prompted all of this
/// roughly quadrupled what one entry costs, so the next generation is the
/// expensive one to keep a dead copy of.
///
/// Matched on the key prefix rather than by listing what is live: a directory
/// this build would never write is one this build cannot need. Failures are
/// ignored throughout — a cache that could not be tidied is not a reason to fail
/// a launch.
pub fn sweep_stale_cache() {
    let Ok(root) = paths::cache_dir() else { return };
    let current = format!("v{RENDER_VERSION}.{:04x}-", ladder_tag());
    let mut removed = 0usize;

    let Ok(packs) = std::fs::read_dir(&root) else { return };
    for pack in packs.flatten() {
        let Ok(variants) = std::fs::read_dir(pack.path()) else { continue };
        for variant in variants.flatten() {
            if !variant.file_type().map(|k| k.is_dir()).unwrap_or(false) {
                continue;
            }
            let name = variant.file_name();
            let name = name.to_string_lossy();
            if name.starts_with(&current) {
                continue;
            }
            if std::fs::remove_dir_all(variant.path()).is_ok() {
                removed += 1;
            }
        }
    }

    if removed > 0 {
        log::info!("cache: removed {removed} directories from an older renderer");
    }
}

/// Exports a pack's SVG masters and manifest, for the repo's `assets/packs`
/// tree. The runtime never reads these — they exist so the artwork is reviewable
/// and contributable rather than buried in a binary.
pub fn export_sources(pack: &PackDef, into: &std::path::Path) -> AppResult<()> {
    let dir = into.join(pack.id);
    std::fs::create_dir_all(&dir)?;

    let mut roles = serde_json::Map::new();
    for role in ALL_ROLES {
        let file_name = format!("{}.svg", role.file_stem());
        std::fs::write(dir.join(&file_name), art::render_role(&pack.style, role, 0.0))?;
        let (hx, hy) = art::hotspot(role);
        roles.insert(
            role.registry_value().to_owned(),
            serde_json::json!({ "src": file_name, "hotspot": [hx, hy] }),
        );
    }

    let manifest = serde_json::json!({
        "id": pack.id,
        "name": pack.name,
        "category": pack.category.as_str(),
        "author": "feylo",
        "license": "MIT",
        "version": "1.0.0",
        "recolorable": true,
        "animated": pack.animated,
        "roles": roles,
    });
    std::fs::write(
        dir.join("pack.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn plain_spec() -> RenderSpec {
        RenderSpec { tint: "#2E8BFF".into(), size: 32, outline: false }
    }

    fn a_set_with_two_different_roles() -> CursorSet {
        let mut set = CursorSet::default();
        set.insert(Role::Arrow, PathBuf::from("arrow.cur"));
        set.insert(Role::Hand, PathBuf::from("the-packs-own-hand.cur"));
        set
    }

    /// The default changes nothing, which is the whole reason it is the default.
    #[test]
    fn keeping_the_packs_hand_leaves_the_set_exactly_as_it_was() {
        let mut set = a_set_with_two_different_roles();
        apply_hover_style(&mut set, HoverStyle::Pack, &plain_spec()).unwrap();
        assert_eq!(set.get(Role::Hand), Some(PathBuf::from("the-packs-own-hand.cur")).as_deref());
        assert_eq!(set.get(Role::Arrow), Some(PathBuf::from("arrow.cur")).as_deref());
    }

    /// **The complaint this feature exists for.** Somebody picks a cursor,
    /// hovers a link, and gets a completely different drawing. Asking for the
    /// pointer to be the hand has to mean the file is literally the same one —
    /// not a re-render, which could differ, but the same path.
    #[test]
    fn using_the_pointer_for_hovering_makes_the_hand_the_same_file() {
        let mut set = a_set_with_two_different_roles();
        apply_hover_style(&mut set, HoverStyle::Pointer, &plain_spec()).unwrap();
        assert_eq!(
            set.get(Role::Hand),
            set.get(Role::Arrow),
            "hovering a link must not change the pointer at all"
        );
    }

    /// A partial set is left with the hand it has rather than none.
    ///
    /// `ArrowOnly` and the preview path build a subset, and a set with no arrow
    /// to copy is a real state. Removing the hand there would turn "keep my
    /// pointer while hovering" into "have no hand", which is worse than the
    /// thing being asked about.
    #[test]
    fn a_set_with_no_arrow_keeps_whatever_hand_it_had() {
        let mut set = CursorSet::default();
        set.insert(Role::Hand, PathBuf::from("lonely-hand.cur"));
        apply_hover_style(&mut set, HoverStyle::Pointer, &plain_spec()).unwrap();
        assert_eq!(set.get(Role::Hand), Some(PathBuf::from("lonely-hand.cur")).as_deref());
    }

    fn spec() -> RenderSpec {
        RenderSpec {
            tint: "#2E8BFF".into(),
            size: 32,
            outline: true,
        }
    }

    /// The bug this exists to prevent: the catalog was shipped with the
    /// built-ins switched off, so every machine without an import showed an
    /// empty grid. Nothing failed — the app correctly reported an empty
    /// library, because the library was empty.
    ///
    /// A user's imports live in their own `%APPDATA%`, so they cannot be what
    /// makes the catalog non-empty for anybody else. Only the compiled-in packs
    /// can, and this asserts they are actually offered.
    /// The regression that made animated packs sit still for half a minute.
    ///
    /// An animated pack builds `.ani` files, and those have to survive the live
    /// layer's loader with their frames intact. Applying one through the static
    /// path installs a frozen first frame over the animated cursor Windows had
    /// just loaded from the registry, and nothing moves until the watchdog
    /// reloads the scheme.
    ///
    /// This builds a real animated pack and puts every file it produces through
    /// the same loader `set_role` will use.
    ///
    /// The pack is built here rather than taken from the catalog. The catalog
    /// used to ship eighteen animated packs and now ships none — it is one
    /// static blend base — but nothing about the `.ani` writer or the loader
    /// changed with it, and this is the test that caught an `.ani` being
    /// flattened to a single frame. Deleting it along with the packs would have
    /// thrown away the guard and kept the bug.
    #[test]
    fn an_animated_pack_produces_files_that_load_as_animated() {
        let base = styles::find("precision-gap-cross").expect("the blend base");
        let pack = styles::PackDef { animated: true, ..base };

        let mut checked = 0usize;
        for role in crate::cursor::roles::ALL_ROLES {
            if !role.is_animatable() {
                continue;
            }
            let path = build_role(&pack, role, &spec()).expect("an animated role builds");
            assert!(
                crate::cursor::engine::is_animated(&path),
                "{role} of an animated pack was not written as an .ani"
            );
            assert!(
                crate::cursor::engine::verify_loadable(&path).is_ok(),
                "{role} did not load as an animated cursor"
            );
            checked += 1;
        }
        assert!(checked > 0, "no animatable roles were exercised");
    }

    #[test]
    fn the_built_in_catalog_is_never_empty() {
        // Deliberately goes through the same call the catalog screen makes,
        // rather than reading the flag: what matters is what a fresh machine
        // actually receives, and the flag is only one way to get that wrong.
        let summaries = list_summaries().expect("the catalog must load");
        let built_in = summaries.iter().filter(|s| s.author == "feylo").count();

        // This used to demand a hundred. The generated catalog is one pack now —
        // the blend base — and what a fresh machine actually receives is the
        // bundled archives, which `bundled` guards with a count of its own.
        // Repeating that here would assert the same thing twice, in the weaker
        // of the two places.
        assert!(
            built_in >= 1,
            "the built-in blend base did not reach the catalog; every imported \
             cursor depends on it for the roles it does not define"
        );

        // Every tile must arrive as a data URI. The webview holds no filesystem
        // capability and the asset protocol is disabled, so any other shape of
        // preview silently renders nothing.
        for summary in summaries.iter().filter(|s| s.author == "feylo") {
            assert!(
                summary.preview.starts_with("data:image/png;base64,"),
                "{} did not produce a data URI",
                summary.id
            );
        }
    }

    /// Two packs with the same name are indistinguishable in the catalog, in
    /// the tray tooltip, and in "USING ...". The id keeps them apart internally;
    /// only the name keeps them apart for the person choosing.
    #[test]
    fn built_in_pack_names_are_unique() {
        let mut seen = std::collections::BTreeMap::new();
        for pack in styles::all() {
            if let Some(other) = seen.insert(pack.name.to_owned(), pack.id) {
                panic!("{} and {} are both called {}", other, pack.id, pack.name);
            }
        }
    }

    /// Two packs sharing an id would collide in the cache directory, so one
    /// would serve the other's rendered files.
    #[test]
    fn built_in_pack_ids_are_unique() {
        let mut seen = std::collections::BTreeSet::new();
        for pack in styles::all() {
            assert!(seen.insert(pack.id.to_owned()), "duplicate pack id: {}", pack.id);
        }
    }

    /// **The regression test for the one-machine blocky pointer.**
    ///
    /// 1.24.0 added the 192 and 256 rungs without touching this file, so a
    /// machine that had already rendered a pack went on serving eight-rung
    /// cursors — stale, topping out at 128 — while a fresh install got ten. Same
    /// app, same pack, two different results, and invisible to anyone whose
    /// cache was empty.
    ///
    /// The key now carries a fingerprint of the ladder, so this cannot recur
    /// silently. If you have changed `TARGET_SIZES` and this test fails, it has
    /// done its job: the expected tag below is the new one.
    #[test]
    fn changing_the_ladder_invalidates_every_cached_cursor() {
        let current = fingerprint(&TARGET_SIZES);

        // One rung more, one fewer, and one moved. None may key the same.
        let longer: Vec<u32> = TARGET_SIZES.iter().copied().chain([384]).collect();
        let shorter: Vec<u32> = TARGET_SIZES[..TARGET_SIZES.len() - 1].to_vec();
        let mut moved = TARGET_SIZES;
        moved[3] += 1;

        assert_ne!(current, fingerprint(&longer), "an added rung must invalidate");
        assert_ne!(current, fingerprint(&shorter), "a removed rung must invalidate");
        assert_ne!(current, fingerprint(&moved), "a changed rung must invalidate");

        // And the same ladder must key the same, or every launch rebuilds.
        assert_eq!(current, fingerprint(&TARGET_SIZES), "stable for one ladder");
    }

    /// The ladder that shipped in 1.23.0, which is what the stale entries on the
    /// reporting machine were rendered against. Its key must differ from
    /// today's, which is the precise statement of the bug.
    #[test]
    fn the_pre_1_24_ladder_keys_differently_from_the_current_one() {
        const BEFORE: [u32; 8] = [10, 16, 24, 32, 48, 64, 96, 128];
        assert_ne!(
            fingerprint(&BEFORE),
            fingerprint(&TARGET_SIZES),
            "an eight-rung cache entry must not satisfy a ten-rung key"
        );
    }

    #[test]
    fn cache_keys_separate_every_visual_choice() {
        let base = spec();
        let bigger = RenderSpec { size: 48, ..base.clone() };
        let plain = RenderSpec { outline: false, ..base.clone() };
        let red = RenderSpec { tint: "#FF0000".into(), ..base.clone() };

        assert_ne!(base.key(true), bigger.key(true));
        assert_ne!(base.key(false), plain.key(false));
        assert_ne!(base.key(false), red.key(false));
        // **The suffix is read, not written in.** `key` consults one global
        // preference — whether the hand and I-beam scale — and a literal here
        // asserts the machine's settings rather than the format. It passed on a
        // machine with that preference off and failed on a machine with it on,
        // which made a green suite a fact about the person running it.
        let scaled = if scale_all_roles() { "-a" } else { "" };
        assert_eq!(base.key(true), format!("v{RENDER_VERSION}.{:04x}-2e8bff-32-o{scaled}", ladder_tag()));
        assert_eq!(base.key(false), format!("v{RENDER_VERSION}.{:04x}-2e8bff-o{scaled}", ladder_tag()));

        // The renderer's version is in the key, not only the user's choices. A
        // change to how a pixel is produced leaves every existing entry stale
        // and still matching its key, so the fix reaches nobody who had already
        // applied that cursor — invisible on a developer's empty cache and
        // permanent on a user's full one.
        assert!(
            base.key(false).starts_with(&format!("v{RENDER_VERSION}.")),
            "the render version has to be part of the cache key"
        );
    }

    /// A backing pack that no longer exists must be replaced, not reported.
    ///
    /// This is the difference between the size and colour controls working and
    /// them silently doing nothing for anyone whose cursor was applied while the
    /// catalog still had 291 packs in it.
    #[test]
    fn a_blend_base_that_no_longer_exists_is_replaced() {
        assert_eq!(resolve_blend_base("removed-in-an-older-build"), BLEND_BASE);
        // One that does exist is left exactly alone.
        assert_eq!(resolve_blend_base(BLEND_BASE), BLEND_BASE);
        assert!(styles::find(&resolve_blend_base("gone")).is_some());
    }

    /// The substitute has to be the same one settings uses, or a cursor repairs
    /// itself onto a different pack depending on which code path got there.
    #[test]
    fn the_blend_base_matches_the_one_settings_falls_back_to() {
        let stale = crate::state::settings::Settings {
            blend_pack: "removed-in-an-older-build".into(),
            ..Default::default()
        }
        .sanitised();
        assert_eq!(stale.blend_pack, BLEND_BASE);
    }

    /// A static ladder is identical at every size, so it must not be stored
    /// once per size. This is what turned a cache into 395 MB of duplicates:
    /// one pack alone had 150 directories holding the same seventeen files.
    #[test]
    fn a_static_ladder_is_cached_once_for_every_size() {
        let small = RenderSpec { size: 10, ..spec() };
        let large = RenderSpec { size: 128, ..spec() };
        assert_eq!(
            small.key(false),
            large.key(false),
            "the static ladder does not depend on the size, so its key must not either"
        );
        // An `.ani` really is built at one size, so it keeps the distinction.
        assert_ne!(small.key(true), large.key(true));
    }

    #[test]
    fn an_unknown_pack_is_reported_not_guessed() {
        assert!(matches!(
            build_set("no-such-pack", &spec()),
            Err(AppError::UnknownPack)
        ));
        assert!(display_name("no-such-pack").is_none());
    }

    #[test]
    fn every_pack_declares_a_real_default_tint() {
        for pack in styles::all() {
            assert!(
                parse_hex_color(pack.default_tint).is_some(),
                "{} has an unusable default tint",
                pack.id
            );
        }
    }

    #[test]
    fn previews_render_for_every_pack() {
        for pack in styles::all() {
            let uri = preview_uri(&pack, pack.default_tint)
                .unwrap_or_else(|e| panic!("{} preview failed: {e}", pack.id));
            assert!(uri.starts_with("data:image/png;base64,"), "{}", pack.id);
            assert!(uri.len() > 200, "{} preview is suspiciously empty", pack.id);
        }
    }
}
