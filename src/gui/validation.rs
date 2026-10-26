use crate::config::{self, Config};
use crate::input;

#[derive(Debug, Clone)]
pub struct ValidationIssue {
    pub field: &'static str,
    pub message: String,
}

pub fn validate_config(config: &Config) -> Vec<ValidationIssue> {
    let mut issues = Vec::new();

    validate_color(&mut issues, "hint_bgcolor", &config.hint_bgcolor);
    validate_color(&mut issues, "hint_fgcolor", &config.hint_fgcolor);
    validate_color(&mut issues, "grid_color", &config.grid_color);
    validate_color(&mut issues, "cursor_color", &config.cursor_color);

    if !["static", "stdin", "detect"]
        .contains(&config.hint_source.trim().to_ascii_lowercase().as_str())
    {
        issues.push(ValidationIssue {
            field: "hint_source",
            message: "must be one of: static, stdin, detect".into(),
        });
    }

    validate_binding(
        &mut issues,
        "speed_modifier_key",
        &config.speed_modifier_key,
    );

    for (index, binding) in config.grid_quadrant_keys.iter().enumerate() {
        validate_binding(&mut issues, "grid_quadrant_keys", binding);
        if binding.trim().is_empty() {
            issues.push(ValidationIssue {
                field: "grid_quadrant_keys",
                message: format!("entry {} must not be empty", index + 1),
            });
        }
    }

    for (index, binding) in config.normal_move_keys.iter().enumerate() {
        validate_binding(&mut issues, "normal_move_keys", binding);
        if binding.trim().is_empty() {
            issues.push(ValidationIssue {
                field: "normal_move_keys",
                message: format!("entry {} must not be empty", index + 1),
            });
        }
    }

    for (index, binding) in config.buttons.iter().enumerate() {
        validate_binding(&mut issues, "buttons", binding);
        if binding.trim().is_empty() {
            issues.push(ValidationIssue {
                field: "buttons",
                message: format!("entry {} must not be empty", index + 1),
            });
        }
    }

    issues
}

fn validate_color(issues: &mut Vec<ValidationIssue>, field: &'static str, value: &str) {
    if !config::is_valid_hex_color(value) {
        issues.push(ValidationIssue {
            field,
            message: "must be #RRGGBB or #RRGGBBAA".into(),
        });
    }
}

fn validate_binding(issues: &mut Vec<ValidationIssue>, field: &'static str, value: &str) {
    if input::binding_to_keysym(value).is_none() {
        issues.push(ValidationIssue {
            field,
            message: format!("invalid keysym or character: {value}"),
        });
    }
}
