import { describe, expect, it } from "vitest";
import {
  isCancellableTeamRunDetailStatus,
  shouldPollTeamRunDetail,
  teamRunDetailStatusLabel,
} from "./AgentManagementView";
import {
  teamRunCheckpointStatusLabel,
  teamRunStatusClass,
} from "./agentTeamRunPresentation";

describe("Agent team Run presentation", () => {
  it("prioritizes follow-up gates in the team Run detail status", () => {
    expect(teamRunDetailStatusLabel("reviewing_followup", "running")).toBe("审查追加要求");
    expect(teamRunDetailStatusLabel("waiting_confirmation", "failed")).toBe("等待确认");
    expect(teamRunDetailStatusLabel("succeeded", "succeeded")).toBe("已完成");
  });

  it("keeps team Run detail polling through review gates", () => {
    expect(shouldPollTeamRunDetail("reviewing_followup")).toBe(true);
    expect(shouldPollTeamRunDetail(" waiting_confirmation ")).toBe(true);
    expect(shouldPollTeamRunDetail("succeeded")).toBe(false);
  });

  it("keeps review gate labels when the run list hydrates detail status", () => {
    expect(teamRunCheckpointStatusLabel("reviewing_followup")).toBe("审查追加要求");
    expect(teamRunCheckpointStatusLabel("waiting_confirmation")).toBe("等待确认");
    expect(teamRunStatusClass("reviewing_followup")).toBe("attention");
    expect(teamRunStatusClass("waiting_confirmation")).toBe("attention");
  });

  it("offers cancellation for every non-terminal Host Run state", () => {
    expect(isCancellableTeamRunDetailStatus("waiting_tool_result")).toBe(true);
    expect(isCancellableTeamRunDetailStatus("needs_review")).toBe(true);
    expect(isCancellableTeamRunDetailStatus("cancelled")).toBe(false);
    expect(isCancellableTeamRunDetailStatus("succeeded")).toBe(false);
  });
});
