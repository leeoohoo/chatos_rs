use super::*;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SkillGateSelector {
    pub pointer: String,
    #[serde(default)]
    pub map: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct SkillGateDeclaration {
    #[serde(default)]
    pub all_of: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub select_by_argument: Option<SkillGateSelector>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SkillGateError {
    #[error("Plugin Skill gate declaration is invalid")]
    InvalidDeclaration,
    #[error("Plugin Skill gate contains an invalid Skill name: {0}")]
    InvalidSkillName(String),
    #[error("Plugin Skill gate requires tool arguments to be a JSON object")]
    InvalidArguments,
    #[error("Plugin Skill gate selector argument is missing: {0}")]
    MissingSelector(String),
    #[error("Plugin Skill gate selector value must be a string: {0}")]
    InvalidSelectorValue(String),
    #[error("Plugin Skill gate has no mapping for selector value: {0}")]
    UnmappedSelectorValue(String),
}

impl SkillGateError {
    pub fn code(&self) -> &'static str {
        match self {
            Self::InvalidDeclaration => "invalid_declaration",
            Self::InvalidSkillName(_) => "invalid_skill_name",
            Self::InvalidArguments => "invalid_arguments",
            Self::MissingSelector(_) => "missing_selector",
            Self::InvalidSelectorValue(_) => "invalid_selector_value",
            Self::UnmappedSelectorValue(_) => "unmapped_selector_value",
        }
    }
}

impl SkillGateDeclaration {
    pub fn validate(&self) -> Result<(), SkillGateError> {
        if self.all_of.is_empty() && self.select_by_argument.is_none() {
            return Err(SkillGateError::InvalidDeclaration);
        }
        for name in self.catalog_skill_names_unchecked() {
            if !valid_skill_name(name.as_str()) {
                return Err(SkillGateError::InvalidSkillName(name));
            }
        }
        if let Some(selector) = self.select_by_argument.as_ref() {
            if !valid_json_pointer(selector.pointer.as_str())
                || selector.map.is_empty()
                || selector.map.keys().any(|value| value.trim().is_empty())
            {
                return Err(SkillGateError::InvalidDeclaration);
            }
        }
        Ok(())
    }

    pub fn catalog_skill_names(&self) -> Result<Vec<String>, SkillGateError> {
        self.validate()?;
        Ok(self.catalog_skill_names_unchecked())
    }

    pub fn required_skill_names(
        &self,
        arguments: &serde_json::Value,
    ) -> Result<Vec<String>, SkillGateError> {
        self.validate()?;
        if !arguments.is_object() {
            return Err(SkillGateError::InvalidArguments);
        }
        let mut required = self.all_of.iter().cloned().collect::<BTreeSet<_>>();
        if let Some(selector) = self.select_by_argument.as_ref() {
            let selected = arguments
                .pointer(selector.pointer.as_str())
                .ok_or_else(|| SkillGateError::MissingSelector(selector.pointer.clone()))?;
            let selected = selected
                .as_str()
                .ok_or_else(|| SkillGateError::InvalidSelectorValue(selector.pointer.clone()))?;
            let skill_name = selector
                .map
                .get(selected)
                .ok_or_else(|| SkillGateError::UnmappedSelectorValue(selected.to_string()))?;
            required.insert(skill_name.clone());
        }
        Ok(required.into_iter().collect())
    }

    fn catalog_skill_names_unchecked(&self) -> Vec<String> {
        self.all_of
            .iter()
            .chain(
                self.select_by_argument
                    .iter()
                    .flat_map(|selector| selector.map.values()),
            )
            .cloned()
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}

fn valid_skill_name(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        && !value.starts_with('-')
        && !value.ends_with('-')
        && !value.contains("--")
}

fn valid_json_pointer(value: &str) -> bool {
    if !value.starts_with('/') {
        return false;
    }
    let bytes = value.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'~' {
            index += 1;
            if index >= bytes.len() || !matches!(bytes[index], b'0' | b'1') {
                return false;
            }
        }
        index += 1;
    }
    true
}
