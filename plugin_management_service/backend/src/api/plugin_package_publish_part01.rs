fn analyze_packaged_skills(
    bytes: &[u8],
    manifest: &PluginManifest,
) -> Result<Vec<PluginSkillComponentSnapshot>, ApiError> {
    if manifest.skills.is_empty() {
        return Ok(Vec::new());
    }
    let mut skill_files = BTreeMap::new();
    for skill in &manifest.skills {
        let normalized = normalize_plugin_relative_path(skill.path.as_str()).map_err(|error| {
            ApiError::bad_request(format!("Plugin Skill path is invalid: {error}"))
        })?;
        let collection_path = normalized.trim_start_matches("./").trim_end_matches('/');
        if skill_files.keys().any(|existing: &String| {
            collection_path.starts_with(format!("{existing}/").as_str())
                || existing.starts_with(format!("{collection_path}/").as_str())
        }) {
            return Err(ApiError::bad_request(
                "Plugin Skill directories must not contain one another",
            ));
        }
        if skill_files
            .insert(
                collection_path.to_string(),
                PackagedSkillFiles {
                    collection_path: collection_path.to_string(),
                    skill_document: None,
                    resources: BTreeMap::new(),
                    total_resource_bytes: 0,
                },
            )
            .is_some()
        {
            return Err(ApiError::bad_request(
                "Plugin Manifest contains a duplicate Skill directory",
            ));
        }
    }

    let decoder = GzDecoder::new(Cursor::new(bytes));
    let mut archive = tar::Archive::new(decoder);
    for entry in archive.entries().map_err(|error| {
        ApiError::bad_request(format!("read npm package archive failed: {error}"))
    })? {
        let mut entry = entry.map_err(|error| {
            ApiError::bad_request(format!("read npm package entry failed: {error}"))
        })?;
        if !entry.header().entry_type().is_file() {
            continue;
        }
        let path = entry
            .path()
            .map_err(|error| {
                ApiError::bad_request(format!("read npm package path failed: {error}"))
            })?
            .into_owned();
        validate_archive_path(path.as_path())?;
        let path_text = path.to_string_lossy().into_owned();
        let Some(package_relative_path) = path_text.strip_prefix("package/") else {
            continue;
        };
        let Some((_, files)) = skill_files.iter_mut().find(|(collection_path, _)| {
            package_relative_path == format!("{collection_path}/SKILL.md")
                || package_relative_path.starts_with(format!("{collection_path}/").as_str())
        }) else {
            continue;
        };
        let relative_path = package_relative_path
            .strip_prefix(format!("{}/", files.collection_path).as_str())
            .expect("matched Skill collection prefix");
        let size = entry.size();
        if relative_path == "SKILL.md" {
            if size == 0 || size > MAX_SKILL_INSTRUCTIONS_BYTES {
                return Err(ApiError::bad_request(format!(
                    "Plugin Skill {}/SKILL.md must contain 1-{MAX_SKILL_INSTRUCTIONS_BYTES} bytes",
                    files.collection_path
                )));
            }
            let mut content = Vec::with_capacity(size as usize);
            entry.read_to_end(&mut content).map_err(|error| {
                ApiError::bad_request(format!("read Plugin Skill instructions failed: {error}"))
            })?;
            if files.skill_document.replace(content).is_some() {
                return Err(ApiError::bad_request(format!(
                    "Plugin Skill {} contains duplicate SKILL.md entries",
                    files.collection_path
                )));
            }
            continue;
        }
        if size == 0 || size > MAX_SKILL_RESOURCE_BYTES {
            return Err(ApiError::bad_request(format!(
                "Plugin Skill resource {}/{} must contain 1-{MAX_SKILL_RESOURCE_BYTES} bytes",
                files.collection_path, relative_path
            )));
        }
        if files.resources.len() >= MAX_SKILL_RESOURCE_COUNT {
            return Err(ApiError::bad_request(format!(
                "Plugin Skill {} contains too many resources",
                files.collection_path
            )));
        }
        files.total_resource_bytes = files.total_resource_bytes.saturating_add(size);
        if files.total_resource_bytes > MAX_SKILL_TOTAL_RESOURCE_BYTES {
            return Err(ApiError::bad_request(format!(
                "Plugin Skill {} resources exceed their total size limit",
                files.collection_path
            )));
        }
        let mut content = Vec::with_capacity(size as usize);
        entry.read_to_end(&mut content).map_err(|error| {
            ApiError::bad_request(format!("read Plugin Skill resource failed: {error}"))
        })?;
        if files
            .resources
            .insert(relative_path.to_string(), content)
            .is_some()
        {
            return Err(ApiError::bad_request(format!(
                "Plugin Skill {} contains duplicate resource {}",
                files.collection_path, relative_path
            )));
        }
    }

    let mut snapshots = Vec::with_capacity(skill_files.len());
    for (_, files) in skill_files {
        let skill_document = files.skill_document.ok_or_else(|| {
            ApiError::bad_request(format!(
                "Plugin Skill {} is missing SKILL.md",
                files.collection_path
            ))
        })?;
        let skill_text = std::str::from_utf8(skill_document.as_slice()).map_err(|_| {
            ApiError::bad_request(format!(
                "Plugin Skill {}/SKILL.md must use UTF-8",
                files.collection_path
            ))
        })?;
        let expected_name = files
            .collection_path
            .rsplit('/')
            .next()
            .unwrap_or(files.collection_path.as_str());
        let parsed = parse_skill_document(skill_text, Some(expected_name))
            .map_err(|error| ApiError::bad_request(error.to_string()))?;
        let resources = files
            .resources
            .into_iter()
            .map(|(relative_path, content)| RuntimeSkillResourceDescriptor {
                kind: classify_skill_resource(relative_path.as_str()),
                relative_path,
                size_bytes: content.len() as u64,
                sha256: hex::encode(Sha256::digest(content.as_slice())),
            })
            .collect::<Vec<_>>();
        let instructions_sha256 = hex::encode(Sha256::digest(skill_document.as_slice()));
        let resource_manifest_sha256 = skill_resource_manifest_sha256(resources.as_slice())
            .map_err(|error| {
                ApiError::internal(format!(
                    "hash Plugin Skill resource manifest failed: {error}"
                ))
            })?;
        let relative_skill_path = format!("{}/SKILL.md", files.collection_path);
        let snapshot_sha256 = plugin_skill_snapshot_sha256(
            parsed.metadata.name.as_str(),
            relative_skill_path.as_str(),
            &parsed.metadata,
            instructions_sha256.as_str(),
            resource_manifest_sha256.as_str(),
        )
        .map_err(|error| ApiError::internal(format!("hash Plugin Skill failed: {error}")))?;
        snapshots.push(PluginSkillComponentSnapshot {
            protocol_version: SKILL_RUNTIME_PROTOCOL_VERSION,
            skill_id: parsed.metadata.name.clone(),
            relative_skill_path,
            metadata: parsed.metadata,
            instructions_sha256,
            resource_manifest_sha256,
            resources,
            snapshot_sha256,
        });
    }
    snapshots.sort_by(|left, right| left.skill_id.cmp(&right.skill_id));
    validate_packaged_skill_dependencies(snapshots.as_slice())?;
    Ok(snapshots)
}

fn classify_skill_resource(relative_path: &str) -> SkillResourceKind {
    let first = relative_path.split('/').next().unwrap_or_default();
    match first {
        "references" => SkillResourceKind::Reference,
        "scripts" => SkillResourceKind::Script,
        "assets" => SkillResourceKind::Asset,
        _ if relative_path.ends_with(".json") || relative_path.ends_with(".schema.json") => {
            SkillResourceKind::Schema
        }
        _ => SkillResourceKind::Other,
    }
}

fn validate_packaged_skill_dependencies(
    snapshots: &[PluginSkillComponentSnapshot],
) -> Result<(), ApiError> {
    let by_name = snapshots
        .iter()
        .map(|snapshot| (snapshot.skill_id.as_str(), snapshot))
        .collect::<BTreeMap<_, _>>();
    for snapshot in snapshots {
        for dependency in snapshot
            .metadata
            .required_skills
            .iter()
            .chain(snapshot.metadata.related_skills.iter())
        {
            if !by_name.contains_key(dependency.as_str()) {
                return Err(ApiError::bad_request(format!(
                    "Plugin Skill {} references missing Skill {}",
                    snapshot.skill_id, dependency
                )));
            }
        }
    }
    fn visit<'a>(
        name: &'a str,
        by_name: &BTreeMap<&'a str, &'a PluginSkillComponentSnapshot>,
        visiting: &mut BTreeSet<&'a str>,
        visited: &mut BTreeSet<&'a str>,
    ) -> Result<(), ApiError> {
        if visited.contains(name) {
            return Ok(());
        }
        if !visiting.insert(name) {
            return Err(ApiError::bad_request(format!(
                "Plugin Skill required dependency graph contains a cycle at {name}"
            )));
        }
        let snapshot = by_name.get(name).expect("validated Skill dependency");
        for dependency in &snapshot.metadata.required_skills {
            visit(dependency.as_str(), by_name, visiting, visited)?;
        }
        visiting.remove(name);
        visited.insert(name);
        Ok(())
    }
    let mut visiting = BTreeSet::new();
    let mut visited = BTreeSet::new();
    for name in by_name.keys().copied() {
        visit(name, &by_name, &mut visiting, &mut visited)?;
    }
    Ok(())
}

fn package_bins(value: &Value, package_name: &str) -> Result<Vec<PackageBin>, ApiError> {
    let values = match value {
        Value::String(target) => BTreeMap::from([(
            package_name
                .rsplit('/')
                .next()
                .unwrap_or(package_name)
                .to_string(),
            target.as_str(),
        )]),
        Value::Object(values) => values
            .iter()
            .map(|(name, target)| {
                target
                    .as_str()
                    .map(|target| (name.clone(), target))
                    .ok_or_else(|| ApiError::bad_request("package.json.bin values must be strings"))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?,
        Value::Null => BTreeMap::new(),
        _ => {
            return Err(ApiError::bad_request(
                "package.json.bin must be a string or object",
            ))
        }
    };
    values
        .into_iter()
        .map(|(name, target)| {
            let name = name.trim();
            if name.is_empty() || name.contains('/') || name.contains('\\') {
                return Err(ApiError::bad_request(
                    "package.json.bin names must be non-empty executable names",
                ));
            }
            let target = target.trim().strip_prefix("./").unwrap_or(target.trim());
            let target_path = FsPath::new(target);
            if target.is_empty()
                || target_path.is_absolute()
                || target_path
                    .components()
                    .any(|component| !matches!(component, Component::Normal(_)))
            {
                return Err(ApiError::bad_request(
                    "package.json.bin targets must be safe relative package paths",
                ));
            }
            Ok(PackageBin {
                name: name.to_string(),
                archive_path: format!("package/{target}"),
            })
        })
        .collect()
}

fn validate_archive_path(path: &FsPath) -> Result<(), ApiError> {
    if path.is_absolute()
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
        || path
            .components()
            .next()
            .and_then(|component| match component {
                Component::Normal(value) => value.to_str(),
                _ => None,
            })
            != Some("package")
    {
        return Err(ApiError::bad_request(
            "npm package contains an unsafe archive path",
        ));
    }
    Ok(())
}

async fn ensure_managed_release_key(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    publisher: &PluginPublisherRecord,
) -> Result<(SigningKeyRef, Ed25519KeyPair), ApiError> {
    let (key_ref, key_pair) = load_or_create_managed_key(state, marketplace, publisher)?;
    if !publisher
        .signing_keys
        .iter()
        .any(|key| key.key_id == key_ref.key_id)
    {
        let mut updated = publisher.clone();
        updated.signing_keys.push(key_ref.clone());
        updated
            .signing_keys
            .sort_by(|left, right| left.key_id.cmp(&right.key_id));
        updated.updated_at = now_rfc3339();
        if !state
            .store
            .replace_plugin_publisher_if_matches(publisher, &updated)
            .await
            .map_err(ApiError::internal)?
        {
            return Err(ApiError::conflict(
                "Plugin publisher changed concurrently; retry publishing",
            ));
        }
    }
    if !marketplace
        .trusted_signing_keys
        .iter()
        .any(|key| key.key_id == key_ref.key_id)
    {
        let mut updated = marketplace.clone();
        updated.trusted_signing_keys.push(key_ref.clone());
        updated
            .trusted_signing_keys
            .sort_by(|left, right| left.key_id.cmp(&right.key_id));
        if !state
            .store
            .replace_plugin_marketplace_if_matches_with_catalog_sync(
                marketplace,
                &updated,
                is_syncable_network_marketplace(&updated),
            )
            .await
            .map_err(ApiError::internal)?
        {
            return Err(ApiError::conflict(
                "Plugin marketplace changed concurrently; retry publishing",
            ));
        }
    }
    Ok((key_ref, key_pair))
}

fn load_or_create_managed_key(
    state: &AppState,
    marketplace: &PluginMarketplaceRecord,
    publisher: &PluginPublisherRecord,
) -> Result<(SigningKeyRef, Ed25519KeyPair), ApiError> {
    let key_dir = state
        .config
        .plugin_artifact_storage_dir
        .join("managed-signing");
    fs::create_dir_all(key_dir.as_path()).map_err(|error| {
        ApiError::internal(format!("create managed signer directory failed: {error}"))
    })?;
    restrict_directory_permissions(key_dir.as_path())?;
    let scope_hash = hex::encode(Sha256::digest(
        format!("{}\0{}", marketplace.id, publisher.publisher_id).as_bytes(),
    ));
    let path = key_dir.join(format!("{}.pk8", &scope_hash[..32]));
    let key_bytes = if path.exists() {
        Zeroizing::new(fs::read(path.as_path()).map_err(|error| {
            ApiError::internal(format!("read managed Release signing key failed: {error}"))
        })?)
    } else {
        let document = Ed25519KeyPair::generate_pkcs8(&SystemRandom::new())
            .map_err(|_| ApiError::internal("generate managed Release signing key failed"))?;
        write_artifact_atomically(path.as_path(), document.as_ref())?;
        restrict_file_permissions(path.as_path())?;
        Zeroizing::new(document.as_ref().to_vec())
    };
    let key_pair = Ed25519KeyPair::from_pkcs8(key_bytes.as_slice())
        .map_err(|_| ApiError::internal("managed Release signing key is invalid"))?;
    let public_key_base64 = STANDARD.encode(key_pair.public_key().as_ref());
    let key_id = format!(
        "managed-{}",
        &hex::encode(Sha256::digest(key_pair.public_key().as_ref()))[..24]
    );
    let existing = publisher
        .signing_keys
        .iter()
        .find(|key| key.key_id == key_id);
    let key_ref = existing.cloned().unwrap_or_else(|| SigningKeyRef {
        key_id,
        publisher_id: publisher.publisher_id.clone(),
        algorithm: PLUGIN_SIGNATURE_ALGORITHM_ED25519.to_string(),
        public_key_base64,
        usages: vec![PLUGIN_SIGNING_KEY_USAGE_RELEASE.to_string()],
        valid_from: now_rfc3339(),
        valid_until: None,
        revoked_at: None,
    });
    Ok((key_ref, key_pair))
}

fn read_stored_artifact_metadata(
    state: &AppState,
    artifact_sha256: &str,
) -> Result<StoredPluginArtifactMetadata, ApiError> {
    let bytes = fs::read(artifact_metadata_path(state, artifact_sha256)).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ApiError::not_found("uploaded Plugin artifact metadata not found")
        } else {
            ApiError::internal(format!("read Plugin artifact metadata failed: {error}"))
        }
    })?;
    serde_json::from_slice(bytes.as_slice()).map_err(|error| {
        ApiError::internal(format!("decode Plugin artifact metadata failed: {error}"))
    })
}

fn verify_stored_artifact(
    state: &AppState,
    stored: &StoredPluginArtifactMetadata,
) -> Result<(), ApiError> {
    let bytes = fs::read(artifact_package_path(
        state,
        stored.artifact_sha256.as_str(),
    ))
    .map_err(|error| {
        ApiError::internal(format!("read uploaded Plugin artifact failed: {error}"))
    })?;
    if bytes.len() > state.config.plugin_artifact_max_bytes
        || hex::encode(Sha256::digest(bytes.as_slice())) != stored.artifact_sha256
        || format!(
            "sha512-{}",
            STANDARD.encode(Sha512::digest(bytes.as_slice()))
        ) != stored.npm_package.integrity
    {
        return Err(ApiError::conflict(
            "uploaded Plugin artifact integrity has changed",
        ));
    }
    Ok(())
}

fn artifact_package_path(state: &AppState, artifact_sha256: &str) -> PathBuf {
    state
        .config
        .plugin_artifact_storage_dir
        .join(format!("{artifact_sha256}.tgz"))
}

fn artifact_metadata_path(state: &AppState, artifact_sha256: &str) -> PathBuf {
    state
        .config
        .plugin_artifact_storage_dir
        .join(format!("{artifact_sha256}.json"))
}

fn write_artifact_atomically(path: &FsPath, bytes: &[u8]) -> Result<(), ApiError> {
    if path.exists() {
        let existing = fs::read(path).map_err(|error| {
            ApiError::internal(format!("read immutable Plugin artifact failed: {error}"))
        })?;
        return if existing == bytes {
            Ok(())
        } else {
            Err(ApiError::conflict(
                "immutable Plugin artifact path already contains different content",
            ))
        };
    }
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::internal("Plugin artifact path has no parent"))?;
    fs::create_dir_all(parent).map_err(|error| {
        ApiError::internal(format!("create Plugin artifact directory failed: {error}"))
    })?;
    let temporary = parent.join(format!(".{}.tmp", Uuid::new_v4()));
    fs::write(temporary.as_path(), bytes)
        .map_err(|error| ApiError::internal(format!("write Plugin artifact failed: {error}")))?;
    restrict_file_permissions(temporary.as_path())?;
    match fs::rename(temporary.as_path(), path) {
        Ok(()) => Ok(()),
        Err(_error) if path.exists() => {
            let existing = fs::read(path).map_err(|error| {
                ApiError::internal(format!(
                    "read concurrently committed Plugin artifact failed: {error}"
                ))
            })?;
            let _ = fs::remove_file(temporary);
            if existing == bytes {
                Ok(())
            } else {
                Err(ApiError::conflict(
                    "immutable Plugin artifact path was concurrently committed with different content",
                ))
            }
        }
        Err(error) => {
            let _ = fs::remove_file(temporary);
            Err(ApiError::internal(format!(
                "commit Plugin artifact failed: {error}"
            )))
        }
    }
}

fn normalize_optional_https_url(value: Option<&str>) -> Result<Option<String>, ApiError> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    let url = reqwest::Url::parse(value)
        .map_err(|_| ApiError::bad_request("license_url is not a valid URL"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err(ApiError::bad_request(
            "license_url must be a plain HTTPS URL",
        ));
    }
    Ok(Some(value.to_string()))
}

#[cfg(unix)]
fn restrict_directory_permissions(path: &FsPath) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| ApiError::internal(format!("protect signer directory failed: {error}")))
}

#[cfg(not(unix))]
fn restrict_directory_permissions(_path: &FsPath) -> Result<(), ApiError> {
    Ok(())
}

#[cfg(unix)]
fn restrict_file_permissions(path: &FsPath) -> Result<(), ApiError> {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(0o600)).map_err(|error| {
        ApiError::internal(format!("protect Plugin artifact file failed: {error}"))
    })
}

#[cfg(not(unix))]
fn restrict_file_permissions(_path: &FsPath) -> Result<(), ApiError> {
    Ok(())
}

fn default_public_visibility() -> String {
    PLUGIN_VISIBILITY_PUBLIC.to_string()
}

fn default_stable_channel() -> String {
    "stable".to_string()
}
