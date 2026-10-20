use super::models::{PresentedRemoteConnectionTestResult, RemoteCredentialBundle};
use crate::terminal_runtime::process::ProcessTermination;
use chatos_local_agent_protocol::{
    LocalRemoteAuthenticationType, LocalRemoteConnectionSpec, LocalRemoteHostKeyPolicy,
};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::{self, File, OpenOptions},
    future::Future,
    io::{Read, Write},
    path::{Path, PathBuf},
    pin::Pin,
    process::Stdio,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    process::Command,
    sync::{watch, Mutex, Notify},
    time::{interval, timeout, Instant, MissedTickBehavior},
};
use uuid::Uuid;

const ASKPASS_MODE_VARIABLE: &str = "CHATOS_SSH_ASKPASS_MODE";
const TARGET_ALIAS: &str = "chatos-target";
const JUMP_ALIAS: &str = "chatos-jump";
const SUCCESS_MARKER: &str = "__CHATOS_REMOTE_OK__";
const DEFAULT_TEST_TIMEOUT: Duration = Duration::from_secs(15);
const INTERACTIVE_SESSION_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const TERMINATION_GRACE_PERIOD: Duration = Duration::from_secs(1);
const MAXIMUM_STDOUT_BYTES: usize = 64 * 1_024;
const MAXIMUM_DIAGNOSTIC_BYTES: usize = 1_024 * 1_024;
const MAXIMUM_PROMPT_BYTES: usize = 64 * 1_024;
const MAXIMUM_PRESENTED_PROMPT_CHARACTERS: usize = 1_000;

pub(super) type TestFuture<'a> =
    Pin<Box<dyn Future<Output = Result<PresentedRemoteConnectionTestResult, String>> + Send + 'a>>;
pub(super) type ResetFuture<'a> = Pin<Box<dyn Future<Output = ()> + Send + 'a>>;

pub(super) trait RemoteConnectionTester: Send + Sync {
    fn test(
        &self,
        connection: SshConnectionMaterial,
        verification_code: Option<String>,
    ) -> TestFuture<'_>;
    fn reset(&self) -> ResetFuture<'_>;
}

#[derive(Clone)]
pub(super) struct SshConnectionMaterial {
    pub(super) spec: LocalRemoteConnectionSpec,
    pub(super) credentials: RemoteCredentialBundle,
    pub(super) jump_authentication_type: Option<LocalRemoteAuthenticationType>,
    pub(super) jump_credentials: Option<RemoteCredentialBundle>,
}

pub(super) struct NativeOpenSshTester {
    runtime_root: PathBuf,
    timeout: Duration,
    sessions: Arc<Mutex<HashMap<String, Arc<ActiveSshSession>>>>,
}

impl NativeOpenSshTester {
    pub(super) fn new(app_data_directory: PathBuf) -> Self {
        Self {
            runtime_root: app_data_directory
                .join("remote-connections")
                .join("runtime"),
            timeout: DEFAULT_TEST_TIMEOUT,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    #[cfg(test)]
    fn with_timeout(runtime_root: PathBuf, timeout: Duration) -> Self {
        Self {
            runtime_root,
            timeout,
            sessions: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    async fn run_test(
        &self,
        connection: SshConnectionMaterial,
        verification_code: Option<String>,
    ) -> Result<PresentedRemoteConnectionTestResult, String> {
        validate_material(&connection)?;
        let verification_code = normalize_verification_code(verification_code)?;
        let fingerprint = connection_fingerprint(&connection);
        let (session, submitted_prompt_count) = if let Some(code) = verification_code.as_deref() {
            let existing = self.sessions.lock().await.get(&fingerprint).cloned();
            let session = existing
                .ok_or_else(|| "SSH 二次验证会话已过期，请重新执行连接测试。".to_owned())?;
            let prompt_count = verification_prompts(&session.prompts()).len();
            session.write_verification_code(code)?;
            (session, Some(prompt_count))
        } else {
            self.discard(&fingerprint).await;
            let session = self.start_session(fingerprint.clone(), connection).await?;
            (session, None)
        };
        let mut cancellation_guard = SessionCancellationGuard::new(session.clone());
        let result = self
            .wait_for_session(
                fingerprint,
                session,
                verification_code.is_some(),
                submitted_prompt_count,
            )
            .await;
        cancellation_guard.disarm();
        result
    }

    async fn start_session(
        &self,
        fingerprint: String,
        connection: SshConnectionMaterial,
    ) -> Result<Arc<ActiveSshSession>, String> {
        let prepared = Arc::new(PreparedSshSession::create(&self.runtime_root, &connection)?);
        let executable = ssh_executable()?;
        let askpass =
            std::env::current_exe().map_err(|_| "无法定位 ChatOS SSH 凭据助手。".to_owned())?;
        validate_regular_executable(&askpass, "ChatOS SSH 凭据助手不可用。")?;
        let mut environment = inherited_ssh_environment();
        restrict_open_ssh_search_environment(&mut environment, &executable)?;
        environment.extend([
            (
                OsString::from("SSH_ASKPASS"),
                askpass.as_os_str().to_owned(),
            ),
            (
                OsString::from("SSH_ASKPASS_REQUIRE"),
                OsString::from("force"),
            ),
            (OsString::from(ASKPASS_MODE_VARIABLE), OsString::from("1")),
            (OsString::from("DISPLAY"), OsString::from("chatos:0")),
            (
                OsString::from("CHATOS_SSH_PROMPT_LOG"),
                prepared.prompt_log.as_os_str().to_owned(),
            ),
            (
                OsString::from("CHATOS_SSH_VERIFICATION_FILE"),
                prepared.verification_response.as_os_str().to_owned(),
            ),
            (
                OsString::from("CHATOS_SSH_TARGET_PASSWORD"),
                OsString::from(connection.credentials.password.as_deref().unwrap_or("")),
            ),
            (
                OsString::from("CHATOS_SSH_JUMP_PASSWORD"),
                OsString::from(prepared.jump_password.as_deref().unwrap_or("")),
            ),
            (
                OsString::from("CHATOS_SSH_JUMP_HOST"),
                OsString::from(connection.spec.jump_host.as_deref().unwrap_or("")),
            ),
            (
                OsString::from("CHATOS_SSH_JUMP_USER"),
                OsString::from(connection.spec.jump_username.as_deref().unwrap_or("")),
            ),
        ]);
        let arguments = vec![
            OsString::from("-v"),
            OsString::from("-F"),
            prepared.config.as_os_str().to_owned(),
            OsString::from(TARGET_ALIAS),
            OsString::from(format!("printf '{SUCCESS_MARKER} '; hostname")),
        ];
        let (cancel, cancellation) = watch::channel(false);
        let session = Arc::new(ActiveSshSession {
            connection,
            prepared: prepared.clone(),
            cancel,
            completion: Mutex::new(None),
            changed: Notify::new(),
        });
        {
            let mut sessions = self.sessions.lock().await;
            if let Some(previous) = sessions.insert(fingerprint.clone(), session.clone()) {
                previous.cancel();
            }
        }
        let task_session = session.clone();
        let task_sessions = self.sessions.clone();
        let task_fingerprint = fingerprint;
        tokio::spawn(async move {
            let result = run_isolated_process(
                &executable,
                &arguments,
                &environment,
                &prepared.prompt_log,
                cancellation,
                INTERACTIVE_SESSION_TIMEOUT,
            )
            .await;
            *task_session.completion.lock().await = Some(result);
            task_session.changed.notify_waiters();
            let mut sessions = task_sessions.lock().await;
            if sessions
                .get(&task_fingerprint)
                .is_some_and(|current| Arc::ptr_eq(current, &task_session))
            {
                sessions.remove(&task_fingerprint);
            }
        });
        Ok(session)
    }

    async fn wait_for_session(
        &self,
        fingerprint: String,
        session: Arc<ActiveSshSession>,
        verification_code_was_submitted: bool,
        submitted_prompt_count: Option<usize>,
    ) -> Result<PresentedRemoteConnectionTestResult, String> {
        let deadline = Instant::now() + self.timeout;
        let mut prompt_poll = interval(Duration::from_millis(100));
        prompt_poll.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            if let Some(result) = session.completion.lock().await.take() {
                self.remove_if_current(&fingerprint, &session).await;
                let outcome = result?;
                if !verification_code_was_submitted {
                    if let Some(prompt) = verification_prompt(&outcome.prompts, &session.connection)
                    {
                        return Ok(PresentedRemoteConnectionTestResult::verification(prompt));
                    }
                }
                return finish_outcome(outcome);
            }
            let prompts = session.prompts();
            let verification_prompts = verification_prompts(&prompts);
            if !verification_code_was_submitted {
                if let Some(prompt) = verification_prompts.first() {
                    self.schedule_expiration(fingerprint.clone(), session.clone());
                    return Ok(PresentedRemoteConnectionTestResult::verification(
                        sanitize_verification_prompt(prompt, &session.connection),
                    ));
                }
            } else if let Some(submitted_prompt_count) = submitted_prompt_count {
                if verification_prompts.len() > submitted_prompt_count {
                    self.schedule_expiration(fingerprint.clone(), session.clone());
                    return Ok(PresentedRemoteConnectionTestResult::verification(
                        sanitize_verification_prompt(
                            verification_prompts
                                .last()
                                .expect("new verification prompt"),
                            &session.connection,
                        ),
                    ));
                }
            }
            tokio::select! {
                _ = session.changed.notified() => {}
                _ = prompt_poll.tick() => {}
                _ = tokio::time::sleep_until(deadline) => {
                    self.remove_if_current(&fingerprint, &session).await;
                    session.cancel();
                    return Err(if prompts.trim().is_empty() {
                        "SSH 连接超时，请检查主机地址、端口和网络。".into()
                    } else {
                        "已连接到 SSH 服务器，但认证等待超时。请使用最新验证码后重试。".into()
                    });
                }
            }
        }
    }

    async fn discard(&self, fingerprint: &str) {
        if let Some(session) = self.sessions.lock().await.remove(fingerprint) {
            session.cancel();
        }
    }

    async fn remove_if_current(&self, fingerprint: &str, session: &Arc<ActiveSshSession>) {
        let mut sessions = self.sessions.lock().await;
        if sessions
            .get(fingerprint)
            .is_some_and(|current| Arc::ptr_eq(current, session))
        {
            sessions.remove(fingerprint);
        }
    }

    fn schedule_expiration(&self, fingerprint: String, session: Arc<ActiveSshSession>) {
        let sessions = self.sessions.clone();
        let session = Arc::downgrade(&session);
        tokio::spawn(async move {
            tokio::time::sleep(INTERACTIVE_SESSION_TIMEOUT).await;
            let Some(session) = session.upgrade() else {
                return;
            };
            let mut sessions = sessions.lock().await;
            if sessions
                .get(&fingerprint)
                .is_some_and(|current| Arc::ptr_eq(current, &session))
            {
                sessions.remove(&fingerprint);
                session.cancel();
            }
        });
    }

    async fn reset_sessions(&self) {
        let sessions = {
            let mut active = self.sessions.lock().await;
            active
                .drain()
                .map(|(_, session)| session)
                .collect::<Vec<_>>()
        };
        for session in &sessions {
            session.cancel();
        }
        for session in sessions {
            let _ = timeout(TERMINATION_GRACE_PERIOD * 2, async {
                loop {
                    let changed = session.changed.notified();
                    if session.completion.lock().await.is_some() {
                        return;
                    }
                    changed.await;
                }
            })
            .await;
        }
    }
}

struct ActiveSshSession {
    connection: SshConnectionMaterial,
    prepared: Arc<PreparedSshSession>,
    cancel: watch::Sender<bool>,
    completion: Mutex<Option<Result<ProcessOutcome, String>>>,
    changed: Notify,
}

impl ActiveSshSession {
    fn prompts(&self) -> String {
        read_bounded_text(&self.prepared.prompt_log, MAXIMUM_PROMPT_BYTES)
    }

    fn write_verification_code(&self, code: &str) -> Result<(), String> {
        self.prepared.write_verification_code(code)
    }

    fn cancel(&self) {
        let _ = self.cancel.send(true);
    }
}

struct SessionCancellationGuard {
    session: Arc<ActiveSshSession>,
    armed: bool,
}

impl SessionCancellationGuard {
    fn new(session: Arc<ActiveSshSession>) -> Self {
        Self {
            session,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for SessionCancellationGuard {
    fn drop(&mut self) {
        if self.armed {
            self.session.cancel();
        }
    }
}

impl RemoteConnectionTester for NativeOpenSshTester {
    fn test(
        &self,
        connection: SshConnectionMaterial,
        verification_code: Option<String>,
    ) -> TestFuture<'_> {
        Box::pin(self.run_test(connection, verification_code))
    }

    fn reset(&self) -> ResetFuture<'_> {
        Box::pin(self.reset_sessions())
    }
}

struct PreparedSshSession {
    directory: PathBuf,
    config: PathBuf,
    prompt_log: PathBuf,
    verification_response: PathBuf,
    jump_password: Option<String>,
}

impl PreparedSshSession {
    fn create(root: &Path, material: &SshConnectionMaterial) -> Result<Self, String> {
        fs::create_dir_all(root).map_err(|_| runtime_error())?;
        reject_reparse_point(root)?;
        let directory = root.join(format!("connection-test-{}", Uuid::new_v4()));
        fs::create_dir(&directory).map_err(|_| runtime_error())?;
        set_private_directory_permissions(&directory)?;
        let config = directory.join("ssh_config");
        let prompt_log = directory.join("prompts.log");
        let verification_response = directory.join("verification-response");
        write_private_file(&config, ssh_config(material)?.as_bytes())?;
        write_private_file(&prompt_log, &[])?;
        write_private_file(&verification_response, &[])?;
        let jump_password = if material.spec.jump_enabled {
            if material.spec.jump_connection_id.is_some() {
                material
                    .jump_credentials
                    .as_ref()
                    .and_then(|credentials| credentials.password.clone())
            } else {
                material.credentials.jump_password.clone()
            }
        } else {
            None
        };
        Ok(Self {
            directory,
            config,
            prompt_log,
            verification_response,
            jump_password,
        })
    }

    fn write_verification_code(&self, code: &str) -> Result<(), String> {
        let metadata =
            fs::symlink_metadata(&self.verification_response).map_err(|_| runtime_error())?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return Err(runtime_error());
        }
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&self.verification_response)
            .map_err(|_| runtime_error())?;
        file.write_all(code.as_bytes())
            .map_err(|_| runtime_error())?;
        file.sync_all().map_err(|_| runtime_error())
    }
}

impl Drop for PreparedSshSession {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.directory);
    }
}

struct ProcessOutcome {
    exit_code: i32,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    prompts: String,
    timed_out: bool,
    output_truncated: bool,
}

async fn run_isolated_process(
    executable: &Path,
    arguments: &[OsString],
    environment: &[(OsString, OsString)],
    prompt_log: &Path,
    mut cancellation: watch::Receiver<bool>,
    duration: Duration,
) -> Result<ProcessOutcome, String> {
    let working_directory = prompt_log.parent().ok_or_else(runtime_error)?;
    let mut command = Command::new(executable);
    command
        .args(arguments)
        .env_clear()
        .envs(environment.iter().cloned())
        .current_dir(working_directory)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(false);
    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        command.as_std_mut().process_group(0);
    }
    #[cfg(windows)]
    {
        const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW);
    }
    let mut child = command
        .spawn()
        .map_err(|_| "无法启动 Windows OpenSSH。".to_owned())?;
    let termination = ProcessTermination::attach(&child).map_err(|_| {
        let _ = child.start_kill();
        "无法隔离 Windows OpenSSH 进程树。".to_owned()
    })?;
    let stdout = child.stdout.take().ok_or_else(runtime_error)?;
    let stderr = child.stderr.take().ok_or_else(runtime_error)?;
    let stdout_exceeded = Arc::new(AtomicBool::new(false));
    let stderr_exceeded = Arc::new(AtomicBool::new(false));
    let stdout_task = tokio::spawn(read_bounded_pipe(
        stdout,
        MAXIMUM_STDOUT_BYTES,
        stdout_exceeded.clone(),
    ));
    let stderr_task = tokio::spawn(read_bounded_pipe(
        stderr,
        MAXIMUM_DIAGNOSTIC_BYTES,
        stderr_exceeded.clone(),
    ));
    let mut wait = Box::pin(child.wait());
    let deadline = Instant::now() + duration;
    let mut timed_out = false;
    let mut cancelled = false;
    let status = loop {
        tokio::select! {
            result = &mut wait => {
                break result.map_err(|_| "Windows OpenSSH 状态不可用。".to_owned())?;
            }
            changed = cancellation.changed() => {
                if changed.is_err() || *cancellation.borrow() {
                    cancelled = true;
                    let _ = termination.terminate(false);
                    break wait_after_termination(&termination, &mut wait).await;
                }
            }
            _ = tokio::time::sleep_until(deadline) => {
                timed_out = true;
                let _ = termination.terminate(false);
                break wait_after_termination(&termination, &mut wait).await;
            }
        }
    };
    drop(wait);
    // ProxyJump and AskPass descendants can retain stdout/stderr after ssh exits.
    // Closing the job/process group here guarantees bounded drain completion.
    let _ = termination.terminate(true);
    let stdout = join_pipe(stdout_task).await?;
    let stderr = join_pipe(stderr_task).await?;
    let prompts = read_bounded_text(prompt_log, MAXIMUM_PROMPT_BYTES);
    if cancelled {
        return Err("SSH 连接测试已取消。".into());
    }
    Ok(ProcessOutcome {
        exit_code: status.code().unwrap_or(-1),
        stdout,
        stderr,
        prompts,
        timed_out,
        output_truncated: stdout_exceeded.load(Ordering::SeqCst)
            || stderr_exceeded.load(Ordering::SeqCst),
    })
}

async fn wait_after_termination<F>(
    termination: &ProcessTermination,
    wait: &mut Pin<Box<F>>,
) -> std::process::ExitStatus
where
    F: Future<Output = std::io::Result<std::process::ExitStatus>>,
{
    match timeout(TERMINATION_GRACE_PERIOD, wait.as_mut()).await {
        Ok(Ok(status)) => status,
        _ => {
            let _ = termination.terminate(true);
            timeout(TERMINATION_GRACE_PERIOD, wait.as_mut())
                .await
                .ok()
                .and_then(Result::ok)
                .unwrap_or_else(failed_exit_status)
        }
    }
}

async fn join_pipe(
    mut task: tokio::task::JoinHandle<std::io::Result<Vec<u8>>>,
) -> Result<Vec<u8>, String> {
    match timeout(TERMINATION_GRACE_PERIOD, &mut task).await {
        Ok(Ok(Ok(bytes))) => Ok(bytes),
        Ok(_) => Err(runtime_error()),
        Err(_) => {
            task.abort();
            Err(runtime_error())
        }
    }
}

async fn read_bounded_pipe(
    mut reader: impl AsyncRead + Unpin,
    maximum: usize,
    exceeded: Arc<AtomicBool>,
) -> std::io::Result<Vec<u8>> {
    let mut result = Vec::new();
    let mut buffer = [0_u8; 8 * 1_024];
    loop {
        let count = reader.read(&mut buffer).await?;
        if count == 0 {
            break;
        }
        if result.len().saturating_add(count) > maximum {
            exceeded.store(true, Ordering::SeqCst);
        }
        if result.len() < maximum {
            let retained = count.min(maximum - result.len());
            result.extend_from_slice(&buffer[..retained]);
        }
    }
    Ok(result)
}

fn validate_material(material: &SshConnectionMaterial) -> Result<(), String> {
    material
        .spec
        .validate()
        .map_err(|_| "远端连接信息无效。".to_owned())?;
    validate_authentication(
        material.spec.authentication_type,
        &material.credentials,
        "本机没有保存这条连接的",
    )?;
    for (path, label) in [
        (
            material.credentials.private_key_path.as_deref(),
            "本机保存的 SSH 私钥文件不可用。",
        ),
        (
            material.credentials.certificate_path.as_deref(),
            "本机保存的 SSH 证书文件不可用。",
        ),
        (
            material.credentials.jump_private_key_path.as_deref(),
            "本机保存的跳板机私钥文件不可用。",
        ),
        (
            material.credentials.jump_certificate_path.as_deref(),
            "本机保存的跳板机证书文件不可用。",
        ),
        (
            material
                .jump_credentials
                .as_ref()
                .and_then(|credentials| credentials.private_key_path.as_deref()),
            "本机保存的跳板机私钥文件不可用。",
        ),
        (
            material
                .jump_credentials
                .as_ref()
                .and_then(|credentials| credentials.certificate_path.as_deref()),
            "本机保存的跳板机证书文件不可用。",
        ),
    ] {
        if let Some(path) = path {
            validate_credential_file(Path::new(path), label)?;
        }
    }
    Ok(())
}

fn validate_authentication(
    authentication_type: LocalRemoteAuthenticationType,
    credentials: &RemoteCredentialBundle,
    prefix: &str,
) -> Result<(), String> {
    match authentication_type {
        LocalRemoteAuthenticationType::Password if credentials.password.is_none() => {
            Err(format!("{prefix}登录密码，请重新编辑并保存。"))
        }
        LocalRemoteAuthenticationType::PrivateKey if credentials.private_key_path.is_none() => {
            Err(format!("{prefix}私钥路径，请重新编辑并保存。"))
        }
        LocalRemoteAuthenticationType::PrivateKeyCert if credentials.private_key_path.is_none() => {
            Err(format!("{prefix}私钥路径，请重新编辑并保存。"))
        }
        LocalRemoteAuthenticationType::PrivateKeyCert if credentials.certificate_path.is_none() => {
            Err(format!("{prefix} SSH 证书路径，请重新编辑并保存。"))
        }
        _ => Ok(()),
    }
}

fn validate_credential_file(path: &Path, message: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| message.to_owned())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        Err(message.to_owned())
    } else {
        Ok(())
    }
}

fn ssh_config(material: &SshConnectionMaterial) -> Result<String, String> {
    let mut blocks = Vec::new();
    if material.spec.jump_enabled {
        let jump_authentication_type = material.jump_authentication_type.unwrap_or_else(|| {
            if material.credentials.jump_private_key_path.is_none() {
                LocalRemoteAuthenticationType::Password
            } else if material.credentials.jump_certificate_path.is_none() {
                LocalRemoteAuthenticationType::PrivateKey
            } else {
                LocalRemoteAuthenticationType::PrivateKeyCert
            }
        });
        let jump_credentials = material
            .jump_credentials
            .as_ref()
            .unwrap_or(&material.credentials);
        let jump_host = material
            .spec
            .jump_host
            .as_deref()
            .ok_or_else(|| "跳板机地址不可用。".to_owned())?;
        let jump_username = material
            .spec
            .jump_username
            .as_deref()
            .ok_or_else(|| "跳板机用户名不可用。".to_owned())?;
        let mut jump = common_host_block(
            JUMP_ALIAS,
            jump_host,
            material.spec.jump_port.unwrap_or(22),
            jump_username,
            material.spec.host_key_policy,
            10,
        );
        jump.extend(authentication_lines(
            jump_authentication_type,
            jump_credentials
                .jump_private_key_path
                .as_deref()
                .or(jump_credentials.private_key_path.as_deref()),
            jump_credentials
                .jump_certificate_path
                .as_deref()
                .or(jump_credentials.certificate_path.as_deref()),
        ));
        blocks.push(jump.join("\n"));
    }
    let mut target = common_host_block(
        TARGET_ALIAS,
        &material.spec.host,
        material.spec.port,
        &material.spec.username,
        material.spec.host_key_policy,
        if material.spec.jump_enabled { 300 } else { 10 },
    );
    if material.spec.jump_enabled {
        target.push(format!("  ProxyJump {JUMP_ALIAS}"));
    }
    target.extend(authentication_lines(
        material.spec.authentication_type,
        material.credentials.private_key_path.as_deref(),
        material.credentials.certificate_path.as_deref(),
    ));
    blocks.push(target.join("\n"));
    Ok(blocks.join("\n\n") + "\n")
}

fn common_host_block(
    alias: &str,
    host: &str,
    port: u32,
    username: &str,
    policy: LocalRemoteHostKeyPolicy,
    connect_timeout_seconds: u64,
) -> Vec<String> {
    vec![
        format!("Host {alias}"),
        format!("  HostName {}", ssh_config_value(host)),
        format!("  Port {port}"),
        format!("  User {}", ssh_config_value(username)),
        format!("  ConnectTimeout {}", connect_timeout_seconds.max(1)),
        "  ConnectionAttempts 1".into(),
        "  ServerAliveInterval 5".into(),
        "  ServerAliveCountMax 1".into(),
        format!(
            "  StrictHostKeyChecking {}",
            match policy {
                LocalRemoteHostKeyPolicy::Strict => "yes",
                LocalRemoteHostKeyPolicy::AcceptNew => "accept-new",
            }
        ),
        "  BatchMode no".into(),
        "  RequestTTY no".into(),
        "  LogLevel ERROR".into(),
    ]
}

fn authentication_lines(
    authentication_type: LocalRemoteAuthenticationType,
    private_key_path: Option<&str>,
    certificate_path: Option<&str>,
) -> Vec<String> {
    match authentication_type {
        LocalRemoteAuthenticationType::Password => vec![
            "  PubkeyAuthentication no".into(),
            "  PasswordAuthentication yes".into(),
            "  KbdInteractiveAuthentication yes".into(),
            "  PreferredAuthentications keyboard-interactive,password".into(),
        ],
        LocalRemoteAuthenticationType::PrivateKey
        | LocalRemoteAuthenticationType::PrivateKeyCert => {
            let mut lines = vec![
                "  PubkeyAuthentication yes".into(),
                "  PasswordAuthentication no".into(),
                "  KbdInteractiveAuthentication yes".into(),
                "  PreferredAuthentications publickey,keyboard-interactive".into(),
                "  IdentitiesOnly yes".into(),
            ];
            if let Some(path) = private_key_path {
                lines.push(format!("  IdentityFile {}", ssh_config_value(path)));
            }
            if authentication_type == LocalRemoteAuthenticationType::PrivateKeyCert {
                if let Some(path) = certificate_path {
                    lines.push(format!("  CertificateFile {}", ssh_config_value(path)));
                }
            }
            lines
        }
    }
}

fn ssh_config_value(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn normalize_verification_code(value: Option<String>) -> Result<Option<String>, String> {
    value
        .map(|value| {
            let value = value.trim();
            if value.is_empty() || value.len() > 1_024 || value.contains(['\0', '\r', '\n']) {
                Err("SSH 二次验证码无效。".to_owned())
            } else {
                Ok(value.to_owned())
            }
        })
        .transpose()
}

fn connection_fingerprint(material: &SshConnectionMaterial) -> String {
    let mut digest = Sha256::new();
    digest.update(serde_json::to_vec(&material.spec).unwrap_or_default());
    digest.update([0]);
    digest.update(serde_json::to_vec(&material.credentials).unwrap_or_default());
    digest.update([0]);
    if let Some(authentication_type) = material.jump_authentication_type {
        digest.update(authentication_type.as_str().as_bytes());
    }
    digest.update([0]);
    if let Some(credentials) = &material.jump_credentials {
        digest.update(serde_json::to_vec(credentials).unwrap_or_default());
    }
    format!("{:x}", digest.finalize())
}

fn finish_outcome(outcome: ProcessOutcome) -> Result<PresentedRemoteConnectionTestResult, String> {
    if outcome.timed_out {
        return Err(if outcome.prompts.trim().is_empty() {
            "SSH 连接超时，请检查主机地址、端口和网络。".into()
        } else {
            "已连接到 SSH 服务器，但认证等待超时。请使用最新验证码后重试。".into()
        });
    }
    if outcome.output_truncated {
        return Err("SSH 诊断输出超过安全上限，连接测试已终止。".into());
    }
    if outcome.exit_code == 0 && String::from_utf8_lossy(&outcome.stdout).contains(SUCCESS_MARKER) {
        return Ok(PresentedRemoteConnectionTestResult::success());
    }
    Err(user_facing_failure(&outcome.stderr))
}

fn verification_prompt(prompts: &str, material: &SshConnectionMaterial) -> Option<String> {
    let prompt = verification_prompts(prompts).into_iter().next()?;
    Some(sanitize_verification_prompt(prompt, material))
}

fn verification_prompts(prompts: &str) -> Vec<&str> {
    prompts
        .lines()
        .filter(|prompt| is_verification_prompt(prompt))
        .collect()
}

fn sanitize_verification_prompt(prompt: &str, material: &SshConnectionMaterial) -> String {
    let mut sanitized = prompt
        .chars()
        .filter(|character| !character.is_control())
        .take(MAXIMUM_PRESENTED_PROMPT_CHARACTERS)
        .collect::<String>();
    for secret in all_secrets(material) {
        if !secret.is_empty() {
            sanitized = sanitized.replace(secret, "••••");
        }
    }
    if sanitized.trim().is_empty() {
        "请输入 SSH 二次验证码。".into()
    } else {
        sanitized
    }
}

fn is_verification_prompt(prompt: &str) -> bool {
    let prompt = prompt.to_lowercase();
    [
        "verification",
        "one-time",
        "otp",
        "mfa",
        "2fa",
        "sms",
        "token",
        "code",
        "验证码",
    ]
    .iter()
    .any(|marker| prompt.contains(marker))
}

fn all_secrets(material: &SshConnectionMaterial) -> Vec<&str> {
    let mut secrets = credential_secrets(&material.credentials);
    if let Some(jump) = &material.jump_credentials {
        secrets.extend(credential_secrets(jump));
    }
    secrets
}

fn credential_secrets(credentials: &RemoteCredentialBundle) -> Vec<&str> {
    [
        credentials.password.as_deref(),
        credentials.private_key_path.as_deref(),
        credentials.certificate_path.as_deref(),
        credentials.jump_password.as_deref(),
        credentials.jump_private_key_path.as_deref(),
        credentials.jump_certificate_path.as_deref(),
    ]
    .into_iter()
    .flatten()
    .collect()
}

fn user_facing_failure(stderr: &[u8]) -> String {
    let source = String::from_utf8_lossy(stderr).to_lowercase();
    if source.contains("host key verification failed")
        || source.contains("remote host identification has changed")
    {
        "主机密钥校验失败。请确认服务器密钥，或选择“首次连接时接受新密钥”。".into()
    } else if source.contains("permission denied") {
        "SSH 认证失败，请检查用户名和本机保存的密码或私钥。".into()
    } else if source.contains("connection refused") {
        "远端服务器拒绝连接，请检查 SSH 端口和服务状态。".into()
    } else if source.contains("no route to host")
        || source.contains("operation timed out")
        || source.contains("connection timed out")
    {
        "无法访问远端主机，请检查网络、地址和防火墙。".into()
    } else if source.contains("could not resolve hostname") {
        "无法解析远端主机地址。".into()
    } else if source.contains("identity file") || source.contains("load key") {
        "无法读取本机保存的 SSH 私钥或证书。".into()
    } else {
        "SSH 连接失败，请检查连接信息和本机 OpenSSH 配置。".into()
    }
}

fn inherited_ssh_environment() -> Vec<(OsString, OsString)> {
    const ALLOWED: &[&str] = &[
        "SystemRoot",
        "WINDIR",
        "USERPROFILE",
        "HOMEDRIVE",
        "HOMEPATH",
        "HOME",
        "TEMP",
        "TMP",
        "PATH",
        "COMSPEC",
        "PATHEXT",
        "SSH_AUTH_SOCK",
    ];
    let mut environment = ALLOWED
        .iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect::<Vec<_>>();
    environment.push((OsString::from("LC_ALL"), OsString::from("C")));
    environment
}

#[cfg(windows)]
fn restrict_open_ssh_search_environment(
    environment: &mut Vec<(OsString, OsString)>,
    executable: &Path,
) -> Result<(), String> {
    let open_ssh_directory = executable
        .parent()
        .ok_or_else(|| "Windows OpenSSH 不可用。".to_owned())?;
    let system_directory = open_ssh_directory
        .parent()
        .ok_or_else(|| "Windows OpenSSH 不可用。".to_owned())?;
    let windows_directory = system_directory
        .parent()
        .ok_or_else(|| "Windows OpenSSH 不可用。".to_owned())?;
    let command_processor = system_directory.join("cmd.exe");
    validate_regular_executable(&command_processor, "Windows 命令处理器不可用。")?;
    environment.retain(|(key, _)| {
        !["PATH", "SystemRoot", "WINDIR", "COMSPEC"]
            .iter()
            .any(|reserved| key.to_string_lossy().eq_ignore_ascii_case(reserved))
    });
    environment.extend([
        (
            OsString::from("PATH"),
            open_ssh_directory.as_os_str().to_owned(),
        ),
        (
            OsString::from("SystemRoot"),
            windows_directory.as_os_str().to_owned(),
        ),
        (
            OsString::from("WINDIR"),
            windows_directory.as_os_str().to_owned(),
        ),
        (
            OsString::from("COMSPEC"),
            command_processor.as_os_str().to_owned(),
        ),
    ]);
    Ok(())
}

#[cfg(not(windows))]
fn restrict_open_ssh_search_environment(
    _environment: &mut Vec<(OsString, OsString)>,
    _executable: &Path,
) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
fn ssh_executable() -> Result<PathBuf, String> {
    use std::os::windows::ffi::OsStringExt;
    use windows_sys::Win32::System::SystemInformation::GetWindowsDirectoryW;
    let mut buffer = vec![0_u16; 32_768];
    let length = unsafe { GetWindowsDirectoryW(buffer.as_mut_ptr(), buffer.len() as u32) };
    if length == 0 || length as usize >= buffer.len() {
        return Err("Windows OpenSSH 不可用。".into());
    }
    let windows = PathBuf::from(OsString::from_wide(&buffer[..length as usize]));
    let executable = windows.join("System32").join("OpenSSH").join("ssh.exe");
    validate_regular_executable(&executable, "Windows OpenSSH 不可用。")?;
    Ok(executable)
}

#[cfg(not(windows))]
fn ssh_executable() -> Result<PathBuf, String> {
    let executable = PathBuf::from("/usr/bin/ssh");
    validate_regular_executable(&executable, "本机 OpenSSH 不可用。")?;
    Ok(executable)
}

fn validate_regular_executable(path: &Path, message: &str) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| message.to_owned())?;
    if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        Err(message.to_owned())
    } else {
        Ok(())
    }
}

fn reject_reparse_point(path: &Path) -> Result<(), String> {
    let metadata = fs::symlink_metadata(path).map_err(|_| runtime_error())?;
    if metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
        Err(runtime_error())
    } else {
        Ok(())
    }
}

#[cfg(windows)]
fn is_reparse_point(metadata: &fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
    metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT != 0
}

#[cfg(not(windows))]
fn is_reparse_point(_metadata: &fs::Metadata) -> bool {
    false
}

fn set_private_directory_permissions(path: &Path) -> Result<(), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(path, fs::Permissions::from_mode(0o700))
            .map_err(|_| runtime_error())?;
    }
    Ok(())
}

fn write_private_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut options = OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path).map_err(|_| runtime_error())?;
    file.write_all(bytes).map_err(|_| runtime_error())?;
    file.sync_all().map_err(|_| runtime_error())
}

fn read_bounded_text(path: &Path, maximum: usize) -> String {
    let Ok(mut file) = File::open(path) else {
        return String::new();
    };
    let Ok(metadata) = file.metadata() else {
        return String::new();
    };
    if !metadata.is_file() {
        return String::new();
    }
    let mut bytes = Vec::new();
    if metadata.len() > maximum as u64 {
        use std::io::Seek;
        if file
            .seek(std::io::SeekFrom::End(-(maximum as i64)))
            .is_err()
        {
            return String::new();
        }
    }
    if file.take(maximum as u64).read_to_end(&mut bytes).is_err() {
        return String::new();
    }
    String::from_utf8_lossy(&bytes).into_owned()
}

#[cfg(unix)]
fn failed_exit_status() -> std::process::ExitStatus {
    use std::os::unix::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(1 << 8)
}

#[cfg(windows)]
fn failed_exit_status() -> std::process::ExitStatus {
    use std::os::windows::process::ExitStatusExt;
    std::process::ExitStatus::from_raw(1)
}

fn runtime_error() -> String {
    "无法准备安全的 SSH 临时运行目录。".into()
}

pub fn run_askpass_helper_if_requested() -> bool {
    if std::env::var_os(ASKPASS_MODE_VARIABLE).as_deref() != Some(OsStr::new("1")) {
        return false;
    }
    let mut arguments = std::env::args_os();
    let _executable = arguments.next();
    let prompt = arguments
        .next()
        .and_then(|value| value.into_string().ok())
        .unwrap_or_default();
    append_prompt_log(&prompt);
    let target_password = std::env::var("CHATOS_SSH_TARGET_PASSWORD").unwrap_or_default();
    let jump_password = std::env::var("CHATOS_SSH_JUMP_PASSWORD").unwrap_or_default();
    let jump_host = std::env::var("CHATOS_SSH_JUMP_HOST").unwrap_or_default();
    let jump_user = std::env::var("CHATOS_SSH_JUMP_USER").unwrap_or_default();
    let verification_file = std::env::var_os("CHATOS_SSH_VERIFICATION_FILE").map(PathBuf::from);
    let response = askpass_response(
        &prompt,
        &target_password,
        &jump_password,
        &jump_host,
        &jump_user,
        "",
        verification_file.as_deref(),
    );
    let _ = writeln!(std::io::stdout(), "{response}");
    true
}

fn append_prompt_log(prompt: &str) {
    let Some(path) = std::env::var_os("CHATOS_SSH_PROMPT_LOG") else {
        return;
    };
    let path = PathBuf::from(path);
    let Ok(metadata) = fs::symlink_metadata(&path) else {
        return;
    };
    if !metadata.is_file()
        || metadata.file_type().is_symlink()
        || is_reparse_point(&metadata)
        || metadata.len() >= MAXIMUM_PROMPT_BYTES as u64
    {
        return;
    }
    let sanitized = prompt
        .chars()
        .map(|character| {
            if character.is_control() {
                ' '
            } else {
                character
            }
        })
        .take(4_096)
        .collect::<String>();
    if let Ok(mut file) = OpenOptions::new().append(true).open(path) {
        let _ = writeln!(file, "{sanitized}");
    }
}

fn askpass_response(
    prompt: &str,
    target_password: &str,
    jump_password: &str,
    jump_host: &str,
    jump_user: &str,
    verification_code: &str,
    verification_file: Option<&Path>,
) -> String {
    if is_verification_prompt(prompt) {
        return if verification_code.is_empty() {
            verification_file
                .and_then(wait_for_verification_code)
                .unwrap_or_default()
        } else {
            verification_code.to_owned()
        };
    }
    if (!jump_host.is_empty() && prompt.contains(jump_host))
        || (!jump_user.is_empty() && prompt.contains(jump_user))
    {
        jump_password.to_owned()
    } else {
        target_password.to_owned()
    }
}

fn wait_for_verification_code(path: &Path) -> Option<String> {
    for _ in 0..1_200 {
        let metadata = fs::symlink_metadata(path).ok()?;
        if !metadata.is_file() || metadata.file_type().is_symlink() || is_reparse_point(&metadata) {
            return None;
        }
        if metadata.len() > 0 && metadata.len() <= 1_024 {
            let mut code = String::new();
            File::open(path)
                .ok()?
                .take(1_025)
                .read_to_string(&mut code)
                .ok()?;
            let code = code.trim();
            if !code.is_empty() && code.len() <= 1_024 && !code.contains(['\0', '\r', '\n']) {
                let _ = OpenOptions::new().write(true).truncate(true).open(path);
                return Some(code.to_owned());
            }
            return None;
        }
        std::thread::sleep(Duration::from_millis(250));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn material(policy: LocalRemoteHostKeyPolicy) -> SshConnectionMaterial {
        SshConnectionMaterial {
            spec: LocalRemoteConnectionSpec {
                name: Some("Production".into()),
                host: "server.example.com".into(),
                port: 22,
                username: "deploy".into(),
                authentication_type: LocalRemoteAuthenticationType::PrivateKeyCert,
                default_remote_path: Some("/srv/app".into()),
                host_key_policy: policy,
                local_connector_device_id: "device".into(),
                local_connector_workspace_id: "workspace".into(),
                jump_enabled: true,
                jump_connection_id: Some("jump-1".into()),
                jump_host: Some("jump.example.com".into()),
                jump_port: Some(2202),
                jump_username: Some("jumper".into()),
            },
            credentials: RemoteCredentialBundle {
                private_key_path: Some("C:\\Keys\\id_ed25519".into()),
                certificate_path: Some("C:\\Keys\\id_ed25519-cert.pub".into()),
                ..RemoteCredentialBundle::default()
            },
            jump_authentication_type: Some(LocalRemoteAuthenticationType::Password),
            jump_credentials: Some(RemoteCredentialBundle {
                password: Some("jump-secret".into()),
                ..RemoteCredentialBundle::default()
            }),
        }
    }

    fn password_material() -> SshConnectionMaterial {
        let mut material = material(LocalRemoteHostKeyPolicy::Strict);
        material.spec.authentication_type = LocalRemoteAuthenticationType::Password;
        material.spec.jump_enabled = false;
        material.spec.jump_connection_id = None;
        material.spec.jump_host = None;
        material.spec.jump_port = None;
        material.spec.jump_username = None;
        material.credentials = RemoteCredentialBundle {
            password: Some("target-secret".into()),
            ..RemoteCredentialBundle::default()
        };
        material.jump_authentication_type = None;
        material.jump_credentials = None;
        material
    }

    #[test]
    fn config_matches_macos_host_key_authentication_and_proxy_jump_semantics() {
        let strict = ssh_config(&material(LocalRemoteHostKeyPolicy::Strict)).unwrap();
        assert!(strict.contains("Host chatos-jump"));
        assert!(strict.contains("ProxyJump chatos-jump"));
        assert!(strict.contains("StrictHostKeyChecking yes"));
        assert!(strict.contains("Port 2202"));
        assert!(strict.contains("PreferredAuthentications keyboard-interactive,password"));
        assert!(strict.contains("IdentityFile \"C:\\\\Keys\\\\id_ed25519\""));
        assert!(strict.contains("CertificateFile \"C:\\\\Keys\\\\id_ed25519-cert.pub\""));
        assert!(!strict.contains("jump-secret"));

        let accept_new = ssh_config(&material(LocalRemoteHostKeyPolicy::AcceptNew)).unwrap();
        assert!(accept_new.contains("StrictHostKeyChecking accept-new"));
    }

    #[test]
    fn askpass_routes_target_jump_and_verification_without_logging_secrets() {
        assert_eq!(
            askpass_response(
                "deploy@server password:",
                "target-secret",
                "jump-secret",
                "jump.example.com",
                "jumper",
                "123456",
                None,
            ),
            "target-secret"
        );
        assert_eq!(
            askpass_response(
                "jumper@jump.example.com password:",
                "target-secret",
                "jump-secret",
                "jump.example.com",
                "jumper",
                "123456",
                None,
            ),
            "jump-secret"
        );
        assert_eq!(
            askpass_response(
                "Verification code:",
                "target-secret",
                "jump-secret",
                "jump.example.com",
                "jumper",
                "123456",
                None,
            ),
            "123456"
        );
    }

    #[test]
    fn askpass_waits_for_a_code_submitted_to_the_same_verification_file() {
        let directory = tempfile::tempdir().unwrap();
        let response = directory.path().join("verification-response");
        write_private_file(&response, &[]).unwrap();
        let waiting_path = response.clone();
        let waiter = std::thread::spawn(move || {
            askpass_response(
                "MFA code:",
                "target-secret",
                "jump-secret",
                "",
                "",
                "",
                Some(&waiting_path),
            )
        });
        std::thread::sleep(Duration::from_millis(50));
        let mut file = OpenOptions::new()
            .write(true)
            .truncate(true)
            .open(&response)
            .unwrap();
        file.write_all(b"654321").unwrap();
        file.sync_all().unwrap();
        assert_eq!(waiter.join().unwrap(), "654321");
        assert_eq!(fs::metadata(response).unwrap().len(), 0);
    }

    #[tokio::test]
    async fn submitted_mfa_code_resumes_the_retained_session_instead_of_reconnecting() {
        let directory = tempfile::tempdir().unwrap();
        let tester = NativeOpenSshTester::with_timeout(
            directory.path().join("runtime"),
            Duration::from_secs(1),
        );
        let connection = password_material();
        let fingerprint = connection_fingerprint(&connection);
        let prepared =
            Arc::new(PreparedSshSession::create(&tester.runtime_root, &connection).unwrap());
        fs::write(&prepared.prompt_log, b"MFA verification code:\n").unwrap();
        let (cancel, _cancellation) = watch::channel(false);
        let session = Arc::new(ActiveSshSession {
            connection: connection.clone(),
            prepared: prepared.clone(),
            cancel,
            completion: Mutex::new(None),
            changed: Notify::new(),
        });
        tester
            .sessions
            .lock()
            .await
            .insert(fingerprint, session.clone());
        let completion_session = session.clone();
        tokio::spawn(async move {
            loop {
                let response =
                    fs::read_to_string(&prepared.verification_response).unwrap_or_default();
                if response == "654321" {
                    *completion_session.completion.lock().await = Some(Ok(ProcessOutcome {
                        exit_code: 0,
                        stdout: format!("{SUCCESS_MARKER} host").into_bytes(),
                        stderr: Vec::new(),
                        prompts: "MFA verification code:\n".into(),
                        timed_out: false,
                        output_truncated: false,
                    }));
                    completion_session.changed.notify_waiters();
                    return;
                }
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
        });

        let result = tester
            .run_test(connection, Some("654321".into()))
            .await
            .unwrap();

        assert!(result.success);
        assert!(tester.sessions.lock().await.is_empty());
    }

    #[tokio::test]
    async fn submitted_mfa_code_never_starts_a_replacement_session() {
        let directory = tempfile::tempdir().unwrap();
        let tester = NativeOpenSshTester::with_timeout(
            directory.path().join("runtime"),
            Duration::from_secs(1),
        );

        let error = tester
            .run_test(password_material(), Some("654321".into()))
            .await
            .unwrap_err();

        assert_eq!(error, "SSH 二次验证会话已过期，请重新执行连接测试。");
        assert!(tester.sessions.lock().await.is_empty());
        assert!(!tester.runtime_root.exists());
    }

    #[tokio::test]
    async fn expiration_timer_does_not_retain_a_completed_session() {
        let directory = tempfile::tempdir().unwrap();
        let tester = NativeOpenSshTester::with_timeout(
            directory.path().join("runtime"),
            Duration::from_secs(1),
        );
        let connection = password_material();
        let prepared =
            Arc::new(PreparedSshSession::create(&tester.runtime_root, &connection).unwrap());
        let session_path = prepared.directory.clone();
        let (cancel, _cancellation) = watch::channel(false);
        let session = Arc::new(ActiveSshSession {
            connection,
            prepared,
            cancel,
            completion: Mutex::new(None),
            changed: Notify::new(),
        });

        tester.schedule_expiration("completed".into(), session.clone());
        drop(session);
        tokio::task::yield_now().await;

        assert!(!session_path.exists());
    }

    #[tokio::test]
    async fn reset_cancels_and_removes_every_retained_mfa_session() {
        let directory = tempfile::tempdir().unwrap();
        let tester = NativeOpenSshTester::with_timeout(
            directory.path().join("runtime"),
            Duration::from_secs(1),
        );
        let connection = password_material();
        let prepared =
            Arc::new(PreparedSshSession::create(&tester.runtime_root, &connection).unwrap());
        let (cancel, cancellation) = watch::channel(false);
        let session = Arc::new(ActiveSshSession {
            connection,
            prepared,
            cancel,
            completion: Mutex::new(Some(Err("cancelled".into()))),
            changed: Notify::new(),
        });
        tester
            .sessions
            .lock()
            .await
            .insert("session".into(), session);

        tester.reset_sessions().await;

        assert!(tester.sessions.lock().await.is_empty());
        assert!(*cancellation.borrow());
    }

    #[test]
    fn verification_prompt_is_bounded_and_redacts_every_local_secret() {
        let connection = material(LocalRemoteHostKeyPolicy::Strict);
        let prompt = verification_prompt(
            &format!(
                "MFA code for jump-secret at {}",
                "x".repeat(MAXIMUM_PRESENTED_PROMPT_CHARACTERS * 2)
            ),
            &connection,
        )
        .unwrap();
        assert!(!prompt.contains("jump-secret"));
        assert!(prompt.chars().count() <= MAXIMUM_PRESENTED_PROMPT_CHARACTERS);
    }

    #[test]
    fn diagnostics_are_classified_without_returning_raw_stderr_or_secrets() {
        let raw = b"Permission denied for password super-secret at C:\\Users\\alice";
        let message = user_facing_failure(raw);
        assert_eq!(
            message,
            "SSH 认证失败，请检查用户名和本机保存的密码或私钥。"
        );
        assert!(!message.contains("super-secret"));
        assert!(!message.contains("C:\\Users"));
    }

    #[cfg(unix)]
    #[tokio::test]
    async fn timeout_terminates_the_entire_process_group() {
        let directory = tempfile::tempdir().unwrap();
        let prompt_log = directory.path().join("prompts.log");
        write_private_file(&prompt_log, &[]).unwrap();
        let marker = directory.path().join("escaped-child");
        let script = format!(
            "(sleep 1; touch {}) & wait",
            shell_quote(&marker.to_string_lossy())
        );
        let (_cancel, cancellation) = watch::channel(false);
        let outcome = run_isolated_process(
            Path::new("/bin/zsh"),
            &[OsString::from("-lc"), OsString::from(script)],
            &inherited_ssh_environment(),
            &prompt_log,
            cancellation,
            Duration::from_millis(50),
        )
        .await
        .unwrap();
        assert!(outcome.timed_out);
        tokio::time::sleep(Duration::from_millis(1_200)).await;
        assert!(!marker.exists());
    }

    #[test]
    fn prepared_session_is_private_and_removed_on_drop() {
        let directory = tempfile::tempdir().unwrap();
        let runtime = directory.path().join("runtime");
        let session =
            PreparedSshSession::create(&runtime, &material(LocalRemoteHostKeyPolicy::Strict))
                .unwrap();
        let session_path = session.directory.clone();
        let config = fs::read_to_string(&session.config).unwrap();
        assert!(!config.contains("jump-secret"));
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&session.directory)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o700
            );
            assert_eq!(
                fs::metadata(&session.config).unwrap().permissions().mode() & 0o777,
                0o600
            );
        }
        drop(session);
        assert!(!session_path.exists());
    }

    fn shell_quote(value: &str) -> String {
        format!("'{}'", value.replace('\'', "'\\''"))
    }

    #[test]
    fn tester_constructor_keeps_runtime_under_the_supplied_app_data_directory() {
        let tester = NativeOpenSshTester::with_timeout(
            PathBuf::from("/app-data/remote-runtime"),
            Duration::from_secs(1),
        );
        assert_eq!(
            tester.runtime_root,
            PathBuf::from("/app-data/remote-runtime")
        );
    }
}
