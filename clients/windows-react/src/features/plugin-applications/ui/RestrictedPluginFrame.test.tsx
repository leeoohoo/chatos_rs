import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { pluginApplicationLaunchFixture } from "../../../test/pluginApplicationFixture";
import type { PluginApplicationPort } from "../ports/PluginApplicationPort";
import { RestrictedPluginFrame } from "./RestrictedPluginFrame";

afterEach(cleanup);

describe("RestrictedPluginFrame", () => {
  it("publishes only admitted capabilities and forwards an exact authenticated request", async () => {
    const adapter = port();
    renderFrame(adapter);
    const frame = screen.getByTitle("Canvas") as HTMLIFrameElement;
    const target = frame.contentWindow!;
    const postMessage = vi.spyOn(target, "postMessage");
    fireEvent.load(frame);
    expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({
      type: "chatos.plugin_ui.ready",
      capabilities: ["host.context.read"],
    }), "http://127.0.0.1:43127");
    expect(frame).toHaveAttribute("sandbox", "allow-forms allow-same-origin allow-scripts");

    window.dispatchEvent(new MessageEvent("message", {
      source: target,
      origin: "http://127.0.0.1:43127",
      data: bridgeRequest(),
    }));
    await waitFor(() => expect(adapter.handleBridgeRequest).toHaveBeenCalledWith({
      ownerUserId: "alice",
      presentationId: "plugin-view-1",
      generation: 3,
      sessionId: "plugin-session-1",
      hostSessionNonce: "b".repeat(64),
      origin: "http://127.0.0.1:43127",
      requestId: "request-1",
      method: "host.context.read",
      payload: {},
    }));
    await waitFor(() => expect(postMessage).toHaveBeenCalledWith(expect.objectContaining({
      type: "chatos.plugin_ui.response",
      request_id: "request-1",
      ok: true,
    }), "http://127.0.0.1:43127"));
  });

  it("rejects wrong origin, nonce, source, undeclared methods and replayed request IDs", async () => {
    const adapter = port();
    renderFrame(adapter);
    const frame = screen.getByTitle("Canvas") as HTMLIFrameElement;
    const target = frame.contentWindow!;
    fireEvent.load(frame);
    const dispatch = (data: object, origin = "http://127.0.0.1:43127", source: MessageEventSource = target) => {
      window.dispatchEvent(new MessageEvent("message", { source, origin, data }));
    };
    dispatch(bridgeRequest(), "http://127.0.0.1:9999");
    dispatch({ ...bridgeRequest(), host_session_nonce: "c".repeat(64) });
    dispatch(bridgeRequest(), undefined, window);
    dispatch({ ...bridgeRequest(), method: "artifact.read" });
    expect(adapter.handleBridgeRequest).not.toHaveBeenCalled();
    dispatch(bridgeRequest());
    await waitFor(() => expect(adapter.handleBridgeRequest).toHaveBeenCalledOnce());
    dispatch(bridgeRequest());
    expect(adapter.handleBridgeRequest).toHaveBeenCalledOnce();
  });
});

function renderFrame(port: PluginApplicationPort) {
  return render(
    <RestrictedPluginFrame
      generation={3}
      launch={pluginApplicationLaunchFixture()}
      ownerUserId="alice"
      port={port}
      presentationId="plugin-view-1"
      reloadToken={0}
    />,
  );
}

function bridgeRequest() {
  return {
    type: "chatos.plugin_ui.request",
    protocol_version: 1,
    adapter_session_id: "plugin-session-1",
    host_session_nonce: "b".repeat(64),
    request_id: "request-1",
    method: "host.context.read",
    payload: {},
  };
}

function port(): PluginApplicationPort {
  return {
    list: vi.fn(async () => []),
    launch: vi.fn(async () => pluginApplicationLaunchFixture()),
    discardPresentation: vi.fn(async () => undefined),
    handleBridgeRequest: vi.fn(async (request) => ({
      requestId: request.requestId,
      result: { projectId: "project-1" },
    })),
  };
}
