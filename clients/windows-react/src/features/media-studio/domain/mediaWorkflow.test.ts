import { describe, expect, it } from "vitest";
import { mediaWorkflowSchema, newMediaWorkflow, newMediaWorkflowNode } from "./mediaWorkflow";

describe("media workflow domain", () => {
  it("accepts a linear workflow with a single terminal video node", () => {
    const workflow = newMediaWorkflow("image-model");
    workflow.nodes[0]!.prompt = "make a still";
    const video = newMediaWorkflowNode("video", "video-model");
    video.prompt = "animate the still";
    workflow.nodes.push(video);
    expect(mediaWorkflowSchema.parse(workflow)).toEqual(workflow);
  });

  it("rejects a video node before another node", () => {
    const workflow = newMediaWorkflow("image-model");
    workflow.nodes[0]!.prompt = "make a still";
    workflow.nodes[0]!.kind = "video";
    workflow.nodes.push(newMediaWorkflowNode("text", "text-model"));
    expect(mediaWorkflowSchema.safeParse(workflow).success).toBe(false);
  });
});
