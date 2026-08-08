use std::time::{SystemTime, UNIX_EPOCH};

/// UTC timestamp in ISO-8601, without pulling in a date library for one string.
pub fn iso_now() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    iso_from_unix(secs)
}

/// Civil date from a Unix timestamp (Howard Hinnant's days-from-civil, inverted).
pub fn iso_from_unix(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let time_of_day = secs.rem_euclid(86_400);

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

    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        time_of_day / 3_600,
        (time_of_day % 3_600) / 60,
        time_of_day % 60
    )
}

/// Strips a UTF-8 byte-order mark.
///
/// Notepad writes one by default, and `serde_json` treats it as a syntax error.
/// Without this, hand-editing `settings.json` silently resets every setting to
/// its default — a failure that looks like the app losing your preferences for
/// no reason.
pub fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{feff}').unwrap_or(text)
}

/// Parses `#RRGGBB` (or `#RGB`) into linear 8-bit channels. Anything else is
/// rejected rather than silently defaulted — a bad tint should surface as an
/// error, not as a black cursor.
pub fn parse_hex_color(text: &str) -> Option<[u8; 3]> {
    let hex = text.strip_prefix('#').unwrap_or(text);
    let bytes = hex.as_bytes();
    let parse = |c: u8| -> Option<u8> {
        match c {
            b'0'..=b'9' => Some(c - b'0'),
            b'a'..=b'f' => Some(c - b'a' + 10),
            b'A'..=b'F' => Some(c - b'A' + 10),
            _ => None,
        }
    };
    match bytes.len() {
        3 => {
            let mut out = [0u8; 3];
            for (i, slot) in out.iter_mut().enumerate() {
                let v = parse(bytes[i])?;
                *slot = v * 17; // #abc -> #aabbcc
            }
            Some(out)
        }
        6 => {
            let mut out = [0u8; 3];
            for (i, slot) in out.iter_mut().enumerate() {
                *slot = parse(bytes[i * 2])? * 16 + parse(bytes[i * 2 + 1])?;
            }
            Some(out)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iso_matches_known_instants() {
        assert_eq!(iso_from_unix(0), "1970-01-01T00:00:00Z");
        assert_eq!(iso_from_unix(1_000_000_000), "2001-09-09T01:46:40Z");
        // A leap day, because that is where naive date maths breaks.
        assert_eq!(iso_from_unix(1_709_164_800), "2024-02-29T00:00:00Z");
    }

    #[test]
    fn a_byte_order_mark_does_not_break_json() {
        let with_bom = "\u{feff}{\"a\":1}";
        assert!(serde_json::from_str::<serde_json::Value>(with_bom).is_err());
        assert!(serde_json::from_str::<serde_json::Value>(strip_bom(with_bom)).is_ok());
        assert_eq!(strip_bom("plain"), "plain");
    }

    #[test]
    fn hex_colours_parse_both_forms_and_reject_junk() {
        assert_eq!(parse_hex_color("#2E8BFF"), Some([0x2e, 0x8b, 0xff]));
        assert_eq!(parse_hex_color("2e8bff"), Some([0x2e, 0x8b, 0xff]));
        assert_eq!(parse_hex_color("#abc"), Some([0xaa, 0xbb, 0xcc]));
        assert_eq!(parse_hex_color("#12345"), None);
        assert_eq!(parse_hex_color("rgb(1,2,3)"), None);
        assert_eq!(parse_hex_color(""), None);
    }
}
