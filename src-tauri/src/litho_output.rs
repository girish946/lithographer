use serde::Serialize;

/// Parsed line from `litho -o gui` stdout/stderr.
#[derive(Debug, Clone, Serialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LithoUiEvent {
    Status {
        phase: Option<String>,
        message: Option<String>,
    },
    Progress {
        phase: Option<String>,
        pct: Option<f64>,
        bytes: Option<u64>,
        total: Option<u64>,
        message: Option<String>,
    },
    Error {
        message: String,
    },
    Done {
        success: bool,
    },
    Raw {
        stream: String,
        line: String,
    },
}

pub fn parse_litho_line(stream: &str, line: &str) -> LithoUiEvent {
    let trimmed = line.trim();
    if trimmed.is_empty() {
        return LithoUiEvent::Raw {
            stream: stream.to_string(),
            line: trimmed.to_string(),
        };
    }

    if let Some(rest) = trimmed.strip_prefix("@status ") {
        let fields = parse_fields(rest);
        return LithoUiEvent::Status {
            phase: fields.get("phase").cloned(),
            message: fields.get("msg").cloned(),
        };
    }

    if let Some(rest) = trimmed.strip_prefix("@progress ") {
        let fields = parse_fields(rest);
        return LithoUiEvent::Progress {
            phase: fields.get("phase").cloned(),
            pct: fields.get("pct").and_then(|v| v.parse().ok()),
            bytes: fields.get("bytes").and_then(|v| v.parse().ok()),
            total: fields.get("total").and_then(|v| v.parse().ok()),
            message: fields.get("msg").cloned(),
        };
    }

    if let Some(rest) = trimmed.strip_prefix("@error ") {
        let fields = parse_fields(rest);
        let message = fields
            .get("msg")
            .cloned()
            .unwrap_or_else(|| rest.to_string());
        return LithoUiEvent::Error { message };
    }

    if trimmed == "@done ok" {
        return LithoUiEvent::Done { success: true };
    }

    if let Some(rest) = trimmed.strip_prefix("@done ") {
        return LithoUiEvent::Done {
            success: rest.eq_ignore_ascii_case("ok"),
        };
    }

    LithoUiEvent::Raw {
        stream: stream.to_string(),
        line: trimmed.to_string(),
    }
}

fn parse_fields(input: &str) -> std::collections::HashMap<String, String> {
    let mut fields = std::collections::HashMap::new();
    let bytes = input.as_bytes();
    let mut i = 0usize;

    while i < bytes.len() {
        while i < bytes.len() && bytes[i] == b' ' {
            i += 1;
        }
        if i >= bytes.len() {
            break;
        }

        let key_start = i;
        while i < bytes.len() && bytes[i] != b'=' {
            i += 1;
        }
        if i >= bytes.len() || bytes[i] != b'=' {
            break;
        }
        let key = &input[key_start..i];
        i += 1;

        let value = if i < bytes.len() && bytes[i] == b'"' {
            i += 1;
            let mut value = String::new();
            while i < bytes.len() {
                match bytes[i] {
                    b'\\' if i + 1 < bytes.len() => {
                        value.push(bytes[i + 1] as char);
                        i += 2;
                    }
                    b'"' => {
                        i += 1;
                        break;
                    }
                    b => {
                        value.push(b as char);
                        i += 1;
                    }
                }
            }
            value
        } else {
            let start = i;
            while i < bytes.len() && bytes[i] != b' ' {
                i += 1;
            }
            input[start..i].to_string()
        };

        fields.insert(key.to_string(), value);
    }

    fields
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_progress_line() {
        let event = parse_litho_line(
            "stdout",
            "@progress phase=writing pct=45.2 bytes=1234567 total=536870912",
        );
        match event {
            LithoUiEvent::Progress { phase, pct, bytes, total, .. } => {
                assert_eq!(phase.as_deref(), Some("writing"));
                assert!((pct.unwrap() - 45.2).abs() < f64::EPSILON);
                assert_eq!(bytes, Some(1_234_567));
                assert_eq!(total, Some(536_870_912));
            }
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parse_error_line() {
        let event = parse_litho_line("stderr", r#"@error msg="Permission denied""#);
        match event {
            LithoUiEvent::Error { message } => assert_eq!(message, "Permission denied"),
            other => panic!("unexpected event: {other:?}"),
        }
    }

    #[test]
    fn parse_done_ok() {
        let event = parse_litho_line("stdout", "@done ok");
        match event {
            LithoUiEvent::Done { success } => assert!(success),
            other => panic!("unexpected event: {other:?}"),
        }
    }
}