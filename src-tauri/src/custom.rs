//! The PNG-to-cursor feature (PRD §6): drop an image, get a real cursor.
//!
//! Two stages, deliberately separated. **Staging** decodes and normalises the
//! image so the hotspot picker has something to show; **building** writes the
//! actual files. Nothing touches the registry or the live pointer until the user
//! has seen exactly what they are about to get.

use crate::build::ani_writer::AniMetadata;
use crate::build::cur_writer::TARGET_SIZES;
use crate::build::hotspot;
use crate::build::pipeline::{self, Finish, Source};
use crate::cursor::roles::{Role, ALL_ROLES, RECOMMENDED_ROLES};
use crate::cursor::scheme::CursorSet;
use crate::error::{AppError, AppResult};
use crate::packs::catalog::{self, RenderSpec};
use crate::paths;
use crate::state::settings::ApplyMode;
use serde::Serialize;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

/// Staged images live in memory, never on disk, and never outlive the session.
/// The frontend holds only an opaque token — it never learns a path, and it
/// cannot ask for one (PRD §13.4).
fn staging() -> &'static Mutex<HashMap<String, Staged>> {
    static STAGING: OnceLock<Mutex<HashMap<String, Staged>>> = OnceLock::new();
    STAGING.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A staged image, and what it looked like before the matte touched it.
///
/// Both, because the editor needs something to reset to and something to re-key
/// from at a different tolerance. Keeping only the processed copy means "reset
/// to original" can only be honoured by asking the user to drop the file again,
/// which is not an undo.
#[derive(Debug, Clone)]
struct Staged {
    /// The **decoded** first frame, exactly as it arrived — not normalised, not
    /// keyed, not prepared.
    ///
    /// One frame and no preparation, both deliberate. The first version of this
    /// stored a fully prepared, unkeyed copy of the whole source, which meant
    /// every import ran the resize-trim-square-pad pipeline *twice* and held
    /// two copies of everything. On a 240-frame animation that is 240 redundant
    /// resamples and double the memory, for a copy that is only ever read if
    /// somebody opens the editor.
    ///
    /// The preparation now happens in `staged_original`, when the editor is
    /// actually opened, on the one frame it edits.
    original_frame: crate::build::bitmap::Bitmap,
    /// What the user is currently working with.
    current: Source,
}

/// At most a handful of staged images at once; a long session should not
/// accumulate decoded frames indefinitely.
const MAX_STAGED: usize = 6;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportedImage {
    pub token: String,
    pub width: u32,
    pub height: u32,
    pub animated: bool,
    pub frame_count: usize,
    pub data_uri: String,
    pub suggested_hotspot: (f32, f32),
    /// Fraction of the image the background removal took, 0.0–1.0.
    pub background_removed: f32,
    /// The image arrived with its background already gone, so nothing was
    /// attempted and nothing needed to be. Distinct from a refusal: there is
    /// no problem here to explain.
    pub already_transparent: bool,
    /// Present when removal was **declined**, with the sentence to show. What
    /// is in `data_uri` is then exactly what was imported.
    pub refusal: Option<String>,
    /// Whether an automatic attempt is worth offering at all. `false` means the
    /// UI should lead with "use it as it is" rather than with a retry that will
    /// produce the same refusal for the same reason.
    pub keyable: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BuiltCursor {
    pub id: String,
    pub name: String,
    pub animated: bool,
    pub frames: usize,
    pub hotspot: (f32, f32),
    pub previews: Vec<Preview>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Preview {
    pub size: u32,
    pub data_uri: String,
}

/// Decodes, trims, squares and stages an image for the hotspot picker.
pub fn stage(bytes: Vec<u8>) -> AppResult<ImportedImage> {
    stage_with(bytes, pipeline::Cut::Auto)
}

pub fn stage_with(bytes: Vec<u8>, cut: pipeline::Cut) -> AppResult<ImportedImage> {
    // Read before the bytes are consumed, and only ever `Some` for a `.cur` or
    // `.ani`. A cursor file states where its click lands; every other format
    // leaves it to be guessed at from the artwork.
    let stated_hotspot = crate::build::icon_reader::hotspot_fraction(&bytes);
    let source = pipeline::decode(bytes)?;

    // The image as it arrived, kept for the editor to reset to and re-key from.
    // One frame, unprepared: see `Staged::original_frame`.
    let original_frame = source.first()?.clone();

    // Normalise every frame the same way, or an animation's frames drift
    // relative to each other and the hotspot means something different per frame.
    let (normalised, report) = match source {
        Source::Static(bitmap) => {
            let (master, report) = pipeline::prepare_master_reported(&bitmap, cut)?;
            (Source::Static(master), report)
        }
        // Animations go through the same background removal as stills. They
        // used to skip it entirely, so a GIF with a white card behind it became
        // a cursor with a white card behind it.
        //
        // The report comes from the first frame, which is the frame the whole
        // sequence's decision is made on — see `prepare_animation_with`. A
        // per-frame report would be a per-frame decision, which is the thing
        // that makes an animated matte shimmer.
        Source::Animated(frames) => {
            let report = frames
                .first()
                .map(|(bitmap, _)| {
                    let keyability = crate::build::matte::assess(bitmap);

                    // **An animation that already has transparency is not a
                    // photograph, and must never be called one.**
                    //
                    // The still path has always checked this first — `attempt`
                    // returns `already_had_alpha` before it ever reaches the
                    // refusal. This branch went straight to the four signals,
                    // and those signals measure the colour of pixels that are
                    // *invisible*: the RGB behind alpha 0 is arbitrary, so a
                    // cut-out cursor reads as a busy, high-contrast border and
                    // fails every one of them.
                    //
                    // What that produced is the worst kind of wrong answer.
                    // Dropping any ordinary `.ani` — a downloaded cursor pack,
                    // which is the single most likely thing to be dropped on
                    // this app — put "This looks like a photo. Automatic
                    // background removal works on flat backgrounds" on screen,
                    // about a file whose background had been removed before it
                    // was ever downloaded. Nothing was wrong, and the app said
                    // something was.
                    if crate::build::matte::already_cut_out(bitmap) {
                        return crate::build::matte::MatteReport {
                            removed: 0.0,
                            already_had_alpha: true,
                            refused: None,
                            keyability,
                        };
                    }

                    crate::build::matte::MatteReport {
                        removed: 0.0,
                        already_had_alpha: false,
                        refused: (!keyability.confident && cut == pipeline::Cut::Auto)
                            .then_some(crate::build::matte::Refusal::LooksLikeAPhotograph),
                        keyability,
                    }
                })
                .unwrap_or_else(crate::build::matte::MatteReport::not_attempted);
            (
                Source::Animated(pipeline::prepare_animation_with(&frames, cut)?),
                report,
            )
        }
    };

    let first = normalised.first()?.clone();
    // The file's own hotspot wins over anything inferred from the pixels, but
    // only once it has been carried through the trim-square-pad that produced
    // the master — the number in the file is in the file's coordinates, and
    // `map_point_to_master` returns `None` rather than guessing when the
    // geometry it models is not the geometry that ran.
    let suggested = stated_hotspot
        .and_then(|point| pipeline::map_point_to_master(&original_frame, &first, point))
        .unwrap_or_else(|| hotspot::compute(&first, hotspot::suggest(&first)));
    let token = uuid::Uuid::new_v4().to_string();

    let image = ImportedImage {
        token: token.clone(),
        width: first.width,
        height: first.height,
        animated: normalised.is_animated(),
        frame_count: normalised.frame_count(),
        data_uri: first.to_png_data_uri()?,
        suggested_hotspot: suggested,
        background_removed: report.removed,
        already_transparent: report.already_had_alpha,
        // `BarelyMoved` is not worth a banner: "there was no background to
        // find" on art that never had one is noise, and the user can see the
        // preview. The two that change what they should do next are shown.
        refusal: match report.refused {
            Some(crate::build::matte::Refusal::BarelyMoved) => None,
            Some(reason) => Some(reason.message().to_owned()),
            None => None,
        },
        // An image that arrived transparent is not "unkeyable" — there is
        // simply nothing left to key, and telling somebody their cut-out cursor
        // "is not a flat background" is an answer to a question they did not
        // ask.
        keyable: report.keyability.confident || report.already_had_alpha,
    };

    if let Ok(mut staged) = staging().lock() {
        if staged.len() >= MAX_STAGED {
            // Oldest-by-arbitrary-order is fine: these are seconds-old scratch
            // decodes, and the alternative is unbounded memory.
            if let Some(key) = staged.keys().next().cloned() {
                staged.remove(&key);
            }
        }
        staged.insert(
            token,
            Staged {
                original_frame,
                current: normalised,
            },
        );
    }
    Ok(image)
}

fn take_staged(token: &str) -> AppResult<Source> {
    staging()
        .lock()
        .ok()
        .and_then(|staged| staged.get(token).map(|s| s.current.clone()))
        .ok_or_else(|| {
            AppError::invalid("that image is no longer staged — drop it in again")
        })
}

/// The staged image as it arrived, before any background removal.
///
/// The editor's starting point. Returned as a single bitmap because the editor
/// works on one frame: an animation's matte is decided from its first frame and
/// applied to the sequence, so that is the frame worth editing.
pub fn staged_original(token: &str) -> AppResult<crate::build::bitmap::Bitmap> {
    let frame = staging()
        .lock()
        .ok()
        .and_then(|staged| staged.get(token).map(|s| s.original_frame.clone()))
        .ok_or_else(|| AppError::invalid("that image is no longer staged — drop it in again"))?;

    // Prepared here rather than at import: normalised for size, trimmed,
    // squared and padded exactly as the keyed copy was, so the editor's canvas
    // lines up with what the app will build. Done once, when the editor opens.
    pipeline::prepare_master_with(&frame, pipeline::Cut::Keep)
}

/// Renders the preview ladder for a staged image without writing anything.
pub fn preview(
    token: &str,
    outline: bool,
    transform: &pipeline::Transform,
) -> AppResult<Vec<Preview>> {
    let source = transformed(take_staged(token)?, transform);
    let finish = Finish {
        tint: None,
        opacity: 1.0,
        outline,
    };
    Ok(pipeline::preview_ladder(source.first()?, &finish)?
        .into_iter()
        .map(|(size, data_uri)| Preview { size, data_uri })
        .collect())
}

/// Applies the user's edits to every frame.
///
/// Every frame, not just the first: a flipped animation whose later frames were
/// left alone would jump on the second frame, and that is the kind of fault that
/// only shows up once the cursor is already applied.
fn transformed(source: Source, transform: &pipeline::Transform) -> Source {
    if transform.is_identity() {
        return source;
    }
    match source {
        Source::Static(bitmap) => Source::Static(transform.apply(&bitmap)),
        Source::Animated(frames) => Source::Animated(
            frames
                .into_iter()
                .map(|(bitmap, delay)| (transform.apply(&bitmap), delay))
                .collect(),
        ),
    }
}

/// Where a custom cursor's files live. Resolving a location and creating one
/// are separate acts: looking up a cursor that does not exist must not leave a
/// directory behind for it.
fn cursor_dir(id: &str) -> AppResult<PathBuf> {
    Ok(paths::custom_dir()?.join(paths::validate_relative(id)?))
}

/// Writes the real files. Static images become one multi-resolution `.cur`;
/// animations become one `.ani` per target size, because the format cannot hold
/// more than one resolution (PRD §5.4).
pub fn build(
    token: &str,
    name: &str,
    hotspot: (f32, f32),
    outline: bool,
    speed: f32,
    transform: &pipeline::Transform,
    hand_token: Option<&str>,
) -> AppResult<BuiltCursor> {
    let source = transformed(take_staged(token)?, transform);
    let id = format!("{}-{}", paths::slugify(name), &uuid::Uuid::new_v4().to_string()[..8]);
    let dir = cursor_dir(&id)?;
    std::fs::create_dir_all(&dir)?;

    let finish = Finish {
        tint: None,
        opacity: 1.0,
        outline,
    };

    // Keep the normalised source as a real PNG next to the cursor, whatever it
    // arrived as. A JPEG has no alpha and a GIF has one bit of it; both become a
    // true RGBA PNG here. That is what makes the artwork re-editable later and
    // what lets a preset carry its own image in a `.cfpack`.
    let master_png = source.first()?.to_png(image::codecs::png::CompressionType::Best)?;
    std::fs::write(dir.join("source.png"), &master_png)?;

    let animated = source.is_animated();
    match &source {
        Source::Static(master) => {
            let bytes = pipeline::build_cur(master, hotspot, &finish, &TARGET_SIZES)?;
            write_verified(&dir.join("cursor.cur"), &bytes)?;
        }
        Source::Animated(frames) => {
            let metadata = AniMetadata {
                name: Some(name.to_owned()),
                author: None,
            };
            for size in TARGET_SIZES {
                let bytes =
                    pipeline::build_ani(frames, hotspot, &finish, size, speed, &metadata)?;
                write_verified(&dir.join(format!("{size}.ani")), &bytes)?;
            }
        }
    }

    let previews = pipeline::preview_ladder(source.first()?, &finish)?
        .into_iter()
        .map(|(size, data_uri)| Preview { size, data_uri })
        .collect();

    // The hover image, if one was given.
    //
    // Written as its own set of files rather than folded into the main cursor:
    // the two are different artwork with different hotspots, and a link cursor
    // that inherits the arrow's hotspot points at the wrong pixel. It uses the
    // hotspot suggested for its own image, which is what the picker would have
    // proposed had it been the main one.
    let has_hand = match hand_token {
        Some(token) => match take_staged(token) {
            Ok(source) => {
                let hand_source = transformed(source, transform);
                let first = hand_source.first()?.clone();
                let spot = hotspot::compute(&first, hotspot::suggest(&first));
                match &hand_source {
                    Source::Static(master) => {
                        let bytes = pipeline::build_cur(master, spot, &finish, &TARGET_SIZES)?;
                        write_verified(&dir.join("hand.cur"), &bytes)?;
                    }
                    Source::Animated(frames) => {
                        let metadata = AniMetadata {
                            name: Some(format!("{name} (hover)")),
                            author: None,
                        };
                        for size in TARGET_SIZES {
                            let bytes =
                                pipeline::build_ani(frames, spot, &finish, size, speed, &metadata)?;
                            write_verified(&dir.join(format!("hand-{size}.ani")), &bytes)?;
                        }
                    }
                }
                if let Ok(mut staged) = staging().lock() {
                    staged.remove(token);
                }
                true
            }
            // A hover image that will not stage must not lose the cursor the
            // user has already built.
            Err(e) => {
                log::warn!("the hover image was skipped: {e}");
                false
            }
        },
        None => false,
    };

    // A manifest beside the files, so a saved cursor can be listed later.
    //
    // Without this the directory is a slug and a uuid: the name the user typed
    // is gone the moment the screen closes, and a saved cursor is only
    // reachable if they remember making it. Writing it is what turns Custom
    // from a one-shot builder into a library.
    let manifest = CustomCursor {
        id: id.clone(),
        name: name.chars().take(48).collect(),
        animated,
        created: crate::util::iso_now(),
        has_hand,
    };
    std::fs::write(dir.join("cursor.json"), serde_json::to_string_pretty(&manifest)?)?;

    // The staged copy has done its job; hold nothing further.
    if let Ok(mut staged) = staging().lock() {
        staged.remove(token);
    }

    Ok(BuiltCursor {
        id,
        name: name.chars().take(48).collect(),
        animated,
        frames: source.frame_count(),
        hotspot,
        previews,
    })
}

/// Writes a cursor file and refuses to leave it behind if Windows will not load
/// it. A broken cursor on disk is a broken cursor one apply away from the
/// registry (PRD §6.1 step 6).
fn write_verified(path: &std::path::Path, bytes: &[u8]) -> AppResult<()> {
    let temp = path.with_extension("tmp");
    std::fs::write(&temp, bytes)?;
    std::fs::rename(&temp, path)?;
    match crate::cursor::engine::verify_loadable(path) {
        Ok(()) => Ok(()),
        Err(e) => {
            let _ = std::fs::remove_file(path);
            Err(e)
        }
    }
}

/// The file to use for a custom cursor at a given size.
fn file_for(id: &str, size: u32) -> AppResult<PathBuf> {
    file_for_role(id, size, false)
}

/// The animation to use for a requested size, preferring the rung *above* it.
///
/// A custom cursor's `.ani` files are written once, one per ladder rung, while
/// the size control moves a pixel at a time — so the requested size usually
/// falls between two rungs and Windows has to rescale whichever it is handed.
///
/// Which side it comes from decides how it looks. Given a smaller rung the shell
/// enlarges: bilinear, unpremultiplied, no gamma correction, and every edge goes
/// soft and blocky — which is most of what a large animated cursor looked like.
/// Given a larger one it shrinks instead, and a minified bitmap keeps its edges.
/// So the smallest rung at or above the request wins, and only when there is
/// none does it fall back downwards, largest first, to keep the enlargement as
/// small as possible.
fn best_animation(dir: &std::path::Path, prefix: &str, size: u32) -> Option<PathBuf> {
    let mut rungs = TARGET_SIZES;
    rungs.sort_unstable();
    let at_or_above = rungs.iter().copied().filter(|&rung| rung >= size);
    let below = rungs.iter().copied().filter(|&rung| rung < size).rev();
    at_or_above
        .chain(below)
        .map(|rung| dir.join(format!("{prefix}{rung}.ani")))
        .find(|candidate| candidate.exists())
}

/// The same, but able to return the hover artwork when there is any.
fn file_for_role(id: &str, size: u32, hand: bool) -> AppResult<PathBuf> {
    let dir = cursor_dir(id)?;
    if hand {
        let still = dir.join("hand.cur");
        if still.exists() {
            return Ok(still);
        }
        if let Some(animated) = best_animation(&dir, "hand-", size) {
            return Ok(animated);
        }
        // No hover image: fall through to the main cursor, which is what a
        // cursor without one has always used.
    }
    let still = dir.join("cursor.cur");
    if still.exists() {
        return Ok(still);
    }
    if let Some(animated) = best_animation(&dir, "", size) {
        return Ok(animated);
    }
    Err(AppError::invalid(
        "that custom cursor's files are missing — rebuild it from the image",
    ))
}

/// Assembles the scheme for a custom cursor and the chosen application mode.
///
/// `Blend` is the mode that makes a single user image usable: the custom arrow
/// rides on top of a full catalog pack, so the other sixteen roles stay coherent
/// instead of reverting to stock Windows (PRD §6.3).
pub fn build_set(
    cursor_id: &str,
    mode: ApplyMode,
    blend_pack: Option<&str>,
    spec: &RenderSpec,
) -> AppResult<CursorSet> {
    let file = file_for(cursor_id, spec.size)?;

    let mut set = match mode {
        ApplyMode::Blend => {
            let pack = blend_pack.ok_or_else(|| {
                AppError::invalid("blending needs a catalog pack for the other roles")
            })?;
            // The descriptor stored this id when the cursor was applied, and it
            // may name one of the 290 generated packs that no longer exist — in
            // which case failing here would leave the size and colour controls
            // permanently doing nothing. See `catalog::resolve_blend_base`.
            catalog::build_set(&catalog::resolve_blend_base(pack), spec)?
        }
        _ => CursorSet::default(),
    };

    // In Blend, the custom image covers the roles that are simply *the pointer*
    // as well as the arrow.
    //
    // Filling working, busy, help, unavailable and alternate from the base pack
    // meant that the instant the machine did anything -- a copy, a download, an
    // app launching -- the user's own cursor was replaced by a stranger's
    // artwork, which reads as the pointer having reverted. The directional and
    // text roles stay with the base pack on purpose: a resize handle shaped like
    // an arrow says nothing about which way to drag.
    const POINTER_LIKE: [Role; 7] = [
        Role::Arrow,
        Role::AppStarting,
        Role::Wait,
        Role::Help,
        Role::No,
        Role::UpArrow,
        Role::Person,
    ];

    // Blend covers the hand too once there is artwork for it. Without one it
    // stays with the base pack, because an arrow-shaped link cursor tells you
    // nothing about what you are hovering over.
    const POINTER_LIKE_WITH_HAND: [Role; 8] = [
        Role::Arrow,
        Role::AppStarting,
        Role::Wait,
        Role::Help,
        Role::No,
        Role::UpArrow,
        Role::Person,
        Role::Hand,
    ];
    let has_hand = cursor_dir(cursor_id)
        .map(|d| d.join("hand.cur").exists() || d.join("hand-32.ani").exists())
        .unwrap_or(false);

    let roles: &[Role] = match mode {
        ApplyMode::ArrowOnly => &[Role::Arrow],
        ApplyMode::Blend if has_hand => &POINTER_LIKE_WITH_HAND,
        ApplyMode::Blend => &POINTER_LIKE,
        ApplyMode::Recommended => &RECOMMENDED_ROLES,
        ApplyMode::All => &ALL_ROLES,
    };
    for role in roles {
        // The hand and the I-beam do not grow with the pointer, so for an
        // animated custom cursor they take the file built at their own size
        // rather than the pointer's.
        let sized = file_for_role(cursor_id, role.size_from(spec.size), *role == Role::Hand)
            .unwrap_or_else(|_| file.clone());
        set.insert(*role, sized);
    }
    Ok(set)
}

/// True when this custom cursor still has usable artwork on disk.
///
/// The directory existing is not the test — a half-written build leaves one
/// behind with nothing in it. What matters is whether a role can actually be
/// pointed at a file, which means `cursor.cur` or a sized `.ani`.
pub fn exists(id: &str) -> bool {
    let Ok(dir) = cursor_dir(id) else {
        return false;
    };
    if dir.join("cursor.cur").is_file() {
        return true;
    }
    std::fs::read_dir(&dir).is_ok_and(|entries| {
        entries.filter_map(Result::ok).any(|entry| {
            entry
                .path()
                .extension()
                .and_then(|x| x.to_str())
                .is_some_and(|x| x.eq_ignore_ascii_case("ani"))
        })
    })
}

/// Removes a built custom cursor's files.
pub fn remove(id: &str) -> AppResult<()> {
    let dir = paths::custom_dir()?.join(paths::validate_relative(id)?);
    let resolved = paths::ensure_inside_storage(&dir)?;
    if resolved.exists() {
        std::fs::remove_dir_all(&resolved)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Builds an animated cursor the way a download site does: a subject with a
    /// transparent surround, several frames, no background anywhere in it.
    fn a_cut_out_animation() -> Vec<u8> {
        use crate::build::ani_writer::{self, AniFrame, AniMetadata};
        use crate::build::bitmap::Bitmap;
        use crate::build::cur_writer::CursorImage;

        let frames: Vec<AniFrame> = (0..4)
            .map(|index| {
                let mut art = Bitmap::new(64, 64);
                // A blob in the middle, everything around it transparent.
                for y in 18..46u32 {
                    for x in 18..46u32 {
                        let shade = 40 + index as u8 * 30;
                        art.set_pixel(x, y, [shade, 200 - shade, 120, 255]);
                    }
                }
                AniFrame {
                    images: vec![CursorImage::new(art, (20, 20))],
                    delay_ms: 100,
                }
            })
            .collect();
        ani_writer::write_ani(&frames, 1.0, &AniMetadata::default()).expect("an ani")
    }

    /// **The regression for the message that made this feature look broken.**
    ///
    /// An animated cursor pack is the single most likely thing to be dropped on
    /// this app, and every one of them arrives already cut out. Staging one used
    /// to answer with "This looks like a photo. Automatic background removal
    /// works on flat backgrounds" — because the animated branch measured the
    /// four keyability signals without first asking whether there was anything
    /// left to key, and those signals read the colour of pixels behind alpha 0,
    /// which is arbitrary.
    ///
    /// Nothing was wrong with the file, nothing was wrong with the import, and
    /// the app said something was.
    #[test]
    fn an_animation_that_is_already_cut_out_is_not_called_a_photograph() {
        let staged = stage(a_cut_out_animation()).expect("an animated cursor stages");

        assert!(staged.animated, "a four-frame .ani is an animation");
        assert_eq!(staged.frame_count, 4);
        assert!(
            staged.refusal.is_none(),
            "nothing needed removing, so there is nothing to explain: {:?}",
            staged.refusal
        );
        assert!(staged.already_transparent, "it arrived with its background gone");
        assert!(
            staged.keyable,
            "an image that is already cut out must not be reported as unkeyable"
        );
    }

    /// The same, for a still `.cur` — the still path has always had this right,
    /// and this is what keeps it that way.
    #[test]
    fn a_cursor_file_is_not_called_a_photograph_either() {
        use crate::build::bitmap::Bitmap;
        use crate::build::cur_writer::{self, CursorImage};

        let mut art = Bitmap::new(64, 64);
        for y in 10..40u32 {
            for x in 10..30u32 {
                art.set_pixel(x, y, [220, 40, 40, 255]);
            }
        }
        let bytes = cur_writer::write_cur(&[CursorImage::new(art, (10, 10))]).expect("a cur");

        let staged = stage(bytes).expect("a cursor file stages");
        assert!(staged.refusal.is_none(), "{:?}", staged.refusal);
        assert!(staged.already_transparent);
    }

    /// The hover image only matters if the hand role actually reaches for it.
    ///
    /// `file_for_role` is the whole mechanism: ask for the hand and get the hand
    /// artwork, ask for anything else and get the pointer. If it silently
    /// returned the pointer for both, adding a hover image would appear to work
    /// -- files written, no error -- and change nothing on screen.
    #[test]
    fn the_hand_reaches_for_its_own_artwork_when_there_is_some() {
        let id = "test-hover-routing";
        let Ok(dir) = cursor_dir(id) else { return };
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");

        std::fs::write(dir.join("cursor.cur"), b"pointer").expect("write");

        // With no hover artwork, the hand falls back to the pointer -- which is
        // what a cursor without one has always done.
        let before = file_for_role(id, 32, true).expect("falls back");
        assert!(before.ends_with("cursor.cur"));

        std::fs::write(dir.join("hand.cur"), b"hover").expect("write");

        assert!(
            file_for_role(id, 32, true).expect("hand").ends_with("hand.cur"),
            "the hand must take the hover artwork"
        );
        assert!(
            file_for_role(id, 32, false).expect("arrow").ends_with("cursor.cur"),
            "everything else keeps the pointer"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    /// An animated custom cursor is drawn at whatever `CursorBaseSize` says,
    /// while its files exist only at the ladder rungs — so one of them has to be
    /// rescaled by Windows, and which side it comes from decides how it looks.
    ///
    /// Enlarging is the shell's bilinear stretch with no premultiplication and
    /// no gamma correction; shrinking keeps its edges. Picking the *nearest*
    /// rung sent 78 px to the 64 px file and enlarged it. The rung above is the
    /// one to take.
    #[test]
    fn an_animation_is_shrunk_to_size_rather_than_stretched_up_to_it() {
        let id = "test-rung-choice";
        let Ok(dir) = cursor_dir(id) else { return };
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).expect("scratch dir");

        for rung in TARGET_SIZES {
            std::fs::write(dir.join(format!("{rung}.ani")), b"frames").expect("write");
        }

        // 78 is nearer to 64, and 64 is the wrong answer.
        let chosen = best_animation(&dir, "", 78).expect("a rung");
        assert!(
            chosen.ends_with("96.ani"),
            "78px should shrink the 96px file, not stretch the 64px one: got {chosen:?}"
        );
        // An exact rung is taken exactly.
        assert!(best_animation(&dir, "", 48).expect("a rung").ends_with("48.ani"));

        // Above every rung there is nothing to shrink, so the largest wins and
        // the enlargement is as small as it can be.
        assert!(best_animation(&dir, "", 200).expect("a rung").ends_with("128.ani"));

        let _ = std::fs::remove_dir_all(&dir);
    }

    use std::io::Cursor as IoCursor;

    fn png(width: u32, height: u32) -> Vec<u8> {
        let mut buffer = image::RgbaImage::new(width, height);
        // An off-centre blob, so trimming and hotspot detection have work to do.
        for y in 2..height.min(10) {
            for x in 1..width.min(6) {
                buffer.put_pixel(x, y, image::Rgba([255, 255, 255, 255]));
            }
        }
        let mut out = Vec::new();
        image::DynamicImage::ImageRgba8(buffer)
            .write_to(&mut IoCursor::new(&mut out), image::ImageFormat::Png)
            .unwrap();
        out
    }

    fn encoded(format: image::ImageFormat) -> Vec<u8> {
        let mut buffer = image::RgbaImage::new(24, 24);
        for y in 4..20u32 {
            for x in 2..14u32 {
                buffer.put_pixel(x, y, image::Rgba([200, 40, 90, 255]));
            }
        }
        let mut out = Vec::new();
        let image = image::DynamicImage::ImageRgba8(buffer);
        // JPEG has no alpha channel, so it must be flattened before encoding.
        let image = if format == image::ImageFormat::Jpeg {
            image::DynamicImage::ImageRgb8(image.to_rgb8())
        } else {
            image
        };
        image
            .write_to(&mut IoCursor::new(&mut out), format)
            .unwrap();
        out
    }

    /// The formats the import screen advertises must all survive the whole
    /// staging path, not just PNG.
    #[test]
    fn every_advertised_input_format_stages_successfully() {
        for format in [
            image::ImageFormat::Png,
            image::ImageFormat::Jpeg,
            image::ImageFormat::Bmp,
            image::ImageFormat::Gif,
            image::ImageFormat::Tiff,
        ] {
            let staged = stage(encoded(format))
                .unwrap_or_else(|e| panic!("{format:?} failed to stage: {e}"));
            assert!(
                staged.data_uri.starts_with("data:image/png;base64,"),
                "{format:?} should be normalised to PNG"
            );
            assert!(staged.width > 0 && staged.height > 0, "{format:?} lost its pixels");
        }

        // And the two the screen advertises that are not image formats at all.
        // A cursor app that cannot open a cursor is the joke it took three
        // releases to notice: dropping a `.ani` was answered with "only PNG,
        // JPEG, GIF, WebP and BMP images can be imported".
        for (what, bytes) in [
            ("a .ani", a_cut_out_animation()),
            ("a .cur", {
                use crate::build::bitmap::Bitmap;
                use crate::build::cur_writer::{self, CursorImage};
                let mut art = Bitmap::new(32, 32);
                for y in 8..24u32 {
                    for x in 8..24u32 {
                        art.set_pixel(x, y, [10, 180, 220, 255]);
                    }
                }
                cur_writer::write_cur(&[CursorImage::new(art, (8, 8))]).expect("a cur")
            }),
        ] {
            let staged = stage(bytes).unwrap_or_else(|e| panic!("{what} failed to stage: {e}"));
            assert!(staged.width > 0 && staged.height > 0, "{what} lost its pixels");
            assert!(staged.data_uri.starts_with("data:image/png;base64,"));
        }
    }

    #[test]
    fn staging_returns_a_token_and_a_preview() {
        let staged = stage(png(32, 32)).unwrap();
        assert!(!staged.token.is_empty());
        assert!(staged.data_uri.starts_with("data:image/png;base64,"));
        assert!(!staged.animated);
        assert_eq!(staged.frame_count, 1);
        let (hx, hy) = staged.suggested_hotspot;
        assert!((0.0..=1.0).contains(&hx) && (0.0..=1.0).contains(&hy));
    }

    #[test]
    fn an_expired_token_is_a_clear_error_not_a_panic() {
        let error = take_staged("not-a-token").unwrap_err();
        assert!(error.to_string().contains("no longer staged"));
    }

    #[test]
    fn a_blend_without_a_pack_is_refused() {
        let spec = RenderSpec {
            tint: "#2E8BFF".into(),
            size: 32,
            outline: true,
        };
        assert!(build_set("nope", ApplyMode::Blend, None, &spec).is_err());
    }

    #[test]
    fn staging_does_not_grow_without_bound() {
        for _ in 0..(MAX_STAGED + 3) {
            let _ = stage(png(16, 16));
        }
        let count = staging().lock().map(|staged| staged.len()).unwrap_or(0);
        assert!(count <= MAX_STAGED, "staged {count} images");
    }
}

/// One cursor the user built and kept.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomCursor {
    pub id: String,
    pub name: String,
    pub animated: bool,
    pub created: String,
    /// Whether a second image was supplied for the link/hover role.
    #[serde(default)]
    pub has_hand: bool,
}

/// Every custom cursor still on disk, newest first.
///
/// Reads the manifests rather than the directory names: a directory is a slug
/// and a uuid, and the whole point is to show the name the user gave it.
pub fn list() -> AppResult<Vec<CustomCursor>> {
    let root = paths::custom_dir()?;
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(&root) else {
        return Ok(out);
    };
    for entry in entries.filter_map(Result::ok) {
        let manifest = entry.path().join("cursor.json");
        let Ok(text) = std::fs::read_to_string(&manifest) else {
            continue;
        };
        if let Ok(cursor) = serde_json::from_str::<CustomCursor>(crate::util::strip_bom(&text)) {
            out.push(cursor);
        }
    }
    // Newest first: the one just made is the one being looked for.
    out.sort_by(|a, b| b.created.cmp(&a.created));
    Ok(out)
}

/// A tile image for a saved custom cursor, from the PNG kept beside it.
pub fn thumbnail(id: &str) -> AppResult<String> {
    let dir = cursor_dir(id)?;
    let bytes = std::fs::read(dir.join("source.png"))
        .map_err(|_| AppError::invalid("that cursor's artwork is missing"))?;
    let decoded = image::load_from_memory(&bytes)
        .map_err(|_| AppError::invalid("that cursor's artwork could not be read"))?
        .to_rgba8();
    let bitmap = crate::build::bitmap::Bitmap::from_rgba(
        decoded.width(),
        decoded.height(),
        decoded.into_raw(),
    )?;
    bitmap.resized(64, 64)?.to_png_data_uri()
}
