use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    // Stamped so About can answer "which build is this, exactly" without needing
    // a commit hash — local builds have no hash, and that is precisely when the
    // question gets asked.
    println!("cargo:rustc-env=CURSED_BUILD_DATE={}", build_date());
    tauri_build::build();
}

/// `YYYY-MM-DD` in UTC, computed without pulling in a date crate.
///
/// Howard Hinnant's civil-from-days, which is exact for any date this will ever
/// see and short enough to read.
fn build_date() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    let days = secs.div_euclid(86_400);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };

    format!("{y:04}-{m:02}-{d:02}")
}
