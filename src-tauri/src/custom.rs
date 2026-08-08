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
fn staging() -> &'static Mutex<HashMap<String, Source>> {
    static STAGING: OnceLock<Mutex<HashMap<String, Source>>> = OnceLock::new();
    STAGING.get_or_init(|| Mutex::new(HashMap::new()))
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
    let source = pipeline::decode(bytes)?;

    // Normalise every frame the same way, or an animation's frames drift
    // relative to each other and the hotspot means something different per frame.
    let normalised = match source {
        Source::Static(bitmap) => Source::Static(pipeline::prepare_master(&bitmap)?),
        Source::Animated(frames) => {
            let mut out = Vec::with_capacity(frames.len());
            for (bitmap, delay) in frames {
                out.push((bitmap.squared().padded(1), delay));
            }
            Source::Animated(out)
        }
    };

    let first = normalised.first()?.clone();
    let suggested = hotspot::compute(&first, hotspot::suggest(&first));
    let token = uuid::Uuid::new_v4().to_string();

    let image = ImportedImage {
        token: token.clone(),
        width: first.width,
        height: first.height,
        animated: normalised.is_animated(),
        frame_count: normalised.frame_count(),
        data_uri: first.to_png_data_uri()?,
        suggested_hotspot: suggested,
    };

    if let Ok(mut staged) = staging().lock() {
        if staged.len() >= MAX_STAGED {
            // Oldest-by-arbitrary-order is fine: these are seconds-old scratch
            // decodes, and the alternative is unbounded memory.
            if let Some(key) = staged.keys().next().cloned() {
                staged.remove(&key);
            }
        }
        staged.insert(token, normalised);
    }
    Ok(image)
}

fn take_staged(token: &str) -> AppResult<Source> {
    staging()
        .lock()
        .ok()
        .and_then(|staged| staged.get(token).cloned())
        .ok_or_else(|| {
            AppError::invalid("that image is no longer staged — drop it in again")
        })
}

/// Renders the preview ladder for a staged image without writing anything.
pub fn preview(token: &str, outline: bool) -> AppResult<Vec<Preview>> {
    let source = take_staged(token)?;
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
) -> AppResult<BuiltCursor> {
    let source = take_staged(token)?;
    let id = format!("{}-{}", paths::slugify(name), &uuid::Uuid::new_v4().to_string()[..8]);
    let dir = cursor_dir(&id)?;
    std::fs::create_dir_all(&dir)?;

    let finish = Finish {
        tint: None,
        opacity: 1.0,
        outline,
    };

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
    let dir = cursor_dir(id)?;
    let still = dir.join("cursor.cur");
    if still.exists() {
        return Ok(still);
    }
    let animated = dir.join(format!("{}.ani", pipeline::nearest_size(size)));
    if animated.exists() {
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
            catalog::build_set(pack, spec)?
        }
        _ => CursorSet::default(),
    };

    let roles: &[Role] = match mode {
        ApplyMode::ArrowOnly | ApplyMode::Blend => &[Role::Arrow],
        ApplyMode::Recommended => &RECOMMENDED_ROLES,
        ApplyMode::All => &ALL_ROLES,
    };
    for role in roles {
        set.insert(*role, file.clone());
    }
    Ok(set)
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
