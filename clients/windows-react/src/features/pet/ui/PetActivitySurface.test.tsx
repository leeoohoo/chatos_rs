import { act, cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { PetPresentation } from "../domain/petPresentation";
import type { PetPresentationPort } from "../ports/PetPresentationPort";
import { PetActivitySurface } from "./PetActivitySurface";

describe("PetActivitySurface", () => {
  afterEach(cleanup);

  it("renders only the safe localized native projection", async () => {
    let listener!: (presentation: PetPresentation) => void;
    const port: PetPresentationPort = {
      refresh: vi.fn(),
      subscribe: vi.fn(async (next) => { listener = next; return () => undefined; }),
    };
    render(<PetActivitySurface port={port} />);
    await act(async () => undefined);
    act(() => listener({
      language: "zh-CN",
      animationState: "waiting",
      primaryActivity: { source: "task_execution", kind: "waiting_for_user", detailCode: "waiting_user", updatedAtUnixMs: 1 },
      activeWorkCount: 0,
      attentionCount: 1,
    }));
    expect(screen.getByText("任务 · 等待你的输入")).toBeVisible();
    expect(screen.getByText("等待你的回复")).toBeVisible();
  });
});
