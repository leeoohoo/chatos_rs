# Windows 受控域名网络架构

## 决策

`Controlled` 不能通过 `HTTP_PROXY`、环境变量、应用内约定或仅解析域名为 IP 来实现。这些方式可以被任意子进程绕过，也无法阻止直连 IP、QUIC、DoH、同 IP 虚拟主机和子进程逃逸。

ChatOS Windows 选择独立的 `ChatOS.NetworkGuard` 特权组件：

1. 签名的 WFP callout driver 在 ALE connect/redirect 层按 AppContainer SID 匹配流量。
2. Windows 服务管理 WFP provider、sublayer、filters、策略版本和本机 broker。
3. Controlled 进程不获得 AppContainer `internetClient` / `privateNetworkClientServer` capability；只有活动 WFP lease 才能显式放行重定向后的 broker 通道，因此驱动缺失或重启时系统能力层本身仍然断网。
4. TCP 80/443 被透明重定向到本机 broker；broker 只转发策略允许的 HTTP Host 或 TLS ClientHello SNI。
5. DNS 只允许进入 NetworkGuard resolver；直接 UDP/TCP 53、外部 DoH、QUIC/UDP 443和无可见域名的连接默认拒绝。
6. IP literal 默认拒绝。确需 IP/CIDR 时必须使用独立的显式策略类型，不能伪装成域名规则。
7. NetworkGuard 服务、驱动、策略签名、可信 Windows SID、托管域名策略或版本协商任一项不可用时 fail closed：客户端拒绝启动 Controlled 命令，不静默降级为更宽或更窄的另一模式；Host/Disabled 请求不因缺少 Controlled 策略而失败。

第一版 Controlled 只承诺 HTTP/HTTPS。SSH、数据库协议、任意 TCP/UDP 和需要 ECH 且无可见 SNI 的站点必须使用 `Host` 模式或后续专用协议代理，不能静默放宽。

## 为什么不能只使用动态 IP 白名单

- 多个域名可以共享同一 CDN IP，允许 IP 会同时允许未授权域名。
- DNS TTL、重绑定、IPv6 和代理链会让静态地址快速失效。
- 子进程可以跳过系统代理并直接创建 socket。
- 仅按可执行文件过滤不能覆盖脚本解释器和完整子进程树。

因此，WFP 必须负责不可绕过的网络边界，broker 必须负责域名语义；两者缺一不可。

## 组件边界

### Desktop / Connector

- 保存服务端下发并签名的域名策略，不持有驱动管理权限。
- 只通过 ACL 收紧的 named pipe 与 NetworkGuard 服务协商。
- 启动命令前提交完整签名策略、每次执行唯一的 `appcontainer_sid` 和真实 suspended PID。
- 收到服务返回的 active lease 后才恢复挂起进程。
- lease 创建失败、过期或服务断线时终止对应 Job Object。

### NetworkGuard Windows 服务

- 以 `LocalSystem` 运行，通过 ACL 收紧的 pipe 和 SYSTEM/Administrators 驱动设备边界管理 ChatOS 自有 WFP provider/sublayer；不向普通客户端暴露 IOCTL。
- 从 pipe 的真实客户端进程读取 Windows SID，并复验目标 PID 的用户 SID 与 AppContainer SID；同时验证策略签名、revision 和 lease 生命周期。
- 维护 AppContainer SID 与 lease 的精确绑定，不使用进程名或模糊路径。
- 服务异常停止时旧 broker PID 不再可达，且 Controlled 进程没有原生网络 capability；服务重新启动前执行 driver reset，active lease residue 不为零则拒绝开放 broker。
- 审计只记录 policy revision、SID hash、目标 host hash、动作和失败类型，不记录 Token、Secret、URL query 或请求正文。

### WFP callout driver

- 仅处理 ChatOS NetworkGuard sublayer，不拦截其他应用。
- 覆盖 IPv4/IPv6、父子进程和 TCP connect redirect。
- 默认阻止匹配 SID 的全部出站流量；仅允许与 resolver/broker 建立的受控通道。
- 驱动包需要正式代码签名、版本回滚策略和独立安装/升级流程，不能由普通 MSIX 静默安装。

### HTTP/TLS broker

- HTTP 校验规范化后的 Host；拒绝 userinfo、空 Host、非法 IDN、通配符越界和非允许端口。
- TLS 只解析 ClientHello，不进行 TLS 中间人解密；按规范化 SNI 决策后原样转发。
- 无 SNI、ECH 隐藏域名、SNI/目标策略不一致或超出握手大小限制时拒绝。
- 连接目标必须来自 NetworkGuard resolver 对同一 host/revision 的结果，防止进程自带 DNS 结果。

## 策略模型

当前正式签名策略模型为：

```text
policy_revision
windows_user_sid       signed binding to the local Windows account
allowed_hosts[]       exact host or constrained *.example.com
allowed_ports[]       default 80,443
expires_at
owner_user_id
device_id
workspace_id
signature_key_id
signature
```

通配符只匹配一个或明确配置的子域深度；`*.example.com` 不包含 `example.com`。域名在签名前使用 IDNA ASCII、去尾点和小写规范化。策略不得包含 URL path、query 或 Secret。

策略申请本身不包含 `windows_user_sid`、`allowed_hosts` 或调用方签名。Windows Connector 在建立设备 WebSocket 时使用设备 Ed25519 私钥签署 v2 connection payload，其中包含当前 `WindowsIdentity` SID；Local Connector 只允许同一 device 首次绑定或重复上报相同 SID，SID 变化必须重新配对。ChatOS 后端对 terminal exec/session 只发送空的 Controlled 策略申请。

Local Connector 按 global → role → user 顺序合并托管 requirements，选择默认 permission profile，并只提取 `network.enabled = true` 下显式 `allow` 的域名。没有默认 profile、没有 allow 域名、设备尚未可信注册 SID，或配置包含 WFP allowlist 无法精确表达的 deny exception 时，不签发策略。第一版远端端口固定为 80/443。

示例托管策略：

```toml
default_permissions = "windows-controlled"

[allowed_permission_profiles]
"windows-controlled" = true

[permissions.windows-controlled]
extends = ":workspace"

[permissions.windows-controlled.network]
enabled = true
mode = "full"

[permissions.windows-controlled.network.domains]
"api.example.com" = "allow"
"*.example.org" = "allow"
```

## 安装与版本门禁

Controlled 选项只有同时满足以下条件才可见：

1. NetworkGuard 服务和 WFP driver 均已安装且签名有效；生产安装要求 Microsoft Hardware Dev Center 返回的签名，不能把本机受信任测试证书当作正式签名。
2. Desktop、服务、驱动协议 major version 一致。
3. WFP provider/sublayer 归属和 ACL 校验通过。
4. fail-closed 自检通过：临时测试 SID 的直连 IP 被拒绝，broker 允许域名可达。
5. 当前用户具有服务 named pipe 的授权身份。

任何检查失败都必须显示“受控网络组件不可用”。当前设置页同时检查本机 NetworkGuard readiness 和设备级服务端策略 readiness；后者区分未配对、SID 未注册、signer 未配置、托管 allowlist 缺失或策略不可编译。Store 保存前复查本机组件，ExecutionPolicyProvider 每次运行前再次复查；服务端仍在每次 terminal exec/session 时重新生成短期策略，因此配置随后失效也会 fail closed。已保存的 Controlled 不会被静默改写成 Disabled 或 Host，而是拒绝启动。

## 自动化验收门槛

NetworkGuard 实现只有满足以下测试才可把能力矩阵改为 `Windows 验收`：

1. 允许域名的 HTTP 和 TLS SNI 成功；同 IP 的未允许 SNI 失败。
2. 直连 IPv4/IPv6、外部 DNS、DoH、QUIC 和无 SNI TLS 失败。
3. 通配符、IDN、尾点、大小写、端口和 DNS TTL/rebind 行为符合策略。
4. 父进程启动的子进程不能绕过；不同 workspace/SID 的 lease 不能互用。
5. 服务崩溃、驱动重启、策略过期、签名错误和版本不匹配全部 fail closed。
6. 取消、超时、Connector 断线和退出登录会删除 lease 并回收 Job；残留 WFP allow/redirect filter 为零。
7. 日志、TRX、崩溃报告和 UI 不包含 Secret、API Key、URL query、请求正文或插件原始路径。
8. x64/ARM64、Windows 10 19041 和 Windows 11 分别验证驱动安装、升级、回滚和卸载。

## 当前状态

- AppContainer `Disabled`、`Controlled` 和 `Host` 的领域模型与 readiness 门禁已实现；Controlled 进程不授予系统网络 capability。
- 设置页已接设备级 `/controlled-network/readiness`，只有本机 driver/service 与服务端 signer、可信 SID、托管 allowlist 同时可用时才允许保存 Controlled；错误状态不暴露 SID 或域名正文。
- Local Connector 后端独立 Ed25519 signer 已实现，并通过 Config Center 管理 TTL、私钥路径和 Key ID；terminal exec/session API 只接受不含 SID/host/port 的策略申请，域名由服务端托管权限层推导，不信任普通调用方自报身份、域名或签名策略。
- Controlled 签名策略信封和验证器已实现：owner/device/workspace/Windows SID identity、Canonical JSON、Ed25519、24 小时最大有效期、IDN、精确域名、单层通配符、80/443 和 IP literal 拒绝。
- Relay 已透传 `network_policy` 并在审批/执行前核对 owner/device/workspace；一次性 exec 与 ConPTY 均执行 suspended → Job → acquire lease → resume，续租丢失终止完整 Job。
- Connector 侧 NetworkGuard v1.0 协议、256 KB framed JSON、named-pipe服务账号校验、readiness、active lease count、lease 生命周期和续租失败 fail-closed 已实现。
- HTTP Host 与 TLS ClientHello SNI 判定核心已实现，包含重复/absolute-form/非 ASCII 拒绝、跨 TLS record 重组和无 SNI fail closed。
- 普通 AppContainer 按 workspace/permission 使用稳定 SID；Controlled 每次 exec/ConPTY 使用独立 SID，避免同策略并发 lease 相互替换。退出后删除临时 profile、撤销 workspace ACL 与临时目录，崩溃残留由受保护注册元数据做有界 GC。
- 独立 `ChatOS.NetworkGuard.Service` Windows Service 已实现：ACL 收紧的 named pipe、真实客户端进程 SID 读取、目标进程用户/AppContainer SID 复验、签名策略复验、精确 lease 身份和过期回收。
- 本机 HTTP/TLS broker 已实现：只接受带可信 WFP redirect context 的连接，Host/SNI 允许后才解析并连接上游，握手 64 KB 上限、超时、端口一致性和双向转发均 fail closed。
- WDK 驱动项目、IOCTL 协议、WFP provider/sublayer、IPv4/IPv6 connect redirect、AppContainer 默认拒绝、broker 通道、active lease health、reset reconciliation 和 x64/ARM64 primitive-driver INF 已加入 `native/ChatOS.NetworkGuard.Driver`。
- Service/driver 构建、开发期 SYS → Inf2Cat → CAT 本机测试签名、Hardware Dev Center submission CAB、Microsoft 签名结果导入、SHA-256 manifest 校验、服务依赖、旧 OEM INF 清理及管理员安装/升级/卸载脚本已加入 `build/networkguard*.ps1`。签名模式明确区分 `unsigned`、`local_test` 和 `microsoft_production`，默认生命周期安装只接受生产模式。
- `build/networkguard-acceptance.ps1` 会验证允许 HTTP/TLS、同 IP denied SNI、IPv4/可选 IPv6 literal、DoH、UDP 53、QUIC/UDP 443、无 SNI、子进程、服务/驱动重启和 active lease residue=0，并删除验收私钥后输出证据。
- 当前 C# 共 382 项测试通过，其中 Connector 242、NetworkGuard 19；Connector 新增 Desktop Automation ID 静态契约。Rust Local Connector 81 项通过（1 项外部 Valkey 测试按环境忽略），Config Center 79 项通过。ChatOS backend 另有策略申请序列化测试；服务端拒绝包含 SID/host/port 注入字段的策略申请，并保证设备 API 不回传已注册 Windows SID。
- Windows 2022 CI 已增加 x64/ARM64 WDK matrix，必须上传 SYS/CAT/INF、Service、SHA-256 manifest 和 WDK build report；在首轮工作流成功前仍不视为已有实际驱动编译证据。
- 当前 macOS 环境不能用 WDK 编译驱动，也不能访问 Microsoft Hardware Dev Center 完成生产签名，更不能证明 WFP 与 AppContainer capability 的真机仲裁行为。因此能力仍保持 `实现中`；必须在装有 VS2022 + WDK 的 Windows 10/11 x64 与 Windows 11 ARM64 上完成编译、微软生产签名、安装和第 1-8 项自动化验收，才可提升为 `Windows 验收`。
