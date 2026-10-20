import { useCallback, useEffect, useRef } from "react";
import {
  pluginFrameRequestSchema,
  type PluginApplicationLaunch,
} from "../domain/pluginApplications";
import type { PluginApplicationPort } from "../ports/PluginApplicationPort";

interface RestrictedPluginFrameProps {
  ownerUserId: string;
  presentationId: string;
  generation: number;
  launch: PluginApplicationLaunch;
  port: PluginApplicationPort;
  reloadToken: number;
}

export function RestrictedPluginFrame({
  ownerUserId,
  presentationId,
  generation,
  launch,
  port,
  reloadToken,
}: RestrictedPluginFrameProps) {
  const frameRef = useRef<HTMLIFrameElement>(null);
  const frameGeneration = useRef(0);
  const mounted = useRef(true);
  const observedRequestIds = useRef(new Set<string>());

  useEffect(() => {
    mounted.current = true;
    return () => {
      mounted.current = false;
      frameGeneration.current += 1;
      observedRequestIds.current.clear();
    };
  }, []);

  const sendReady = useCallback(() => {
    const target = frameRef.current?.contentWindow;
    if (!target) return;
    frameGeneration.current += 1;
    observedRequestIds.current.clear();
    target.postMessage({
      type: "chatos.plugin_ui.ready",
      protocol_version: 1,
      adapter_session_id: launch.sessionId,
      host_session_nonce: launch.hostSessionNonce,
      capabilities: [...launch.bridgeCapabilities],
    }, launch.bridgeOrigin);
  }, [launch]);

  useEffect(() => {
    const receive = (event: MessageEvent<unknown>) => {
      const target = frameRef.current?.contentWindow;
      if (!target || event.source !== target || event.origin !== launch.bridgeOrigin) return;
      const acceptedTarget = target;
      const serialized = boundedJson(event.data);
      if (serialized == null) return;
      const parsed = pluginFrameRequestSchema.safeParse(JSON.parse(serialized));
      if (!parsed.success) return;
      const request = parsed.data;
      if (request.adapter_session_id !== launch.sessionId
        || request.host_session_nonce !== launch.hostSessionNonce
        || !launch.bridgeCapabilities.includes(request.method)
        || observedRequestIds.current.has(request.request_id)
        || observedRequestIds.current.size >= 4_096) return;
      observedRequestIds.current.add(request.request_id);
      const acceptedFrameGeneration = frameGeneration.current;
      void port.handleBridgeRequest({
        ownerUserId,
        presentationId,
        generation,
        sessionId: launch.sessionId,
        hostSessionNonce: launch.hostSessionNonce,
        origin: launch.bridgeOrigin,
        requestId: request.request_id,
        method: request.method,
        payload: request.payload,
      }).then((response) => {
        if (response.requestId !== request.request_id) throw new Error("bridge response mismatch");
        respond({
          type: "chatos.plugin_ui.response",
          protocol_version: 1,
          adapter_session_id: launch.sessionId,
          host_session_nonce: launch.hostSessionNonce,
          request_id: request.request_id,
          ok: true,
          result: response.result,
        });
      }).catch(() => {
        respond({
          type: "chatos.plugin_ui.response",
          protocol_version: 1,
          adapter_session_id: launch.sessionId,
          host_session_nonce: launch.hostSessionNonce,
          request_id: request.request_id,
          ok: false,
          result: {},
          error_code: "host_request_failed",
          error_message: "宿主请求失败。",
        });
      });

      function respond(message: Record<string, unknown>) {
        if (!mounted.current
          || frameGeneration.current !== acceptedFrameGeneration
          || frameRef.current?.contentWindow !== acceptedTarget) return;
        acceptedTarget.postMessage(message, launch.bridgeOrigin);
      }
    };
    window.addEventListener("message", receive);
    return () => window.removeEventListener("message", receive);
  }, [generation, launch, ownerUserId, port, presentationId]);

  return (
    <iframe
      aria-label={`${launch.application.displayName} 应用内容`}
      className="plugin-frame"
      key={`${launch.sessionId}:${reloadToken}`}
      onLoad={sendReady}
      ref={frameRef}
      referrerPolicy="no-referrer"
      sandbox="allow-forms allow-same-origin allow-scripts"
      src={launch.url}
      title={launch.application.displayName}
    />
  );
}

function boundedJson(value: unknown): string | null {
  try {
    const serialized = JSON.stringify(value);
    if (serialized == null || new TextEncoder().encode(serialized).byteLength > 256 * 1_024) {
      return null;
    }
    return serialized;
  } catch {
    return null;
  }
}
