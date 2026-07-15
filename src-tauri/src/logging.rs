const SENSITIVE_KEYS: &[&str] = &[
    "password",
    "passwd",
    "secret",
    "token",
    "authorization",
    "api_key",
    "apikey",
];

pub fn redact_text(input: &str) -> String {
    let mut output = input.to_owned();
    for key in SENSITIVE_KEYS {
        for separator in ['=', ':'] {
            let marker = format!("{key}{separator}");
            let lowercase = output.to_lowercase();
            let mut offset = 0;
            while let Some(relative) = lowercase[offset..].find(&marker) {
                let start = offset + relative + marker.len();
                let end = output[start..]
                    .find(|c: char| c.is_whitespace() || c == ',' || c == '&')
                    .map(|v| start + v)
                    .unwrap_or(output.len());
                output.replace_range(start..end, "[REDACTED]");
                offset = start + "[REDACTED]".len();
            }
        }
    }
    output
}

pub fn init() {
    let _ = tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .with_target(false)
        .compact()
        .try_init();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_common_secret_shapes() {
        let value = redact_text("password=hunter2 token:abc123 user=demo");
        assert!(!value.contains("hunter2"));
        assert!(!value.contains("abc123"));
        assert!(value.contains("user=demo"));
    }
}
