import { describe, expect, it } from "vitest";
import { resolveWorkspaceIndex } from "./workspaceIndex";

describe("workspace index projection", () => {
  it("never invents a project from an orphan Host conversation", () => {
    const snapshot = resolveWorkspaceIndex(
      [{ id: "local", name: "Local", revision: 1, projectTypeKey: "software_development" }],
      {
        contacts: [],
        conversations: [
          { id: "remote", title: "Remote", projectId: "remote-only", updatedAtUnixMs: 20 },
          { id: "local-chat", title: "Local chat", projectId: "local", updatedAtUnixMs: 10 },
        ],
      },
    );

    expect(snapshot.projects.map((project) => project.id)).toEqual(["local"]);
    expect(snapshot.projects[0].conversationId).toBe("local-chat");
  });

  it("chooses the newest conversation for each source-owned resource", () => {
    const snapshot = resolveWorkspaceIndex([], {
      contacts: [{ id: "jiguli", agentId: "jiguli", name: "叽咕狸", status: "active" }],
      conversations: [
        { id: "old", title: "Old", contactId: "jiguli", updatedAtUnixMs: 10 },
        { id: "new", title: "New", contactId: "jiguli", updatedAtUnixMs: 20 },
      ],
    });

    expect(snapshot.contacts[0].conversationId).toBe("new");
    expect(snapshot.contacts[0].subtitle).toBe("New");
  });
});

