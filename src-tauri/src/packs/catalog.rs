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

impl RenderSpec {
    fn rgb(&self) -> AppResult<[u8; 3]> {
        parse_hex_color(&self.tint)
            .ok_or_else(|| AppError::invalid(format!("{} is not a colour", self.tint)))
    }

    /// Cache directory name. Every input that changes a pixel is in the key, so
    /// a stale cache entry cannot be served for a different choice.
    fn key(&self) -> String {
        format!(
            "{}-{}-{}",
            self.tint.trim_start_matches('#').to_ascii_lowercase(),
            self.size.clamp(32, 256),
            if self.outline { "o" } else { "n" }
        )
    }
}

pub fn list_summaries() -> AppResult<Vec<PackSummary>> {
    let mut out: Vec<PackSummary> = styles::all()
        .into_iter()
        .map(|pack| {
            let preview = preview_uri(&pack, pack.default_tint)?;
            Ok(PackSummary {
                id: pack.id.to_owned(),
                name: pack.name.to_owned(),
                category: pack.category.as_str(),
                author: "feylo",
                recolorable: true,
                animated: pack.animated,
                preview,
            })
        })
        .collect::<AppResult<_>>()?;

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
        "ANIMATED" => "ANIMATED",
        "ANIME" => "ANIME",
        "GAMING" => "GAMING",
        "VEHICLES" => "VEHICLES",
        "NEON" => "NEON",
        "RETRO" => "RETRO",
        _ => "IMPORTED",
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
        // Windows is currently drawing (PRD §5.4).
        let size = pipeline::nearest_size(spec.size);
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
            let rendered = role_bitmap(pack, role, size, 0.0)?;
            let coloured = rendered.tinted(finish.tint.unwrap_or([255, 255, 255]));
            let finished = if finish.outline {
                coloured.with_contrast_outline()
            } else {
                coloured
            };
            let max = (size - 1) as f32;
            images.push(CursorImage::new(
                finished,
                (
                    (hx * max).round() as u16,
                    (hy * max).round() as u16,
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
    for (role, path) in files {
        set.insert(role, path);
    }
    Ok(set)
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

    #[test]
    fn cache_keys_separate_every_visual_choice() {
        let base = spec();
        let bigger = RenderSpec { size: 48, ..base.clone() };
        let plain = RenderSpec { outline: false, ..base.clone() };
        let red = RenderSpec { tint: "#FF0000".into(), ..base.clone() };

        assert_ne!(base.key(), bigger.key());
        assert_ne!(base.key(), plain.key());
        assert_ne!(base.key(), red.key());
        assert_eq!(base.key(), "2e8bff-32-o");
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
