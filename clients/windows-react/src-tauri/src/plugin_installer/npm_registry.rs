use super::*;

pub(super) async fn fetch_npm_version_metadata(
    package_name: &str,
    version: &str,
) -> Result<Value, String> {
    let mut url = reqwest::Url::parse("https://registry.npmjs.org/")
        .map_err(|_| install_error("npm Registry 地址无效"))?;
    url.path_segments_mut()
        .map_err(|_| install_error("npm Registry 地址无效"))?
        .push(package_name)
        .push(version);
    fetch_npm_json(url).await
}

pub(super) async fn fetch_npm_json(url: reqwest::Url) -> Result<Value, String> {
    let response = npm_client(NPM_QUERY_TIMEOUT)?
        .get(url)
        .send()
        .await
        .map_err(|_| install_error("npm Registry 查询失败"))?;
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length == 0 || length > MAXIMUM_NPM_METADATA_BYTES)
    {
        return Err(install_error("npm Registry 查询失败"));
    }
    let bytes = read_bounded_response(response, MAXIMUM_NPM_METADATA_BYTES, "npm 元数据").await?;
    serde_json::from_slice(&bytes).map_err(|_| install_error("npm 返回了无法验证的包信息"))
}

pub(super) fn npm_client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .timeout(timeout)
        .redirect(reqwest::redirect::Policy::none())
        .user_agent("ChatOS-Windows/3.0.9 plugin-installer")
        .build()
        .map_err(|_| install_error("无法初始化 npm Registry 客户端"))
}

pub(super) async fn read_bounded_response(
    mut response: Response,
    maximum_bytes: u64,
    label: &str,
) -> Result<Vec<u8>, String> {
    let mut result = Vec::new();
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| install_error(&format!("{label}读取失败")))?
    {
        if (result.len() as u64)
            .checked_add(chunk.len() as u64)
            .is_none_or(|length| length > maximum_bytes)
        {
            return Err(install_error(&format!("{label}超过安全限制")));
        }
        result.extend_from_slice(&chunk);
    }
    if result.is_empty() {
        return Err(install_error(&format!("{label}为空")));
    }
    Ok(result)
}

pub(super) async fn download_npm_source_artifact(
    mut response: Response,
    destination: &Path,
    expected_sha512: &[u8],
) -> Result<(), String> {
    if !response.status().is_success()
        || response
            .content_length()
            .is_some_and(|length| length == 0 || length > MAXIMUM_ARCHIVE_BYTES)
    {
        return Err(install_error("npm Plugin 下载失败或大小超过限制"));
    }
    let mut options = tokio::fs::OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(windows)]
    {
        const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
        options
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT);
    }
    let mut file = options
        .open(destination)
        .await
        .map_err(|_| install_error("无法保存 npm Plugin"))?;
    let mut sha512 = Sha512::new();
    let mut written = 0_u64;
    while let Some(chunk) = response
        .chunk()
        .await
        .map_err(|_| install_error("npm Plugin 下载中断"))?
    {
        written = written
            .checked_add(chunk.len() as u64)
            .filter(|value| *value <= MAXIMUM_ARCHIVE_BYTES)
            .ok_or_else(|| install_error("npm Plugin 下载大小超过限制"))?;
        sha512.update(&chunk);
        file.write_all(&chunk)
            .await
            .map_err(|_| install_error("无法保存 npm Plugin"))?;
    }
    if written == 0 || sha512.finalize().as_slice() != expected_sha512 {
        return Err(install_error("npm Plugin 完整性校验失败"));
    }
    file.flush()
        .await
        .map_err(|_| install_error("无法保存 npm Plugin"))?;
    file.sync_all()
        .await
        .map_err(|_| install_error("无法保存 npm Plugin"))?;
    Ok(())
}

pub(super) fn validate_npm_tarball_url(value: &str) -> Result<reqwest::Url, String> {
    let url = reqwest::Url::parse(value).map_err(|_| install_error("npm tarball 地址无效"))?;
    if url.scheme() != "https"
        || url.host_str() != Some("registry.npmjs.org")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.query().is_some()
        || url.fragment().is_some()
    {
        return Err(install_error("npm tarball 地址不受信任"));
    }
    Ok(url)
}
