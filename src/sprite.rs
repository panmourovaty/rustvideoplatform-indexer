use log::{info, warn};
use rand::Rng;
use std::path::Path;
use std::process::Command;

const THUMB_W: u32 = 352;
const THUMB_H: u32 = 198;
const SPRITE_COLS: u32 = 5;
const PREGEN_DIR: &str = "system_pregen";

/// Generate a random alphanumeric string of the given length.
fn random_string(len: usize) -> String {
    const CHARSET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::rng();
    (0..len)
        .map(|_| CHARSET[rng.random_range(0..CHARSET.len())] as char)
        .collect()
}

/// Build the xstack layout string for N thumbnails in SPRITE_COLS columns.
/// Each thumbnail is THUMB_W x THUMB_H pixels.
fn build_xstack_layout(count: u32) -> String {
    let mut parts = Vec::new();
    for i in 0..count {
        let col = i % SPRITE_COLS;
        let row = i / SPRITE_COLS;
        let x = col * THUMB_W;
        let y = row * THUMB_H;
        parts.push(format!("{x}_{y}"));
    }
    parts.join("|")
}

/// Attempt to generate a trending sprite from the given list of media IDs.
/// Returns the sprite filename (relative to source_dir/system_pregen/) on success.
///
/// Falls back to thumbnail.avif if thumbnail-sm.avif is not available for a media item.
/// Skips items where neither thumbnail exists.
pub fn generate_trending_sprite(source_dir: &str, media_ids: &[String]) -> Option<String> {
    if media_ids.is_empty() {
        return None;
    }

    // Collect paths of available thumbnails
    let thumb_paths: Vec<String> = media_ids
        .iter()
        .filter_map(|id| {
            let sm = format!("{}/{}/thumbnail-sm.avif", source_dir, id);
            let full = format!("{}/{}/thumbnail.avif", source_dir, id);
            if Path::new(&sm).exists() {
                Some(sm)
            } else if Path::new(&full).exists() {
                Some(full)
            } else {
                None
            }
        })
        .collect();

    if thumb_paths.is_empty() {
        warn!("No thumbnail files found for trending sprite generation");
        return None;
    }

    let count = thumb_paths.len() as u32;
    let sprite_name = format!("trending_sprite_{}.avif", random_string(12));
    let pregen_dir = format!("{}/{}", source_dir, PREGEN_DIR);

    if let Err(e) = std::fs::create_dir_all(&pregen_dir) {
        warn!("Failed to create system_pregen directory: {e}");
        return None;
    }

    let sprite_path = format!("{}/{}", pregen_dir, sprite_name);

    // Build ffmpeg command with xstack filter
    // Each input needs a scale filter to ensure consistent size (352x198 with letterbox)
    let mut args: Vec<String> = vec!["-nostdin".into(), "-y".into()];

    // Add all inputs
    for path in &thumb_paths {
        args.push("-i".into());
        args.push(path.clone());
    }

    // Build filter_complex: scale each input then xstack them
    let scale_parts: Vec<String> = (0..count)
        .map(|i| {
            format!(
                "[{i}:v]scale={THUMB_W}:{THUMB_H}:force_original_aspect_ratio=decrease,\
                 pad={THUMB_W}:{THUMB_H}:(ow-iw)/2:(oh-ih)/2:black,\
                 format=yuv420p[v{i}]"
            )
        })
        .collect();

    let input_refs: String = (0..count).map(|i| format!("[v{i}]")).collect();
    let layout = build_xstack_layout(count);
    let xstack = format!(
        "{input_refs}xstack=inputs={count}:layout={layout}:fill=black[out]"
    );

    let filter_complex = format!("{};{}", scale_parts.join(";"), xstack);

    args.push("-filter_complex".into());
    args.push(filter_complex);
    args.push("-map".into());
    args.push("[out]".into());
    args.push("-c:v".into());
    args.push("libsvtav1".into());
    args.push("-svtav1-params".into());
    args.push("avif=1".into());
    args.push("-crf".into());
    args.push("28".into());
    args.push("-frames:v".into());
    args.push("1".into());
    args.push(sprite_path.clone());

    info!(
        "Generating trending sprite with {} thumbnails: {}",
        count, sprite_name
    );

    let status = Command::new("ffmpeg").args(&args).status();
    match status {
        Ok(s) if s.success() => {
            info!("Trending sprite generated: {}", sprite_name);
            Some(sprite_name)
        }
        Ok(s) => {
            warn!(
                "ffmpeg sprite generation failed with exit code: {:?}",
                s.code()
            );
            None
        }
        Err(e) => {
            warn!("Failed to execute ffmpeg for sprite generation: {e}");
            None
        }
    }
}

/// Delete a sprite file if it exists.
pub fn delete_sprite(source_dir: &str, sprite_name: &str) {
    let path = format!("{}/{}/{}", source_dir, PREGEN_DIR, sprite_name);
    if let Err(e) = std::fs::remove_file(&path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            warn!("Failed to delete old sprite {path}: {e}");
        }
    }
}
