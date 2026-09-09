/// The machine's short hostname — the one shown in interactive shell
/// prompts. Owned by the server process only: it is read straight from the
/// OS on demand, with no collector, no IPC, and no database involvement, so
/// it can never be affected by (or affect) data collection.

/// Short hostname (no domain), e.g. `pgx1` rather than `pgx1.tailnet.ts.net`.
pub fn hostname() -> String {
    // /proc/sys/kernel/hostname holds the kernel's short hostname, which is
    // exactly what login shells display in prompts.
    if let Ok(raw) = std::fs::read_to_string("/proc/sys/kernel/hostname") {
        let name = raw.trim();
        if !name.is_empty() {
            return name.to_string();
        }
    }
    // Non-Linux or read failure: fall back to common environment variables,
    // taking the short (first label) form either way.
    for var in ["HOSTNAME", "HOST"] {
        if let Ok(value) = std::env::var(var) {
            let short = value.split('.').next().unwrap_or("").trim();
            if !short.is_empty() {
                return short.to_string();
            }
        }
    }
    String::new()
}

/// JSON body for `GET /api/host` — always a string value so the frontend
/// never has to branch on the field's type.
pub fn hostname_response() -> String {
    serde_json::json!({ "hostname": hostname() }).to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hostname_returns_a_string() {
        // May be empty in a minimal container; must never panic.
        assert_eq!(hostname_response().contains(r#""hostname":""#), true);
    }

    #[test]
    fn hostname_shape_is_json_object() {
        let v: serde_json::Value = serde_json::from_str(&hostname_response()).unwrap();
        assert!(v.get("hostname").unwrap().is_string());
    }
}
