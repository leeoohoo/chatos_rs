// SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
// Required Notice: Copyright (c) 2025 AI Chat Team

#[cfg(target_os = "linux")]
use super::*;

#[cfg(target_os = "linux")]
pub(super) async fn bind_unix_listener(path: &Path) -> Result<UnixListener, String> {
    if path.exists() {
        std::fs::remove_file(path).map_err(|err| err.to_string())?;
    }
    UnixListener::bind(path)
        .map_err(|err| format!("bind proxy bridge {} failed: {err}", path.display()))
}

#[cfg(target_os = "linux")]
pub(super) async fn run_host_bridge(listener: UnixListener, endpoint: SocketAddr) {
    loop {
        let Ok((mut unix, _)) = listener.accept().await else {
            return;
        };
        tokio::spawn(async move {
            let Ok(mut tcp) = TcpStream::connect(endpoint).await else {
                return;
            };
            let _ = tokio::io::copy_bidirectional(&mut unix, &mut tcp).await;
        });
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn is_internal_network_proxy_wrapper() -> bool {
    std::env::args().nth(1).as_deref() == Some("--internal-network-proxy-wrapper")
}

#[cfg(target_os = "linux")]
pub(crate) fn is_internal_command_wrapper() -> bool {
    std::env::args().nth(1).as_deref() == Some("--internal-command-wrapper")
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn is_internal_command_wrapper() -> bool {
    false
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn is_internal_network_proxy_wrapper() -> bool {
    false
}

#[cfg(target_os = "linux")]
pub(crate) async fn run_internal_network_proxy_wrapper() -> Result<(), String> {
    let spec = LinuxProxyWrapperSpec::from_args(std::env::args().skip(2).collect())?;
    ensure_loopback_interface_up()
        .map_err(|err| format!("enable sandbox loopback failed: {err}"))?;

    let http_listener = TcpListener::bind((Ipv4Addr::LOCALHOST, spec.http_port))
        .await
        .map_err(|err| format!("bind sandbox HTTP proxy bridge failed: {err}"))?;
    let mut tasks = vec![tokio::spawn(run_local_bridge(
        http_listener,
        spec.http_socket,
    ))];
    if let (Some(port), Some(socket)) = (spec.socks_port, spec.socks_socket) {
        let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
            .await
            .map_err(|err| format!("bind sandbox SOCKS5 proxy bridge failed: {err}"))?;
        tasks.push(tokio::spawn(run_local_bridge(listener, socket)));
    }

    let status = run_seccomp_wrapped_command(spec.command).await?;
    for task in tasks {
        task.abort();
    }
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(target_os = "linux")]
pub(crate) async fn run_internal_command_wrapper() -> Result<(), String> {
    let args = std::env::args().skip(2).collect::<Vec<_>>();
    let command = args
        .strip_prefix(&["--".to_string()])
        .ok_or_else(|| "command wrapper is missing command separator".to_string())?
        .to_vec();
    let status = run_seccomp_wrapped_command(command).await?;
    std::process::exit(status.code().unwrap_or(1));
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn run_internal_command_wrapper() -> Result<(), String> {
    Err("command wrapper is only available on Linux".to_string())
}

#[cfg(target_os = "linux")]
pub(super) async fn run_seccomp_wrapped_command(
    command_parts: Vec<String>,
) -> Result<std::process::ExitStatus, String> {
    use std::os::unix::process::CommandExt;

    let executable = command_parts
        .first()
        .ok_or_else(|| "sandbox command wrapper is missing a command".to_string())?;
    let mut command = tokio::process::Command::new(executable);
    command.args(&command_parts[1..]);
    unsafe {
        command
            .as_std_mut()
            .pre_exec(install_no_unix_socket_seccomp);
    }
    command.status().await.map_err(|err| err.to_string())
}

#[cfg(target_os = "linux")]
pub(super) fn install_no_unix_socket_seccomp() -> std::io::Result<()> {
    const BPF_LD_W_ABS: u16 = 0x20;
    const BPF_JMP_JEQ_K: u16 = 0x15;
    const BPF_RET_K: u16 = 0x06;
    const SECCOMP_RET_KILL_PROCESS: u32 = 0x8000_0000;
    const SECCOMP_RET_ERRNO: u32 = 0x0005_0000;
    const SECCOMP_RET_ALLOW: u32 = 0x7fff_0000;
    const SECCOMP_SET_MODE_FILTER: libc::c_uint = 1;
    const SECCOMP_DATA_ARCH_OFFSET: u32 = 4;
    const SECCOMP_DATA_NR_OFFSET: u32 = 0;
    const SECCOMP_DATA_ARG0_OFFSET: u32 = 16;

    #[cfg(target_arch = "x86_64")]
    const AUDIT_ARCH: u32 = 0xc000_003e;
    #[cfg(target_arch = "aarch64")]
    const AUDIT_ARCH: u32 = 0xc000_00b7;
    #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
    return Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "seccomp Unix-socket filter is unsupported on this Linux architecture",
    ));

    #[cfg(any(target_arch = "x86_64", target_arch = "aarch64"))]
    {
        let mut filters = [
            libc::sock_filter {
                code: BPF_LD_W_ABS,
                jt: 0,
                jf: 0,
                k: SECCOMP_DATA_ARCH_OFFSET,
            },
            libc::sock_filter {
                code: BPF_JMP_JEQ_K,
                jt: 1,
                jf: 0,
                k: AUDIT_ARCH,
            },
            libc::sock_filter {
                code: BPF_RET_K,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_KILL_PROCESS,
            },
            libc::sock_filter {
                code: BPF_LD_W_ABS,
                jt: 0,
                jf: 0,
                k: SECCOMP_DATA_NR_OFFSET,
            },
            libc::sock_filter {
                code: BPF_JMP_JEQ_K,
                jt: 0,
                jf: 3,
                k: libc::SYS_socket as u32,
            },
            libc::sock_filter {
                code: BPF_LD_W_ABS,
                jt: 0,
                jf: 0,
                k: SECCOMP_DATA_ARG0_OFFSET,
            },
            libc::sock_filter {
                code: BPF_JMP_JEQ_K,
                jt: 0,
                jf: 1,
                k: libc::AF_UNIX as u32,
            },
            libc::sock_filter {
                code: BPF_RET_K,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_ERRNO | libc::EACCES as u32,
            },
            libc::sock_filter {
                code: BPF_RET_K,
                jt: 0,
                jf: 0,
                k: SECCOMP_RET_ALLOW,
            },
        ];
        let program = libc::sock_fprog {
            len: filters.len() as u16,
            filter: filters.as_mut_ptr(),
        };
        if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
            return Err(std::io::Error::last_os_error());
        }
        if unsafe {
            libc::syscall(
                libc::SYS_seccomp,
                SECCOMP_SET_MODE_FILTER,
                0,
                &program as *const libc::sock_fprog,
            )
        } != 0
        {
            return Err(std::io::Error::last_os_error());
        }
        Ok(())
    }
}

#[cfg(not(target_os = "linux"))]
pub(crate) async fn run_internal_network_proxy_wrapper() -> Result<(), String> {
    Err("network proxy wrapper is only available on Linux".to_string())
}

#[cfg(target_os = "linux")]
pub(super) struct LinuxProxyWrapperSpec {
    http_port: u16,
    http_socket: PathBuf,
    socks_port: Option<u16>,
    socks_socket: Option<PathBuf>,
    command: Vec<String>,
}

#[cfg(target_os = "linux")]
impl LinuxProxyWrapperSpec {
    pub(super) fn from_args(args: Vec<String>) -> Result<Self, String> {
        let separator = args
            .iter()
            .position(|arg| arg == "--")
            .ok_or_else(|| "network proxy wrapper is missing command separator".to_string())?;
        let options = &args[..separator];
        let command = args[separator + 1..].to_vec();
        if command.is_empty() {
            return Err("network proxy wrapper is missing a command".to_string());
        }
        let values = options
            .chunks_exact(2)
            .map(|pair| (pair[0].as_str(), pair[1].clone()))
            .collect::<BTreeMap<_, _>>();
        if !options.len().is_multiple_of(2) {
            return Err("network proxy wrapper options must be key/value pairs".to_string());
        }
        let http_port = values
            .get("--http-port")
            .ok_or_else(|| "network proxy wrapper is missing HTTP port".to_string())?
            .parse()
            .map_err(|_| "network proxy wrapper HTTP port is invalid".to_string())?;
        let http_socket = PathBuf::from(
            values
                .get("--http-socket")
                .ok_or_else(|| "network proxy wrapper is missing HTTP socket".to_string())?,
        );
        let socks_port = values
            .get("--socks-port")
            .map(|value| value.parse())
            .transpose()
            .map_err(|_| "network proxy wrapper SOCKS port is invalid".to_string())?;
        let socks_socket = values.get("--socks-socket").map(PathBuf::from);
        if socks_port.is_some() != socks_socket.is_some() {
            return Err("network proxy wrapper SOCKS route is incomplete".to_string());
        }
        Ok(Self {
            http_port,
            http_socket,
            socks_port,
            socks_socket,
            command,
        })
    }
}

#[cfg(target_os = "linux")]
pub(super) async fn run_local_bridge(listener: TcpListener, socket: PathBuf) {
    loop {
        let Ok((mut tcp, _)) = listener.accept().await else {
            return;
        };
        let socket = socket.clone();
        tokio::spawn(async move {
            let Ok(mut unix) = UnixStream::connect(socket).await else {
                return;
            };
            let _ = tokio::io::copy_bidirectional(&mut tcp, &mut unix).await;
        });
    }
}

#[cfg(target_os = "linux")]
pub(super) fn ensure_loopback_interface_up() -> std::io::Result<()> {
    const LOOPBACK_INTERFACE_NAME: &[u8] = b"lo";
    let fd = unsafe { libc::socket(libc::AF_INET, libc::SOCK_DGRAM | libc::SOCK_CLOEXEC, 0) };
    if fd < 0 {
        return Err(std::io::Error::last_os_error());
    }
    let mut ifreq = unsafe { std::mem::zeroed::<libc::ifreq>() };
    for (index, byte) in LOOPBACK_INTERFACE_NAME.iter().copied().enumerate() {
        ifreq.ifr_name[index] = byte as libc::c_char;
    }
    let result = unsafe { libc::ioctl(fd, libc::SIOCGIFFLAGS as libc::Ioctl, &mut ifreq) };
    if result < 0 {
        let err = std::io::Error::last_os_error();
        unsafe { libc::close(fd) };
        return Err(err);
    }
    let flags = unsafe { ifreq.ifr_ifru.ifru_flags };
    if flags & libc::IFF_UP as libc::c_short == 0 {
        ifreq.ifr_ifru.ifru_flags = flags | libc::IFF_UP as libc::c_short;
        let result = unsafe { libc::ioctl(fd, libc::SIOCSIFFLAGS as libc::Ioctl, &ifreq) };
        if result < 0 {
            let err = std::io::Error::last_os_error();
            unsafe { libc::close(fd) };
            return Err(err);
        }
    }
    unsafe { libc::close(fd) };
    Ok(())
}
