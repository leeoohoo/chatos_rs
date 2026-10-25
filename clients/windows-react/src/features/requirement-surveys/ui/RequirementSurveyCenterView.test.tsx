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
    expect(screen.getByRole("contentinfo", { name: "项目分页" })).toHaveTextContent("1–2 / 2");
  });

  it("paginates projects with the macOS 10, 20 and 50 item sizes", async () => {
    const projects = Array.from({ length: 25 }, (_, index) => ({
      id: `project-${index + 1}`,
      title: `项目 ${String(index + 1).padStart(2, "0")}`,
    }));
    const port = stubPort({ listProjectRequirementSurveys: vi.fn(async () => []) });
    render(<RequirementSurveyCenterView hostReady ownerUserId="owner-1" projects={projects} port={port} />);

    expect(await screen.findByRole("button", { name: /项目 01/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: /项目 21/ })).not.toBeInTheDocument();
    expect(screen.getByRole("contentinfo", { name: "项目分页" })).toHaveTextContent("1–20 / 25");

    fireEvent.click(screen.getByRole("button", { name: "项目下一页" }));
    expect(screen.getByRole("button", { name: /项目 21/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: /项目 01/ })).not.toBeInTheDocument();

    fireEvent.change(screen.getByRole("combobox", { name: "项目每页" }), { target: { value: "10" } });
    expect(screen.getByRole("button", { name: /项目 01/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: /项目 11/ })).not.toBeInTheDocument();
    expect(screen.getByRole("contentinfo", { name: "项目分页" })).toHaveTextContent("1–10 / 25");
  });

  it("paginates project surveys and preserves the page while inspecting one survey", async () => {
    const surveys = Array.from({ length: 25 }, (_, index) => survey({
      surveyId: `survey-${index + 1}`,
      title: `调研 ${String(index + 1).padStart(2, "0")}`,
      createdAtUnixMs: index + 1,
      updatedAtUnixMs: index + 1,
    }));
    const port = stubPort({ listProjectRequirementSurveys: vi.fn(async () => surveys) });
    render(<RequirementSurveyCenterView hostReady ownerUserId="owner-1" projects={[
      { id: "project-1", title: "Alpha" },
    ]} port={port} />);

    fireEvent.click(await screen.findByRole("button", { name: /Alpha/ }));
    expect(screen.getByRole("button", { name: /调研 25/ })).toBeVisible();
    expect(screen.queryByRole("button", { name: /调研 05/ })).not.toBeInTheDocument();
    expect(screen.getByRole("contentinfo", { name: "调研分页" })).toHaveTextContent("1–20 / 25");

    fireEvent.click(screen.getByRole("button", { name: "调研下一页" }));
    fireEvent.click(screen.getByRole("button", { name: /调研 05/ }));
    fireEvent.click(screen.getByRole("button", { name: "‹ 返回调研列表" }));
    expect(screen.getByRole("button", { name: /调研 05/ })).toBeVisible();
    expect(screen.getByRole("contentinfo", { name: "调研分页" })).toHaveTextContent("21–25 / 25");

    fireEvent.click(screen.getByRole("button", { name: "‹ 返回项目列表" }));
    fireEvent.click(screen.getByRole("button", { name: /Alpha/ }));
    expect(screen.getByRole("button", { name: /调研 25/ })).toBeVisible();
    expect(screen.getByRole("contentinfo", { name: "调研分页" })).toHaveTextContent("1–20 / 25");
  });

  it("opens a project, answers every question kind and resumes the exact survey", async () => {
    const openSurvey = survey({
      questions: [
        { questionId: "goal", prompt: "需要交付什么？", responseKind: "text", required: true, options: [] },
        { questionId: "mode", prompt: "选择模式", responseKind: "single_choice", required: true, options: ["A", "B"] },
        { questionId: "areas", prompt: "选择范围", responseKind: "multiple_choice", required: true, options: ["UI", "Host"] },
        { questionId: "optional", prompt: "可选范围", responseKind: "multiple_choice", required: false, options: ["文档"] },
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
    fireEvent.click(within(detail).getByLabelText("文档"));
    fireEvent.click(within(detail).getByLabelText("文档"));
    fireEvent.click(within(detail).getByLabelText("否"));
    fireEvent.click(within(detail).getByRole("button", { name: "提交并继续" }));

    await waitFor(() => expect(resolve).toHaveBeenCalledWith(
      "owner-1", "project-1", "survey-1", 1,
      { goal: "交付 Alpha", mode: "A", areas: ["UI", "Host"], ship: false },
    ));
    expect(await within(detail).findByText(/下方可查看方案或后续回复/)).toBeVisible();
  });

  it("does not load surveys until the Host is ready", () => {
    const port = stubPort();
    render(<RequirementSurveyCenterView hostReady={false} ownerUserId="owner-1" projects={[
      { id: "project-1", title: "Alpha" },
    ]} port={port} />);
    expect(screen.getByText("正在连接本机服务")).toBeVisible();
    expect(port.listProjectRequirementSurveys).not.toHaveBeenCalled();
  });

  it("shows the exact plan status and confirms without starting work", async () => {
    const resolved = survey({
      status: "resolved", version: 2, answers: { goal: "Alpha" },
      updatedAtUnixMs: 30, resolvedAtUnixMs: 30,
    });
    const currentPlan = plan({ markdown: "# 实施方案" });
    const confirmPlan = vi.fn(async () => ({ ...currentPlan, confirmedAtUnixMs: 50 }));
    const onOpenConversation = vi.fn();
    const port = stubPort({
      listProjectRequirementSurveys: vi.fn(async () => [resolved]),
      loadFollowUp: vi.fn(async () => ({
        surveyId: "survey-1", surveyVersion: 2,
        followUp: { status: "completed_without_reply" as const, markdown: null },
        plan: currentPlan,
        planFollowUp: { status: "completed_without_reply" as const, markdown: null },
      })),
      confirmPlan,
    });
    render(<RequirementSurveyCenterView hostReady ownerUserId="owner-1" onOpenConversation={onOpenConversation} projects={[
      { id: "project-1", title: "Alpha" },
    ]} port={port} />);
    fireEvent.click(await screen.findByRole("button", { name: /Alpha/ }));
    fireEvent.click(await screen.findByRole("button", { name: /项目目标/ }));
    expect(await screen.findByRole("heading", { name: "实施方案" })).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "确认当前方案" }));
    await waitFor(() => expect(confirmPlan).toHaveBeenCalledWith(
      "owner-1", "project-1", "survey-1", 1,
    ));
    expect(await screen.findByText("方案已确认，本次不会自动执行任何任务")).toBeVisible();
    fireEvent.click(screen.getByRole("button", { name: "返回原对话，安排下一步" }));
    expect(onOpenConversation).toHaveBeenCalledWith("project-1", "conversation-1");
  });
});

function stubPort(overrides: Partial<RequirementSurveyPort> = {}): RequirementSurveyPort {
  const defaults: RequirementSurveyPort = {
    listProjectRequirementSurveys: vi.fn(async () => [survey()]),
    resolveProjectRequirementSurvey: vi.fn(async (_owner, _project, _survey, version, answers) => survey({
      status: "resolved", version: version + 1, answers, updatedAtUnixMs: 30, resolvedAtUnixMs: 30,
    })),
    loadFollowUp: vi.fn(async (_owner, _project, surveyId) => ({
      surveyId, surveyVersion: 2,
      followUp: { status: "processing" as const, markdown: null },
      plan: null, planFollowUp: null,
    })),
    confirmPlan: vi.fn(async () => plan({ confirmedAtUnixMs: 50 })),
    adoptPlan: vi.fn(async () => plan({ confirmedAtUnixMs: 50 })),
  };
  return { ...defaults, ...overrides } as RequirementSurveyPort;
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

function plan(overrides: Record<string, unknown> = {}) {
  return {
    planId: "requirement-plan:survey-1", revision: 1, projectId: "project-1",
    sourceConversationId: "conversation-1", sourceSurveyId: "survey-1", sourceRunId: "run-1",
    title: "方案", markdown: "# 方案", createdAtUnixMs: 40, confirmedAtUnixMs: null,
    ...overrides,
  };
}
