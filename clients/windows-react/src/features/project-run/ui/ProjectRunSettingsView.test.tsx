import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import {
  projectRunAnalysis,
  runningProjectState,
  stubProjectRunPort,
} from "../../../test/projectRunFixture";
import { useProjectRunStore } from "../application/projectRunStore";
import { ProjectRunSettingsView } from "./ProjectRunSettingsView";

describe("ProjectRunSettingsView", () => {
  beforeEach(() => useProjectRunStore.getState().reset());
  afterEach(cleanup);

  it("renders the macOS project settings hierarchy and starts the selected opaque target", async () => {
    const port = stubProjectRunPort();
    port.start = vi.fn(async () => runningProjectState());
    renderView(port);

    expect(await screen.findByRole("heading", { name: "Project One" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "宠物快捷访问" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "运行预检" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "运行目标" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "运行实例" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "启动所需环境" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "项目配置文件" })).toBeInTheDocument();

    fireEvent.click(screen.getByRole("button", { name: "▶ 启动新实例" }));
    await waitFor(() => expect(port.start).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      targetId: "target-1",
    }));
    fireEvent.click(await screen.findByRole("button", { name: "› 展开运行日志" }));
    expect(await screen.findByText("ready on http://localhost:3000", { exact: false })).toBeInTheDocument();
  });

  it("uses only opaque toolchain IDs and does not reveal saved environment values", async () => {
    const port = stubProjectRunPort();
    port.updateEnvironment = vi.fn(async () => projectRunAnalysis());
    renderView(port);

    fireEvent.click(await screen.findByRole("button", { name: /高级设置：环境变量/ }));
    const secret = screen.getByLabelText("API_TOKEN 环境变量值");
    expect(secret).toHaveValue("");
    expect(secret).toHaveAttribute("placeholder", "已安全保存 · 输入以替换");
    fireEvent.change(screen.getByRole("combobox", { name: "选择 Node.js" }), { target: { value: "node-system" } });
    fireEvent.click(screen.getByRole("button", { name: "保存环境变量" }));

    await waitFor(() => expect(port.updateEnvironment).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      selectedToolchains: { node: "node-system" },
      environmentVariables: [{ key: "API_TOKEN", value: null }],
    }));
  });

  it("toggles the device favorite through the native port", async () => {
    const port = stubProjectRunPort();
    port.setFavorite = vi.fn(async () => ({ projectIds: ["project-1"] }));
    renderView(port);

    fireEvent.click(await screen.findByRole("checkbox", { name: "设为宠物常用项目" }));
    await waitFor(() => expect(port.setFavorite).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      isFavorite: true,
    }));
  });

  it("confirms project removal, preserves the local folder and notifies the shell", async () => {
    const port = stubProjectRunPort();
    port.removeProject = vi.fn(port.removeProject);
    const onProjectRemoved = vi.fn();
    renderView(port, onProjectRemoved);

    fireEvent.click(await screen.findByRole("button", { name: "删除项目" }));
    const dialog = screen.getByRole("alertdialog", { name: "删除项目？" });
    expect(dialog).toHaveTextContent("本机项目文件夹不会被删除");
    fireEvent.click(within(dialog).getByRole("button", { name: "删除" }));

    await waitFor(() => expect(port.removeProject).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
    }));
    expect(onProjectRemoved).toHaveBeenCalledWith("project-1");
  });
});

function renderView(port = stubProjectRunPort(), onProjectRemoved = vi.fn()) {
  return render(
    <ProjectRunSettingsView
      onProjectRemoved={onProjectRemoved}
      ownerUserId="owner-1"
      port={port}
      projectId="project-1"
      projectName="Project One"
    />,
  );
}
