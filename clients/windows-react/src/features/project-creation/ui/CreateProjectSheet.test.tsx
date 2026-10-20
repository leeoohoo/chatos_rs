import { cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ProjectCreationPort } from "../ports/ProjectCreationPort";
import { CreateProjectSheet } from "./CreateProjectSheet";

function port(): ProjectCreationPort {
  return {
    listProjectTypes: vi.fn().mockResolvedValue([{
      key: "software_development",
      label: "通用软件开发",
      description: "无法进一步归类的软件项目。",
      categoryKey: "software_product",
      categoryLabel: "软件与数字产品",
    }]),
    pickProjectDirectory: vi.fn().mockResolvedValue({
      selectionId: "selection-1",
      directoryName: "repo",
    }),
    discardProjectDirectorySelection: vi.fn().mockResolvedValue(undefined),
    createLocalProject: vi.fn().mockResolvedValue({
      id: "project-1",
      name: "Local",
      revision: 1,
      projectTypeKey: "software_development",
    }),
  };
}

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("CreateProjectSheet", () => {
  it("uses the native folder grant and publishes success after requesting dismissal", async () => {
    const adapter = port();
    const order: string[] = [];
    render(
      <CreateProjectSheet
        ownerUserId="alice"
        port={adapter}
        onDismiss={() => order.push("dismiss")}
        onCreated={() => order.push("created")}
      />,
    );
    await waitFor(() => expect(screen.getByRole("combobox")).toBeEnabled());
    fireEvent.click(screen.getByRole("button", { name: "选择文件夹…" }));
    await screen.findByText("repo");
    const name = screen.getByRole("textbox", { name: "项目名称" });
    fireEvent.change(name, { target: { value: " Local " } });
    fireEvent.click(screen.getByRole("button", { name: "创建项目" }));
    await waitFor(() => expect(adapter.createLocalProject).toHaveBeenCalledWith(expect.objectContaining({
      ownerUserId: "alice",
      selectionId: "selection-1",
      name: "Local",
    })));
    await waitFor(() => expect(order).toEqual(["dismiss", "created"]));
  });

  it("does not make an absolute path part of the rendered or submitted contract", async () => {
    const adapter = port();
    const { container } = render(
      <CreateProjectSheet ownerUserId="alice" port={adapter} onDismiss={() => undefined} onCreated={() => undefined} />,
    );
    fireEvent.click(await screen.findByRole("button", { name: "选择文件夹…" }));
    await screen.findByText("repo");
    expect(container.textContent).not.toContain("C:\\");
    expect(JSON.stringify(vi.mocked(adapter.pickProjectDirectory).mock.results)).not.toContain("absolutePath");
  });
});
