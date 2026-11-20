use super::*;

pub(super) fn acquire_local_package(
    plugin_root: &Path,
    source: &Path,
) -> Result<AcquiredPluginSource, String> {
    let plugin_root = canonical_plugin_root(plugin_root)?;
    let source_root = local_source_root(source, None)?;
    if source_root == plugin_root
        || source_root.starts_with(&plugin_root)
        || plugin_root.starts_with(&source_root)
    {
        return Err(install_error("Plugin 来源目录与安装目录不能重叠"));
    }
    let staging_root = plugin_root
        .join(".source-staging")
        .join(Uuid::new_v4().to_string());
    let package_root = staging_root.join("package");
    ensure_owned_directory(&package_root).map_err(|_| install_error("无法创建 Plugin 临时目录"))?;
    let acquired = AcquiredPluginSource {
        plugin_root,
        staging_root,
        package_root,
    };
    snapshot_local_tree(&source_root, Some(&acquired.package_root))?;
    Ok(acquired)
}

pub(super) fn install_copied_package(
    plugin_root: &Path,
    source_root: &Path,
    expected_fingerprint: &str,
) -> Result<CommittedPluginPackage, String> {
    let staging_container = plugin_root
        .join(".staging")
        .join(Uuid::new_v4().to_string());
    let package_root = staging_container.join("package");
    ensure_owned_directory(&package_root)
        .map_err(|_| install_error("无法创建 Plugin 临时安装目录"))?;
    let staged = (|| {
        let fingerprint = snapshot_local_tree(source_root, Some(&package_root))?;
        if fingerprint != expected_fingerprint {
            return Err(install_error("Plugin 内容自预检后已变化，请重新检查"));
        }
        let manifest = validate_local_package(&package_root)?;
        Ok((manifest, fingerprint))
    })();
    let (manifest, fingerprint) = match staged {
        Ok(value) => value,
        Err(error) => {
            let _ = remove_confined_directory(plugin_root, &staging_container);
            return Err(error);
        }
    };
    let plugin_directory = plugin_root.join(sha256_text(&format!("local:{}", manifest.name)));
    ensure_owned_directory(&plugin_directory)
        .map_err(|_| install_error("无法创建 Plugin 版本目录"))?;
    let final_root = plugin_directory.join(sha256_text(&manifest.version));
    let backup_root = if final_root.exists() {
        let backup_directory = plugin_root.join(".backups");
        ensure_owned_directory(&backup_directory)
            .map_err(|_| install_error("无法创建 Plugin 回滚目录"))?;
        let backup = backup_directory.join(Uuid::new_v4().to_string());
        fs::rename(&final_root, &backup).map_err(|_| install_error("无法准备 Plugin 安装回滚"))?;
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::rename(&package_root, &final_root) {
        if let Some(backup) = &backup_root {
            let _ = fs::rename(backup, &final_root);
        }
        let _ = remove_confined_directory(plugin_root, &staging_container);
        return Err(install_error(&format!("无法提交 Plugin 安装：{error}")));
    }
    let _ = remove_confined_directory(plugin_root, &staging_container);
    if let Err(error) = write_installation_integrity(plugin_root, &final_root, &fingerprint) {
        let _ = remove_confined_directory(plugin_root, &final_root);
        if let Some(backup) = &backup_root {
            let _ = fs::rename(backup, &final_root);
        }
        return Err(error);
    }
    Ok(CommittedPluginPackage {
        plugin_root: plugin_root.to_path_buf(),
        installation_root: final_root,
        manifest,
        artifact_sha256: fingerprint,
        backup_root,
    })
}

pub(super) async fn acquire_git_package(
    plugin_root: &Path,
    repository_url: &str,
    git_reference: Option<&str>,
) -> Result<AcquiredPluginSource, String> {
    validate_git_repository_url(repository_url)?;
    let reference = validate_git_reference(git_reference)?;
    let plugin_root = canonical_plugin_root(plugin_root)?;
    let staging_root = plugin_root
        .join(".source-staging")
        .join(Uuid::new_v4().to_string());
    let git_root = staging_root.join("repository.git");
    let archive_path = staging_root.join("repository.tar");
    let package_root = staging_root.join("package");
    ensure_owned_directory(&git_root).map_err(|_| install_error("无法创建 Git Plugin 临时目录"))?;
    let acquired = AcquiredPluginSource {
        plugin_root,
        staging_root,
        package_root,
    };

    run_plugin_git(
        &["init", "--bare", "--quiet", "."],
        &git_root,
        "无法初始化 Git Plugin 临时仓库",
    )
    .await?;
    run_plugin_git(
        &[
            "-c",
            git_null_hooks_config(),
            "-c",
            "credential.helper=",
            "-c",
            "protocol.file.allow=never",
            "-c",
            "submodule.recurse=false",
            "fetch",
            "--quiet",
            "--depth",
            "1",
            "--no-tags",
            repository_url,
            &reference,
        ],
        &git_root,
        "Git Plugin 获取失败，请检查仓库地址和 ref",
    )
    .await?;
    run_plugin_git_archive(&git_root, &archive_path).await?;
    extract_git_archive(&archive_path, &acquired.package_root)?;
    Ok(acquired)
}

pub(super) async fn acquire_npm_package(
    plugin_root: &Path,
    package_name: &str,
    exact_version: &str,
) -> Result<AcquiredPluginSource, String> {
    validate_npm_package_name(package_name)?;
    validate_exact_version(exact_version)?;
    let metadata = fetch_npm_version_metadata(package_name, exact_version).await?;
    if metadata.get("name").and_then(Value::as_str) != Some(package_name)
        || metadata.get("version").and_then(Value::as_str) != Some(exact_version)
    {
        return Err(install_error("npm Plugin 身份与固定版本不一致"));
    }
    let dist = metadata
        .get("dist")
        .and_then(Value::as_object)
        .ok_or_else(|| install_error("npm Plugin 缺少发布完整性信息"))?;
    let tarball = dist
        .get("tarball")
        .and_then(Value::as_str)
        .ok_or_else(|| install_error("npm Plugin 缺少 tarball"))?;
    let integrity = dist
        .get("integrity")
        .and_then(Value::as_str)
        .ok_or_else(|| install_error("npm Plugin 缺少 sha512 integrity"))?;
    let tarball_url = validate_npm_tarball_url(tarball)?;
    let expected_sha512 = parse_npm_integrity(integrity)?;

    let plugin_root = canonical_plugin_root(plugin_root)?;
    let staging_root = plugin_root
        .join(".source-staging")
        .join(Uuid::new_v4().to_string());
    ensure_owned_directory(&staging_root)
        .map_err(|_| install_error("无法创建 npm Plugin 临时目录"))?;
    let acquired = AcquiredPluginSource {
        plugin_root,
        package_root: staging_root.join("package"),
        staging_root,
    };
    let archive_path = acquired.staging_root.join("package.tgz");
    let client = npm_client(NPM_DOWNLOAD_TIMEOUT)?;
    let response = client
        .get(tarball_url)
        .send()
        .await
        .map_err(|_| install_error("npm Plugin 下载失败"))?;
    download_npm_source_artifact(response, &archive_path, &expected_sha512).await?;
    let staging_root = acquired.staging_root.clone();
    let archive = archive_path.clone();
    tauri::async_runtime::spawn_blocking(move || extract_archive(&archive, &staging_root))
        .await
        .map_err(|_| install_error("npm Plugin 解包任务异常退出"))??;
    let package_json = read_bounded_text(
        &acquired.package_root.join("package.json"),
        MAXIMUM_PACKAGE_JSON_BYTES,
    )?;
    let package_json: Value = serde_json::from_str(&package_json)
        .map_err(|_| install_error("npm Plugin package.json 无效"))?;
    if package_json.get("name").and_then(Value::as_str) != Some(package_name)
        || package_json.get("version").and_then(Value::as_str) != Some(exact_version)
    {
        return Err(install_error("npm Plugin 解包身份与固定版本不一致"));
    }
    Ok(acquired)
}
