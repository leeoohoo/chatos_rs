import { describe, expect, it } from "vitest";
import {
  mediaWorkflowRunEventSchema,
  mediaWorkflowSchema,
  newMediaWorkflow,
  newMediaWorkflowNode,
} from "./mediaWorkflow";

describe("media workflow domain", () => {
  it("accepts a linear workflow with a single terminal video node", () => {
    const workflow = newMediaWorkflow("image-model");
    workflow.nodes[0]!.prompt = "make a still";
    const video = newMediaWorkflowNode("video", "video-model");
    video.prompt = "animate the still";
    workflow.nodes.push(video);
    expect(video).toMatchObject({ videoSize: "768P", videoRatio: "16:9", videoSeconds: 4 });
    expect(mediaWorkflowSchema.parse(workflow)).toEqual(workflow);
  });

  it("rejects video options outside the MiniMax-H3 protocol", () => {
    const workflow = newMediaWorkflow("video-model", "video");
    expect(mediaWorkflowSchema.safeParse({
      ...workflow,
      nodes: [{ ...workflow.nodes[0]!, videoSize: "1280x720" }],
    }).success).toBe(false);
    expect(mediaWorkflowSchema.safeParse({
      ...workflow,
      nodes: [{ ...workflow.nodes[0]!, videoRatio: "2:1" }],
    }).success).toBe(false);
    expect(mediaWorkflowSchema.safeParse({
      ...workflow,
      nodes: [{ ...workflow.nodes[0]!, videoSeconds: 16 }],
    }).success).toBe(false);
  });

  it("rejects a video node before another node", () => {
    const workflow = newMediaWorkflow("image-model");
    workflow.nodes[0]!.prompt = "make a still";
    workflow.nodes[0]!.kind = "video";
    workflow.nodes.push(newMediaWorkflowNode("text", "text-model"));
    expect(mediaWorkflowSchema.safeParse(workflow).success).toBe(false);
  });

  it("requires exact node event fields and persisted result shapes", () => {
    const event = {
      ownerUserId: "alice",
      workflowId: "11111111-1111-4111-8111-111111111111",
      clientRequestId: "request-1",
      nodeId: "22222222-2222-4222-8222-222222222222",
      phase: "completed",
      progress: null,
      result: { type: "text", text: "finished" },
      errorMessage: null,
    };
    expect(mediaWorkflowRunEventSchema.parse(event)).toEqual(event);
    expect(mediaWorkflowRunEventSchema.safeParse({ ...event, result: { type: "video", item: { previewUrl: "file:///unsafe" } } }).success).toBe(false);
  });
});
