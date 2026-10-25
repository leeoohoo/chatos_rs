import { cleanup, fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useProjectDirectoryStore } from "../application/projectDirectoryStore";
import { useProjectGitStore } from "../../project-git/application/projectGitStore";
import type { ProjectDirectoryListing } from "../domain/projectDirectory";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";
import { stubProjectGitPort } from "../../../test/projectGitFixture";
import { stubProjectRunPort } from "../../../test/projectRunFixture";
import { ProjectDirectoryView } from "./ProjectDirectoryView";
import { ProjectWorkspaceSurface } from "./ProjectWorkspaceSurface";

describe("ProjectDirectoryView", () => {
  beforeEach(() => useProjectDirectoryStore.getState().reset());
  afterEach(cleanup);

  it("loads a project-relative tree and expands directories", async () => {
    const port = stubPort();
    render(<ProjectDirectoryView ownerUserId="owner-1" port={port} projectId="project-1" />);
    const sources = await screen.findByRole("treeitem", { name: /Sources/ });
    fireEvent.click(sources);
    expect(await screen.findByRole("treeitem", { name: /App.ts/ })).toBeInTheDocument();
    expect(port.loadDirectory).toHaveBeenNthCalledWith(1, {
      ownerUserId: "owner-1",
      projectId: "project-1",
      path: "",
      forceRefresh: false,
    });
    expect(port.loadDirectory).toHaveBeenNthCalledWith(2, {
      ownerUserId: "owner-1",
      projectId: "project-1",
      path: "Sources",
      forceRefresh: false,
    });
  });

  it("loads file content using only a project-relative path", async () => {
    render(<ProjectDirectoryView ownerUserId="owner-1" port={stubPort()} projectId="project-1" />);
    fireEvent.click(await screen.findByRole("treeitem", { name: /README.md/ }));
    expect(await screen.findByLabelText("README.md 文件内容")).toHaveTextContent("# Hello");
    expect(screen.getByText("/README.md")).toBeInTheDocument();
    expect(screen.queryByText(/C:\\/)).not.toBeInTheDocument();
  });

  it("searches names and content then opens a matching line", async () => {
    const port = stubPort();
    render(<ProjectDirectoryView ownerUserId="owner-1" port={port} projectId="project-1" />);
    fireEvent.change(await screen.findByRole("searchbox", { name: "搜索项目文件" }), { target: { value: "hello" } });
    fireEvent.click(await screen.findByRole("button", { name: /README.md 第 1 行/ }));
    expect(await screen.findByLabelText("README.md 文件内容")).toHaveTextContent("# Hello");
    expect(port.searchEntries).toHaveBeenCalledWith(expect.objectContaining({ query: "hello", limit: 100 }));
    expect(port.searchContent).toHaveBeenCalledWith(expect.objectContaining({ query: "hello", limit: 200 }));
  });

  it("edits and saves a writable text file through the native port", async () => {
    const port = stubPort();
    render(<ProjectDirectoryView ownerUserId="owner-1" port={port} projectId="project-1" />);
    fireEvent.click(await screen.findByRole("treeitem", { name: /README.md/ }));
    fireEvent.click(await screen.findByRole("button", { name: "编辑" }));
    const editor = screen.getByRole("textbox", { name: "README.md 编辑内容" });
    fireEvent.change(editor, { target: { value: "# Updated" } });
    fireEvent.click(screen.getByRole("button", { name: "保存" }));
    await waitFor(() => expect(port.writeFile).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      path: "README.md",
      expectedRevision: "a".repeat(64),
      content: "# Updated",
    }));
    expect(await screen.findByLabelText("README.md 文件内容")).toHaveTextContent("# Updated");
  });

  it("creates a folder in the macOS-equivalent selected parent", async () => {
    const port = stubPort();
    render(<ProjectDirectoryView ownerUserId="owner-1" port={port} projectId="project-1" />);
    fireEvent.click(await screen.findByRole("treeitem", { name: /Sources/ }));
    fireEvent.click(screen.getByRole("button", { name: "新建项目文件或文件夹" }));
    fireEvent.click(screen.getByRole("button", { name: "文件夹" }));
    fireEvent.change(screen.getByRole("textbox", { name: "文件夹名称" }), { target: { value: "Components" } });
    fireEvent.click(screen.getByRole("button", { name: "创建" }));
    await waitFor(() => expect(port.createEntry).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      parentPath: "Sources",
      name: "Components",
      kind: "directory",
    }));
  });

  it("confirms deletion and sends the file revision", async () => {
    const port = stubPort();
    render(<ProjectDirectoryView ownerUserId="owner-1" port={port} projectId="project-1" />);
    fireEvent.click(await screen.findByRole("treeitem", { name: /README.md/ }));
    fireEvent.click(await screen.findByRole("button", { name: "删除" }));
    const dialog = screen.getByRole("alertdialog", { name: "删除“README.md”？" });
    expect(dialog).toBeInTheDocument();
    fireEvent.click(within(dialog).getByRole("button", { name: "删除" }));
    await waitFor(() => expect(port.deleteEntry).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      path: "README.md",
      kind: "file",
      expectedRevision: "a".repeat(64),
    }));
  });
});

describe("ProjectWorkspaceSurface", () => {
  beforeEach(() => {
    useProjectDirectoryStore.getState().reset();
    useProjectGitStore.getState().reset();
  });
  afterEach(cleanup);

  it("defaults to macOS messages and exposes admitted project tabs", async () => {
    render(
      <ProjectWorkspaceSurface
        directoryPort={stubPort()}
        gitPort={stubProjectGitPort()}
        runPort={stubProjectRunPort()}
        messages={<div>Conversation surface</div>}
        onProjectRemoved={vi.fn()}
        ownerUserId="owner-1"
        projectId="project-1"
        projectName="Project One"
      />,
    );
    expect(screen.getByText("Conversation surface")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "项目设置" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "项目目录" }));
    expect(await screen.findByRole("treeitem", { name: /Sources/ })).toBeInTheDocument();
  });

  it("opens a Git diff then returns to the matching project file", async () => {
    const directoryPort = stubPort();
    render(
      <ProjectWorkspaceSurface
        directoryPort={directoryPort}
        gitPort={stubProjectGitPort()}
        runPort={stubProjectRunPort()}
        messages={<div>Conversation surface</div>}
        onProjectRemoved={vi.fn()}
        ownerUserId="owner-1"
        projectId="project-1"
        projectName="Project One"
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "Git" }));
    const directoryTabs = await screen.findByRole("navigation", { name: "项目目录视图" });
    expect(within(directoryTabs).getByRole("button", { name: "Git" })).toHaveTextContent("Git 2");
    fireEvent.click(await screen.findByRole("button", { name: "README.md 暂存区差异" }));
    fireEvent.click(await screen.findByRole("button", { name: "打开文件" }));
    expect(await screen.findByLabelText("README.md 文件内容")).toHaveTextContent("# Hello");
    expect(directoryPort.readFile).toHaveBeenCalledWith({
      ownerUserId: "owner-1",
      projectId: "project-1",
      path: "README.md",
    });
  });
});

function stubPort(): ProjectDirectoryPort {
  return {
    loadDirectory: vi.fn(async (request) => request.path === "" ? rootListing() : sourcesListing()),
    readFile: vi.fn(async (request) => ({
      projectId: request.projectId,
      path: request.path,
      name: request.path.split("/").at(-1) ?? request.path,
      contentType: "text/markdown",
      contentEncoding: "utf8" as const,
      isBinary: false,
      isImage: false,
      isWritable: true,
      size: 7,
      modifiedAtUnixMs: 1,
      revision: "a".repeat(64),
      content: "# Hello",
    })),
    searchEntries: vi.fn(async (request) => ({
      projectId: request.projectId,
      query: request.query,
      matches: [rootListing().entries[1]],
      visitedDirectories: 1,
      visitedEntries: 2,
      isTruncated: false,
    })),
    searchContent: vi.fn(async (request) => ({
      projectId: request.projectId,
      query: request.query,
      matches: [{ path: "README.md", line: 1, column: 3, text: "# Hello" }],
      scannedFiles: 1,
      visitedEntries: 2,
      isTruncated: false,
    })),
    writeFile: vi.fn(async (request) => ({
      projectId: request.projectId,
      path: request.path,
      file: {
        projectId: request.projectId,
        path: request.path,
        name: request.path.split("/").at(-1) ?? request.path,
        contentType: "text/markdown",
        contentEncoding: "utf8" as const,
        isBinary: false,
        isImage: false,
        isWritable: true,
        size: request.content.length,
        modifiedAtUnixMs: 2,
        revision: "b".repeat(64),
        content: request.content,
      },
    })),
    createEntry: vi.fn(async (request) => ({
      projectId: request.projectId,
      path: request.parentPath ? `${request.parentPath}/${request.name}` : request.name,
      parentPath: request.parentPath,
      listing: request.parentPath === "Sources" ? sourcesListing() : rootListing(),
    })),
    deleteEntry: vi.fn(async (request) => ({
      projectId: request.projectId,
      path: request.path,
      parentPath: request.path.includes("/") ? request.path.slice(0, request.path.lastIndexOf("/")) : "",
      listing: rootListing(),
    })),
  };
}

function rootListing(): ProjectDirectoryListing {
  return {
    projectId: "project-1",
    path: "",
    parentPath: null,
    isWritable: true,
    entries: [
      {
        name: "Sources",
        path: "Sources",
        isDirectory: true,
        isWritable: true,
        size: null,
        modifiedAtUnixMs: 1,
      },
      {
        name: "README.md",
        path: "README.md",
        isDirectory: false,
        isWritable: true,
        size: 10,
        modifiedAtUnixMs: 1,
      },
    ],
    isTruncated: false,
  };
}

function sourcesListing(): ProjectDirectoryListing {
  return {
    projectId: "project-1",
    path: "Sources",
    parentPath: "",
    isWritable: true,
    entries: [{
      name: "App.ts",
      path: "Sources/App.ts",
      isDirectory: false,
      isWritable: true,
      size: 10,
      modifiedAtUnixMs: 1,
    }],
    isTruncated: false,
  };
}
