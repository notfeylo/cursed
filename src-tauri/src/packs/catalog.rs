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
            .ok_or_else(|| AppError::invalid(format!("{} is not a colour", self.tint)))
    }

    /// Cache directory name. Every input that changes a pixel is in the key, so
    /// a stale cache entry cannot be served for a different choice.
    fn key(&self) -> String {
        format!(
            "v{RENDER_VERSION}-{}-{}-{}{}",
            self.tint.trim_start_matches('#').to_ascii_lowercase(),
            self.size.clamp(crate::state::settings::MIN_CURSOR_PX, crate::state::settings::MAX_CURSOR_PX),
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
    let dir = paths::cache_dir()?.join(pack.id).join(spec.key());
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
        let size = pipeline::nearest_size(glyph_size(role, spec.size));
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

    let mut set = build_roles(base, &ALL_ROLES, spec)?;

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

/// Builds only the Arrow.
///
/// Catalog hover has to feel instant, and a full 17-role build is real work. The
/// pointer a user is looking at while browsing is the arrow, so preview renders
/// exactly that and nothing else — then the commit builds the rest.
pub fn build_preview_set(pack_id: &str, spec: &RenderSpec) -> AppResult<CursorSet> {
    if is_imported(pack_id) {
        // An imported pack's files already exist, so hovering costs a lookup
        // rather than a render.
        let pack = crate::import::get(pack_id)?;
        let files = crate::import::role_files(&pack)?;
        let mut set = CursorSet::default();
        if let Some(path) = files.get(&Role::Arrow).or_else(|| files.values().next()) {
            set.insert(Role::Arrow, path.clone());
        }
        return Ok(set);
    }
    build_roles(pack_id, &[Role::Arrow], spec)
}

pub fn display_name(pack_id: &str) -> Option<&'static str> {
    styles::find(pack_id).map(|pack| pack.name)
}

pub fn default_tint(pack_id: &str) -> Option<&'static str> {
    styles::find(pack_id).map(|pack| pack.default_tint)
}

/// Total bytes of rendered cursors on disk.
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

/// Empties the render cache. Safe at any time: anything removed is rebuilt on
/// the next apply, and files still referenced by the registry are re-created
/// before they are needed because a re-apply always rebuilds first.
pub fn clear_cache() -> AppResult<u64> {
    let dir = paths::cache_dir()?;
    let freed = cache_size()?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir)?;
    }
    std::fs::create_dir_all(&dir)?;
    Ok(freed)
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
    #[test]
    fn an_animated_pack_produces_files_that_load_as_animated() {
        let pack = styles::all()
            .into_iter()
            .find(|p| p.animated)
            .expect("the catalog ships animated packs");

        let set = build_set(pack.id, &spec()).expect("an animated pack builds");

        let mut checked = 0usize;
        for (role, path) in &set.files {
            if !crate::cursor::engine::is_animated(path) {
                continue;
            }
            assert!(
                crate::cursor::engine::verify_loadable(path).is_ok(),
                "{role} of {} did not load as an animated cursor",
                pack.id
            );
            checked += 1;
        }
        assert!(checked > 0, "{} produced no .ani files at all", pack.id);
    }

    #[test]
    fn the_built_in_catalog_is_never_empty() {
        // Deliberately goes through the same call the catalog screen makes,
        // rather than reading the flag: what matters is what a fresh machine
        // actually receives, and the flag is only one way to get that wrong.
        let summaries = list_summaries().expect("the catalog must load");
        let built_in = summaries.iter().filter(|s| s.author == "feylo").count();

        assert!(
            built_in >= 100,
            "only {built_in} built-in packs reached the catalog; a machine with \
             no imports would look bare or empty"
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

    #[test]
    fn cache_keys_separate_every_visual_choice() {
        let base = spec();
        let bigger = RenderSpec { size: 48, ..base.clone() };
        let plain = RenderSpec { outline: false, ..base.clone() };
        let red = RenderSpec { tint: "#FF0000".into(), ..base.clone() };

        assert_ne!(base.key(), bigger.key());
        assert_ne!(base.key(), plain.key());
        assert_ne!(base.key(), red.key());
        assert_eq!(base.key(), format!("v{RENDER_VERSION}-2e8bff-32-o"));

        // The renderer's version is in the key, not only the user's choices. A
        // change to how a pixel is produced leaves every existing entry stale
        // and still matching its key, so the fix reaches nobody who had already
        // applied that cursor — invisible on a developer's empty cache and
        // permanent on a user's full one.
        assert!(
            base.key().starts_with(&format!("v{RENDER_VERSION}-")),
            "the render version has to be part of the cache key"
        );
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
