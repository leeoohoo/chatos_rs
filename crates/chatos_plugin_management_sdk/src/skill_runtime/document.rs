#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SkillDocumentError {
    #[error("Skill document must start with YAML frontmatter")]
    MissingFrontmatter,
    #[error("Skill YAML frontmatter is not terminated")]
    UnterminatedFrontmatter,
    #[error("Skill YAML frontmatter is invalid: {0}")]
    InvalidYaml(String),
    #[error("Skill field {field} is invalid: {message}")]
    InvalidField { field: String, message: String },
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct RawSkillFrontmatter {
    name: String,
    description: String,
    #[serde(default)]
    disable_model_invocation: bool,
    #[serde(default)]
    metadata: BTreeMap<String, YamlValue>,
}

pub fn parse_skill_document(
    raw: &str,
    expected_directory_name: Option<&str>,
) -> Result<ParsedSkillDocument, SkillDocumentError> {
    let normalized = raw.replace("\r\n", "\n");
    let (frontmatter, body) = split_frontmatter(normalized.as_str())?;
    let parsed: RawSkillFrontmatter = serde_yaml::from_str(frontmatter)
        .map_err(|error| SkillDocumentError::InvalidYaml(error.to_string()))?;
    let name = normalized_skill_name(parsed.name.as_str(), "name")?;
    if let Some(expected) = expected_directory_name
        .map(str::trim)
        .filter(|value| !value.is_empty())
    {
        if name != expected {
            return Err(SkillDocumentError::InvalidField {
                field: "name".to_string(),
                message: format!("must match Skill directory name {expected}"),
            });
        }
    }
    let description = parsed.description.trim().to_string();
    if description.is_empty() {
        return Err(SkillDocumentError::InvalidField {
            field: "description".to_string(),
            message: "must not be empty".to_string(),
        });
    }
    let instructions = body.trim().to_string();
    if instructions.is_empty() {
        return Err(SkillDocumentError::InvalidField {
            field: "instructions".to_string(),
            message: "must not be empty".to_string(),
        });
    }

    let role =
        parse_enum_metadata(
            &parsed.metadata,
            "chatos.role",
            SkillRole::Leaf,
            |value| match value {
                "router" => Some(SkillRole::Router),
                "leaf" => Some(SkillRole::Leaf),
                _ => None,
            },
        )?;
    let context_mode = parse_enum_metadata(
        &parsed.metadata,
        "chatos.context-mode",
        SkillContextMode::Inline,
        |value| match value {
            "inline" => Some(SkillContextMode::Inline),
            "isolated" => Some(SkillContextMode::Isolated),
            _ => None,
        },
    )?;
    let activation_policy = if parsed.disable_model_invocation {
        SkillActivationPolicy::UserOnly
    } else {
        parse_enum_metadata(
            &parsed.metadata,
            "chatos.activation-policy",
            SkillActivationPolicy::ModelOrUser,
            |value| match value {
                "model_or_user" | "model-or-user" => Some(SkillActivationPolicy::ModelOrUser),
                "user_only" | "user-only" => Some(SkillActivationPolicy::UserOnly),
                "model_only" | "model-only" => Some(SkillActivationPolicy::ModelOnly),
                _ => None,
            },
        )?
    };
    let required_skills = parse_skill_list(&parsed.metadata, "chatos.required-skills")?;
    let related_skills = parse_skill_list(&parsed.metadata, "chatos.related-skills")?;
    let max_output_chars = parse_optional_u32(&parsed.metadata, "chatos.max-output-chars")?;
    let extra = parsed
        .metadata
        .iter()
        .filter_map(|(key, value)| yaml_scalar_text(value).map(|value| (key.clone(), value)))
        .collect();

    Ok(ParsedSkillDocument {
        metadata: PackagedSkillMetadata {
            name,
            description,
            role,
            activation_policy,
            context_mode,
            required_skills,
            related_skills,
            max_output_chars,
            extra,
        },
        instructions,
    })
}

fn split_frontmatter(raw: &str) -> Result<(&str, &str), SkillDocumentError> {
    let Some(rest) = raw.strip_prefix("---\n") else {
        return Err(SkillDocumentError::MissingFrontmatter);
    };
    let Some(end) = rest.find("\n---\n") else {
        return Err(SkillDocumentError::UnterminatedFrontmatter);
    };
    Ok((&rest[..end], &rest[end + 5..]))
}

fn normalized_skill_name(value: &str, field: &str) -> Result<String, SkillDocumentError> {
    let value = value.trim();
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--");
    if !valid {
        return Err(SkillDocumentError::InvalidField {
            field: field.to_string(),
            message:
                "must use lowercase letters, digits, and single hyphens (maximum 64 characters)"
                    .to_string(),
        });
    }
    Ok(value.to_string())
}

fn parse_enum_metadata<T: Copy>(
    metadata: &BTreeMap<String, YamlValue>,
    key: &str,
    default: T,
    parse: impl FnOnce(&str) -> Option<T>,
) -> Result<T, SkillDocumentError> {
    let Some(value) = metadata.get(key) else {
        return Ok(default);
    };
    let value = yaml_scalar_text(value).ok_or_else(|| SkillDocumentError::InvalidField {
        field: format!("metadata.{key}"),
        message: "must be a scalar value".to_string(),
    })?;
    parse(value.trim()).ok_or_else(|| SkillDocumentError::InvalidField {
        field: format!("metadata.{key}"),
        message: format!("unsupported value {value}"),
    })
}

fn parse_skill_list(
    metadata: &BTreeMap<String, YamlValue>,
    key: &str,
) -> Result<Vec<String>, SkillDocumentError> {
    let Some(value) = metadata.get(key) else {
        return Ok(Vec::new());
    };
    let raw_items = match value {
        YamlValue::String(value) => value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string)
            .collect::<Vec<_>>(),
        YamlValue::Sequence(values) => values
            .iter()
            .map(|value| {
                yaml_scalar_text(value).ok_or_else(|| SkillDocumentError::InvalidField {
                    field: format!("metadata.{key}"),
                    message: "must contain only Skill names".to_string(),
                })
            })
            .collect::<Result<Vec<_>, _>>()?,
        _ => {
            return Err(SkillDocumentError::InvalidField {
                field: format!("metadata.{key}"),
                message: "must be a comma-separated string or a list".to_string(),
            })
        }
    };
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for item in raw_items {
        let item = normalized_skill_name(item.as_str(), format!("metadata.{key}").as_str())?;
        if seen.insert(item.clone()) {
            items.push(item);
        }
    }
    Ok(items)
}

fn parse_optional_u32(
    metadata: &BTreeMap<String, YamlValue>,
    key: &str,
) -> Result<Option<u32>, SkillDocumentError> {
    let Some(value) = metadata.get(key) else {
        return Ok(None);
    };
    let parsed = match value {
        YamlValue::Number(value) => value.as_u64(),
        YamlValue::String(value) => value.trim().parse::<u64>().ok(),
        _ => None,
    }
    .and_then(|value| u32::try_from(value).ok())
    .filter(|value| *value > 0)
    .ok_or_else(|| SkillDocumentError::InvalidField {
        field: format!("metadata.{key}"),
        message: "must be a positive 32-bit integer".to_string(),
    })?;
    Ok(Some(parsed))
}

fn yaml_scalar_text(value: &YamlValue) -> Option<String> {
    match value {
        YamlValue::String(value) => Some(value.trim().to_string()),
        YamlValue::Bool(value) => Some(value.to_string()),
        YamlValue::Number(value) => Some(value.to_string()),
        _ => None,
    }
}
