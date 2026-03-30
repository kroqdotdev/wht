use rand::Rng;
use std::path::Path;

/// Resolve the request body from CLI options, in priority order.
pub fn resolve_body(
    data: Option<&str>,
    file: Option<&str>,
    template: Option<&str>,
    size: Option<&str>,
) -> Result<Option<Vec<u8>>, String> {
    if let Some(d) = data {
        return Ok(Some(d.as_bytes().to_vec()));
    }

    if let Some(path) = file {
        let p = Path::new(path);
        let bytes = std::fs::read(p).map_err(|e| format!("Failed to read file {path}: {e}"))?;
        return Ok(Some(bytes));
    }

    if let Some(name) = template {
        let body = crate::templates::get_template(name)?;
        return Ok(Some(body.into_bytes()));
    }

    if let Some(s) = size {
        let bytes = generate_sized_payload(s)?;
        return Ok(Some(bytes));
    }

    Ok(None)
}

/// Parse a human-readable size string like "1kb", "5mb", "25mb" into bytes.
fn parse_size(s: &str) -> Result<usize, String> {
    let s = s.trim().to_lowercase();

    let (num_part, multiplier) = if s.ends_with("mb") {
        (&s[..s.len() - 2], 1024 * 1024)
    } else if s.ends_with("kb") {
        (&s[..s.len() - 2], 1024)
    } else if s.ends_with('b') {
        (&s[..s.len() - 1], 1)
    } else {
        // Assume bytes if no unit
        (s.as_str(), 1)
    };

    let num: f64 = num_part
        .trim()
        .parse()
        .map_err(|_| format!("Invalid size number: {num_part}"))?;

    let total = (num * multiplier as f64) as usize;
    let max = 25 * 1024 * 1024;
    if total > max {
        return Err(format!("Size {s} exceeds maximum of 25mb"));
    }
    if total == 0 {
        return Err("Size must be greater than 0".to_string());
    }

    Ok(total)
}

/// Generate a JSON payload of approximately the requested size.
fn generate_sized_payload(size_str: &str) -> Result<Vec<u8>, String> {
    let target = parse_size(size_str)?;

    // Envelope: {"size":"<size_str>","padding":"..."}
    // We'll build the JSON manually so we can hit the exact target size.
    let prefix = format!("{{\"size\":\"{size_str}\",\"padding\":\"");
    let suffix = "\"}";
    let overhead = prefix.len() + suffix.len();

    if target <= overhead {
        // Too small for the envelope — just return a short JSON
        let body = format!("{{\"size\":\"{size_str}\"}}");
        return Ok(body.into_bytes());
    }

    let padding_len = target - overhead;
    let mut rng = rand::rng();
    let padding: String = (0..padding_len)
        .map(|_| {
            let idx = rng.random_range(0..36);
            if idx < 10 {
                (b'0' + idx) as char
            } else {
                (b'a' + idx - 10) as char
            }
        })
        .collect();

    let mut out = Vec::with_capacity(target);
    out.extend_from_slice(prefix.as_bytes());
    out.extend_from_slice(padding.as_bytes());
    out.extend_from_slice(suffix.as_bytes());

    Ok(out)
}
