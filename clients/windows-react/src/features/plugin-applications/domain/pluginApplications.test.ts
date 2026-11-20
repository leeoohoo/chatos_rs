import { describe, expect, it } from "vitest";
import {
  pluginApplicationLaunchSchema,
  pluginApplicationSchema,
  pluginFrameRequestSchema,
} from "./pluginApplications";
import {
  pluginApplicationFixture,
  pluginApplicationLaunchFixture,
} from "../../../test/pluginApplicationFixture";

describe("plugin application domain", () => {
  it("accepts the projected native catalog shape and rejects hidden installation fields", () => {
    expect(pluginApplicationSchema.parse(pluginApplicationFixture()).displayName).toBe("Canvas");
    expect(() => pluginApplicationSchema.parse({
      ...pluginApplicationFixture(),
      executable: "C:\\Plugins\\canvas.exe",
    })).toThrow();
    const iconPreviewUrl = "http://chatos-plugin-icon.localhost/11111111-1111-4111-8111-111111111111/22222222-2222-4222-8222-222222222222";
    expect(pluginApplicationSchema.parse(pluginApplicationFixture({ iconPreviewUrl })).iconPreviewUrl)
      .toBe(iconPreviewUrl);
    expect(() => pluginApplicationSchema.parse(pluginApplicationFixture({
      iconPreviewUrl: "file:///C:/Plugins/canvas/logo.png",
    }))).toThrow();
  });

  it("only accepts exact loopback HTTP launches from the native runtime", () => {
    const launch = pluginApplicationLaunchSchema.parse(pluginApplicationLaunchFixture());
    expect(launch.bridgeOrigin).toBe("http://127.0.0.1:43127");
    expect(launch.url).toContain("/__chatos_plugin_session?token=");
    expect(() => pluginApplicationLaunchSchema.parse(pluginApplicationLaunchFixture({
      url: "https://plugins.example.com/",
      bridgeOrigin: "https://plugins.example.com",
    }))).toThrow();
    expect(() => pluginApplicationLaunchSchema.parse(pluginApplicationLaunchFixture({
      bridgeOrigin: "http://localhost:43127",
    }))).toThrow();
  });

  it("requires a bounded strict bridge envelope", () => {
    const request = {
      type: "chatos.plugin_ui.request",
      protocol_version: 1,
      adapter_session_id: "plugin-session-1",
      host_session_nonce: "b".repeat(64),
      request_id: "request-1",
      method: "host.context.read",
      payload: {},
    };
    expect(pluginFrameRequestSchema.parse(request).request_id).toBe("request-1");
    expect(() => pluginFrameRequestSchema.parse({ ...request, executable: "cmd.exe" })).toThrow();
    expect(() => pluginFrameRequestSchema.parse({ ...request, request_id: "\n" })).toThrow();
  });
});
