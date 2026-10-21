import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useRequirementSurveyCenterStore } from "../application/requirementSurveyCenterStore";
import type { RequirementSurvey } from "../domain/requirementSurvey";
import type { RequirementSurveyPort } from "../ports/RequirementSurveyPort";
import { RequirementSurveyCenterView } from "./RequirementSurveyCenterView";

beforeEach(() => useRequirementSurveyCenterStore.getState().reset());
afterEach(() => { cleanup(); vi.restoreAllMocks(); });

describe("RequirementSurveyCenterView", () => {
  it("shows the macOS-aligned project center and explicit empty project state", async () => {
    const port = stubPort({ listProjectRequirementSurveys: vi.fn(async () => []) });
    const { rerender } = render(
      <RequirementSurveyCenterView hostReady ownerUserId="owner-1" projects={[]} port={port} />,
    );
    expect(screen.getByText("暂无项目")).toBeVisible();
    expect(screen.getByText(/创建项目后/)).toBeVisible();

    rerender(<RequirementSurveyCenterView hostReady ownerUserId="owner-1" projects={[
      { id: "project-1", title: "Alpha" }, { id: "project-2", title: "Beta" },
    ]} port={port} />);
    expect(await screen.findByRole("button", { name: /Alpha/ })).toBeVisible();
    expect(screen.getByRole("button", { name: /Beta/ })).toBeVisible();
    expect(port.listProjectRequirementSurveys).toHaveBeenCalledWith("owner-1", "project-1");
    expect(port.listProjectRequirementSurveys).toHaveBeenCalledWith("owner-1", "project-2");
  });

  it("opens a project, answers every question kind and resumes the exact survey", async () => {
    const openSurvey = survey({
      questions: [
        { questionId: "goal", prompt: "需要交付什么？", responseKind: "text", required: true, options: [] },
        { questionId: "mode", prompt: "选择模式", responseKind: "single_choice", required: true, options: ["A", "B"] },
        { questionId: "areas", prompt: "选择范围", responseKind: "multiple_choice", required: true, options: ["UI", "Host"] },
        { questionId: "ship", prompt: "是否发布", responseKind: "boolean", required: true, options: [] },
      ],
    });
    const resolvedSurvey = survey({
      surveyId: "survey-2", title: "历史调研", status: "resolved", version: 2,
      answers: { goal: "完成" }, updatedAtUnixMs: 30, resolvedAtUnixMs: 30,
    });
    const resolve = vi.fn(async (_owner, _project, _survey, version, answers) => ({
      ...openSurvey, status: "resolved" as const, version: version + 1, answers,
      updatedAtUnixMs: 40, resolvedAtUnixMs: 40,
    }));
    const port = stubPort({
      listProjectRequirementSurveys: vi.fn(async () => [openSurvey, resolvedSurvey]),
      resolveProjectRequirementSurvey: resolve,
    });
    render(<RequirementSurveyCenterView hostReady ownerUserId="owner-1" projects={[
      { id: "project-1", title: "Alpha" },
    ]} port={port} />);

    fireEvent.click(await screen.findByRole("button", { name: /Alpha/ }));
    const project = await screen.findByRole("region", { name: "Alpha的需求调研" });
    expect(within(project).getAllByText("待填写").length).toBeGreaterThan(0);
    expect(within(project).getAllByText("已提交").length).toBeGreaterThan(0);
    fireEvent.click(within(project).getByRole("button", { name: /项目目标/ }));

    const detail = await screen.findByRole("region", { name: "需求调研详情" });
    fireEvent.change(within(detail).getByLabelText("需要交付什么？"), { target: { value: "交付 Alpha" } });
    fireEvent.click(within(detail).getByLabelText("A"));
    fireEvent.click(within(detail).getByLabelText("UI"));
    fireEvent.click(within(detail).getByLabelText("Host"));
    fireEvent.click(within(detail).getByLabelText("否"));
    fireEvent.click(within(detail).getByRole("button", { name: "提交并继续" }));

    await waitFor(() => expect(resolve).toHaveBeenCalledWith(
      "owner-1", "project-1", "survey-1", 1,
      { goal: "交付 Alpha", mode: "A", areas: ["UI", "Host"], ship: false },
    ));
    expect(await within(detail).findByText(/原 Run 已恢复/)).toBeVisible();
  });

  it("does not load surveys until the Host is ready", () => {
    const port = stubPort();
    render(<RequirementSurveyCenterView hostReady={false} ownerUserId="owner-1" projects={[
      { id: "project-1", title: "Alpha" },
    ]} port={port} />);
    expect(screen.getByText("正在连接本机服务")).toBeVisible();
    expect(port.listProjectRequirementSurveys).not.toHaveBeenCalled();
  });
});

function stubPort(overrides: Partial<RequirementSurveyPort> = {}): RequirementSurveyPort {
  return {
    listProjectRequirementSurveys: vi.fn(async () => [survey()]),
    resolveProjectRequirementSurvey: vi.fn(async (_owner, _project, _survey, version, answers) => survey({
      status: "resolved", version: version + 1, answers, updatedAtUnixMs: 30, resolvedAtUnixMs: 30,
    })),
    ...overrides,
  };
}

function survey(overrides: Partial<RequirementSurvey> = {}): RequirementSurvey {
  return {
    surveyId: "survey-1", projectId: "project-1", sourceConversationId: "conversation-1",
    sourceRunId: "run-1", sourceTaskId: null, title: "项目目标", description: "请补充目标",
    questions: [{ questionId: "goal", prompt: "需要交付什么？", responseKind: "text", required: true, options: [] }],
    answers: null, status: "open", version: 1, createdAtUnixMs: 10, updatedAtUnixMs: 20,
    resolvedAtUnixMs: null, ...overrides,
  };
}
