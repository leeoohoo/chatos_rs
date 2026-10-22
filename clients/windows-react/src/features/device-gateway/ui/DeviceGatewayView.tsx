import { useEffect, type ReactNode } from "react";
import { useGeneralSettingsStore } from "../../general-settings/application/generalSettingsStore";
import { useHostLifecycleStore } from "../../host-lifecycle/application/hostLifecycleStore";
import type { HostLifecyclePort } from "../../host-lifecycle/ports/HostLifecyclePort";
import { useDeviceGatewayStore } from "../application/deviceGatewayStore";
import type { DeviceGatewayPort } from "../ports/DeviceGatewayPort";
import "./deviceGateway.css";

export function DeviceGatewayView({ ownerUserId, username, port, hostPort }: { ownerUserId: string; username: string; port: DeviceGatewayPort; hostPort: HostLifecyclePort }) {
  const gateway = useDeviceGatewayStore((state) => state.snapshot);
  const isLoading = useDeviceGatewayStore((state) => state.isLoading);
  const error = useDeviceGatewayStore((state) => state.error);
  const load = useDeviceGatewayStore((state) => state.load);
  const reset = useDeviceGatewayStore((state) => state.reset);
  const host = useHostLifecycleStore((state) => state.snapshot);
  const start = useHostLifecycleStore((state) => state.start);
  const stop = useHostLifecycleStore((state) => state.stop);
  const language = useGeneralSettingsStore((state) => state.settings.interfaceLanguage);
  const english = language === "en-US";

  useEffect(() => {
    void load(port, ownerUserId);
    return () => reset();
  }, [load, ownerUserId, port, reset]);

  useEffect(() => {
    if (gateway && gateway.connectorRunning !== host.ready) void load(port, ownerUserId);
  }, [gateway, host.ready, load, ownerUserId, port]);

  if (!gateway && isLoading) return <GatewayStatus loading title={english ? "Reading device status" : "正在读取设备状态"} detail={english ? "Checking the local identity and Rust Host." : "正在检查本机身份与 Rust Host。"} />;
  if (!gateway) return <GatewayStatus title={english ? "Device status unavailable" : "设备状态暂时不可用"} detail={error ?? (english ? "Unable to read the native connector state." : "无法读取本机连接器状态。") } action={english ? "Retry" : "重试"} onAction={() => void load(port, ownerUserId)} />;

  const connecting = ["launching", "handshaking", "synchronizing", "restarting"].includes(host.phase);
  const connected = gateway.connectorRunning && host.ready;
  const toggleConnection = async () => {
    if (connected) await stop(hostPort);
    else await start(hostPort, ownerUserId);
    await load(port, ownerUserId);
  };

  return <div className="device-gateway-page">
    {error && <div className="device-gateway-alert" role="alert">! {error}<button onClick={() => void load(port, ownerUserId)} type="button">{english ? "Retry" : "重试"}</button></div>}
    <GatewayCard icon="⌁" title={english ? "Connection Status" : "连接状态"} subtitle={connected ? (english ? "Connected to ChatOS through the authenticated control plane" : "已通过账号认证控制面连接 ChatOS") : (english ? "Local configuration is retained while disconnected" : "本机配置已保留，当前未连接")}>
      <div className="device-gateway-health"><i className={connected ? "is-online" : ""} /><strong>{connected ? (english ? "Local Agent Host is healthy" : "本机 Agent Host 运行正常") : (connecting ? (english ? "Starting the local service" : "正在启动本机服务") : (english ? "Local service is disconnected" : "本机服务未连接"))}</strong></div>
      <div className="device-gateway-divider" />
      <GatewayValue label={english ? "User" : "用户"} value={username} />
      <GatewayValue label={english ? "Device" : "设备"} value={gateway.deviceName} />
      <GatewayValue label="Device ID" value={gateway.deviceId} mono />
      <GatewayValue label={english ? "Gateway" : "网关"} value={gateway.cloudBaseUrl} mono />
      <GatewayValue label={english ? "Connection mode" : "连接方式"} value={english ? "Authenticated control plane" : "账号认证控制面"} />
      <div className="device-gateway-actions"><button className={connected ? "is-danger" : "is-primary"} disabled={connecting} onClick={() => void toggleConnection()} type="button">{connecting ? (english ? "Connecting…" : "连接中…") : connected ? (english ? "Disconnect Local Service" : "断开本机服务") : (english ? "Connect Gateway" : "连接网关")}</button><button disabled={isLoading} onClick={() => void load(port, ownerUserId)} type="button">{english ? "Refresh Status" : "刷新状态"}</button></div>
    </GatewayCard>

    {!gateway.deviceSignedRelayAvailable && <section className="device-gateway-difference"><span aria-hidden="true">i</span><div><strong>{english ? "Windows connection model" : "Windows 连接方式"}</strong><p>{english ? "This client uses the signed-in account session and a local Rust Host. The macOS device-signed long-lived Relay is not claimed on Windows." : "当前 Windows 客户端使用登录账号会话与本机 Rust Host，不宣称具备 macOS 的设备签名长连接 Relay。"}</p></div></section>}

    <GatewayCard icon="▣" title={english ? "Local Boundaries" : "本机边界"} subtitle={english ? "Files, terminals, and permission policies execute only on this device." : "文件、终端和权限策略只在当前设备执行。"}>
      <BoundaryRow icon="▤" label={english ? "File routing" : "文件路由"} value={gateway.workspaceCount === 0 ? (english ? "No workspaces" : "尚无工作区") : (english ? `${gateway.workspaceCount} workspaces` : `${gateway.workspaceCount} 个工作区`)} />
      <BoundaryRow icon="✓" label={english ? "Access control" : "权限控制"} value={english ? "Enforced by Tauri / Rust" : "由 Tauri / Rust 强制执行"} />
      <BoundaryRow icon="◉" label={english ? "Runtime" : "运行方式"} value={gateway.runtime} />
      <BoundaryRow icon="⌘" label={english ? "Developer mode" : "开发环境"} value={gateway.developerMode ? (english ? "Enabled" : "已开启") : (english ? "Disabled" : "已关闭")} />
      <div className="device-gateway-divider" />
      <p className="device-gateway-note">{english ? "Cloud requests must pass account authentication, workspace boundaries, and command approval. React never receives local paths and cannot execute commands directly." : "云端请求必须先经过账号认证、工作区边界与命令审批；React 不接收本机路径，也不能直接执行命令。"}</p>
    </GatewayCard>
  </div>;
}

function GatewayCard({ icon, title, subtitle, children }: { icon: string; title: string; subtitle: string; children: ReactNode }) { return <section className="device-gateway-card"><header><span aria-hidden="true">{icon}</span><div><h2>{title}</h2><p>{subtitle}</p></div></header><div className="device-gateway-card__body">{children}</div></section>; }
function GatewayValue({ label, value, mono = false }: { label: string; value: string; mono?: boolean }) { return <div className="device-gateway-value"><span>{label}</span><strong className={mono ? "is-mono" : ""}>{value}</strong></div>; }
function BoundaryRow({ icon, label, value }: { icon: string; label: string; value: string }) { return <div className="device-gateway-boundary"><span aria-hidden="true">{icon}</span><strong>{label}</strong><p>{value}</p></div>; }
function GatewayStatus({ title, detail, loading, action, onAction }: { title: string; detail: string; loading?: boolean; action?: string; onAction?(): void }) { return <div className="device-gateway-status"><span aria-hidden="true">{loading ? <i className="mini-spinner" /> : "!"}</span><h2>{title}</h2><p>{detail}</p>{action && <button onClick={onAction} type="button">{action}</button>}</div>; }
