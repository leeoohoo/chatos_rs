import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { gitMutationResult, gitSnapshot, stubProjectGitPort } from "../../../test/projectGitFixture";
import { useProjectGitStore } from "../application/projectGitStore";
import { unavailableProjectGitSnapshot } from "../domain/projectGit";
import { ProjectGitView } from "./ProjectGitView";

describe("ProjectGitView", () => {
  beforeEach(() => useProjectGitStore.getState().reset());
  afterEach(cleanup);

  it("initializes a project that is not yet a repository", async () => {
    const unavailable = unavailableProjectGitSnapshot("project-1");
    const port = stubProjectGitPort(unavailable);
    port.initializeRepository = vi.fn(async () => gitMutationResult(gitSnapshot()));
    renderView(port);

    fireEvent.click(await screen.findByRole("button", { name: "初始化仓库" }));
    await waitFor(() => expect(port.initializeRepository).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
    }));
    expect(await screen.findByText("feat: project git")).toBeInTheDocument();
  });

  it("shows staged and working-tree groups and toggles their exact paths", async () => {
    const port = stubProjectGitPort();
    port.stagePaths = vi.fn(async () => gitMutationResult());
    port.unstagePaths = vi.fn(async () => gitMutationResult());
    renderView(port);

    fireEvent.click(await screen.findByRole("button", { name: "暂存 Sources/App.ts" }));
    await waitFor(() => expect(port.stagePaths).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      paths: ["Sources/App.ts"],
    }));
    fireEvent.click(screen.getByRole("button", { name: "取消暂存 README.md" }));
    await waitFor(() => expect(port.unstagePaths).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      paths: ["README.md"],
    }));
  });

  it("loads a diff and opens the matching project-relative file", async () => {
    const port = stubProjectGitPort();
    const onOpenFile = vi.fn();
    renderView(port, onOpenFile);

    fireEvent.click(await screen.findByRole("button", { name: "README.md 暂存区差异" }));
    expect(await screen.findByLabelText("README.md Git 差异")).toHaveTextContent("new");
    fireEvent.click(screen.getByRole("button", { name: "打开文件" }));
    expect(onOpenFile).toHaveBeenCalledWith("README.md");
  });

  it("commits the frozen message through the native port", async () => {
    const port = stubProjectGitPort();
    port.commit = vi.fn(async () => gitMutationResult(gitSnapshot({ changes: [] })));
    renderView(port);

    fireEvent.change(await screen.findByRole("textbox", { name: "Git 提交说明" }), {
      target: { value: "feat: finish git workbench" },
    });
    fireEvent.click(screen.getByRole("button", { name: "✓ 提交 1 个文件" }));
    await waitFor(() => expect(port.commit).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      message: "feat: finish git workbench",
    }));
    expect(screen.getByRole("textbox", { name: "Git 提交说明" })).toHaveValue("");
  });

  it("creates branches and adds remotes through Apple-style sheets", async () => {
    const port = stubProjectGitPort();
    port.createBranch = vi.fn(async () => gitMutationResult());
    port.saveRemote = vi.fn(async () => gitMutationResult());
    renderView(port);

    fireEvent.click(await screen.findByRole("button", { name: "新建 Git 分支" }));
    fireEvent.change(screen.getByRole("textbox", { name: "Git 分支名称" }), { target: { value: "feature/react-git" } });
    fireEvent.click(screen.getByRole("button", { name: "创建" }));
    await waitFor(() => expect(port.createBranch).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      name: "feature/react-git",
      switchToBranch: true,
    }));

    fireEvent.change(screen.getByRole("combobox", { name: "管理 Git 远端" }), { target: { value: "__new__" } });
    fireEvent.change(screen.getByRole("textbox", { name: "Git 远端地址" }), { target: { value: "https://example.com/new.git" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(port.saveRemote).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      originalName: null,
      name: "origin",
      url: "https://example.com/new.git",
    }));
  });

  it("preserves a redacted native remote URL when only its visible configuration is saved", async () => {
    const snapshot = gitSnapshot({
      remotes: [{
        name: "origin",
        url: "https://example.com/team/private.git",
        urlIsRedacted: true,
      }],
    });
    const port = stubProjectGitPort(snapshot);
    port.saveRemote = vi.fn(async () => gitMutationResult(snapshot));
    renderView(port);

    const remotePicker = await screen.findByRole("combobox", { name: "管理 Git 远端" });
    fireEvent.change(remotePicker, { target: { value: "origin" } });
    expect(screen.getByText(/凭据或本机路径已由原生层隐藏/)).toBeInTheDocument();
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(port.saveRemote).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      originalName: "origin",
      name: "origin",
      url: null,
    }));
  });
});

function renderView(port = stubProjectGitPort(), onOpenFile = vi.fn()) {
  return render(
    <ProjectGitView
      onOpenFile={onOpenFile}
      ownerUserId="owner-1"
      port={port}
      projectId="project-1"
    />,
  );
}
