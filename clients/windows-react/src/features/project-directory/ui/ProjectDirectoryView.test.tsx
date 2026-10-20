import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useProjectDirectoryStore } from "../application/projectDirectoryStore";
import type { ProjectDirectoryListing } from "../domain/projectDirectory";
import type { ProjectDirectoryPort } from "../ports/ProjectDirectoryPort";
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

  it("shows only relative file details and does not claim preview support", async () => {
    render(<ProjectDirectoryView ownerUserId="owner-1" port={stubPort()} projectId="project-1" />);
    fireEvent.click(await screen.findByRole("treeitem", { name: /README.md/ }));
    expect(screen.getByText("/README.md")).toBeInTheDocument();
    expect(screen.queryByText(/C:\\/)).not.toBeInTheDocument();
    expect(screen.getByText(/文件预览与编辑将在后续独立切片/)).toBeInTheDocument();
  });
});

describe("ProjectWorkspaceSurface", () => {
  beforeEach(() => useProjectDirectoryStore.getState().reset());
  afterEach(cleanup);

  it("defaults to macOS messages and exposes the admitted directory tab", async () => {
    render(
      <ProjectWorkspaceSurface
        directoryPort={stubPort()}
        messages={<div>Conversation surface</div>}
        ownerUserId="owner-1"
        projectId="project-1"
      />,
    );
    expect(screen.getByText("Conversation surface")).toBeInTheDocument();
    expect(screen.getByRole("button", { name: "项目设置" })).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "项目目录" }));
    expect(await screen.findByRole("treeitem", { name: /Sources/ })).toBeInTheDocument();
  });
});

function stubPort(): ProjectDirectoryPort {
  return {
    loadDirectory: vi.fn(async (request) => request.path === "" ? rootListing() : sourcesListing()),
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
