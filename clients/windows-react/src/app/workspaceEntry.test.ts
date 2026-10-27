import { describe, expect, it } from "vitest";
import { selectAuthenticatedWorkspace } from "./workspaceEntry";

const snapshot = {
  projects: [{ id: "project-1", title: "Alpha", subtitle: null, conversationId: null }],
  contacts: [{ id: "contact-1", title: "叽咕狸", subtitle: null, conversationId: null }],
};

describe("selectAuthenticatedWorkspace", () => {
  it("opens the first project after authentication", () => {
    expect(selectAuthenticatedWorkspace({ kind: "workbench" }, "workspace", "user-1", "user-1", snapshot))
      .toEqual({ kind: "project", id: "project-1" });
  });

  it("falls back to the first contact when no project exists", () => {
    expect(selectAuthenticatedWorkspace({ kind: "workbench" }, "workspace", "user-1", "user-1", { projects: [], contacts: snapshot.contacts }))
      .toEqual({ kind: "contact", id: "contact-1" });
  });

  it("never overrides settings or an explicit selection", () => {
    const project = { kind: "project" as const, id: "already-selected" };
    expect(selectAuthenticatedWorkspace({ kind: "workbench" }, "general-settings", "user-1", "user-1", snapshot))
      .toEqual({ kind: "workbench" });
    expect(selectAuthenticatedWorkspace(project, "workspace", "user-1", "user-1", snapshot))
      .toEqual(project);
    expect(selectAuthenticatedWorkspace({ kind: "workbench" }, "workspace", "user-1", "user-2", snapshot))
      .toEqual({ kind: "workbench" });
  });
});
