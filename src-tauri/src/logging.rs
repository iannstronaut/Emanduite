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
    redact_bearer_tokens(&mut output);
    for key in SENSITIVE_KEYS {
        for separator in ['=', ':'] {
            let marker = format!("{key}{separator}");
            let mut offset = 0;
            loop {
                let lowercase = output.to_lowercase();
                let Some(relative) = lowercase[offset..].find(&marker) else {
                    break;
                };
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

fn redact_bearer_tokens(output: &mut String) {
    let marker = "bearer ";
    let mut offset = 0;
    loop {
        let lowercase = output.to_lowercase();
        let Some(relative) = lowercase[offset..].find(marker) else {
            break;
        };
        let start = offset + relative + marker.len();
        let end = output[start..]
            .find(|c: char| c.is_whitespace() || c == ',' || c == '&')
            .map(|value| start + value)
            .unwrap_or(output.len());
        output.replace_range(start..end, "[REDACTED]");
        offset = start + "[REDACTED]".len();
    }
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
        let value = redact_text(
            "password=hunter2 token:abc123 password=second authorization:Bearer live-token user=demo",
        );
        assert!(!value.contains("hunter2"));
        assert!(!value.contains("abc123"));
        assert!(!value.contains("second"));
        assert!(!value.contains("live-token"));
        assert!(value.contains("user=demo"));
    }
}
