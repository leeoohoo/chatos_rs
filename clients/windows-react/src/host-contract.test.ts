import { describe, expect, it } from "vitest";
import healthFailure from "../../contracts/local-agent/v48/fixtures/health-failure.json";
import healthRequest from "../../contracts/local-agent/v48/fixtures/health-request.json";
import healthSuccess from "../../contracts/local-agent/v48/fixtures/health-success.json";
import {
  healthRequestSchema,
  healthResponseSchema,
  LOCAL_AGENT_MAX_FRAME_BYTES,
  LOCAL_AGENT_MAX_MESSAGE_BYTES,
} from "./features/host-lifecycle/domain/hostContract";

describe("Local Agent Host v48 shared fixtures", () => {
  it("parses the canonical health request", () => {
    expect(healthRequestSchema.parse(healthRequest).command.type).toBe("health");
  });

  it("parses success with the bounded transport limits", () => {
    const response = healthResponseSchema.parse(healthSuccess);
    expect(response.ok).toBe(true);
    if (response.ok) {
      expect(response.result.max_frame_bytes).toBe(LOCAL_AGENT_MAX_FRAME_BYTES);
      expect(response.result.max_message_bytes).toBe(LOCAL_AGENT_MAX_MESSAGE_BYTES);
      expect(response.result.scheduler).toEqual({
        maximum_concurrent_work_items: 4,
        effective_maximum_concurrent_work_items: 3,
        pressure_level: "elevated",
        active_work_items: 2,
        idle_workers: 2,
        queued_ready_work_items: 3,
      });
    }
  });

  it("preserves retryability on typed failures", () => {
    const response = healthResponseSchema.parse(healthFailure);
    expect(response.ok).toBe(false);
    if (!response.ok) expect(response.error.retryable).toBe(true);
  });
});
