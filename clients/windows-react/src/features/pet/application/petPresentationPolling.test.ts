import { describe, expect, it, vi } from "vitest";
import type { PetPresentationPort } from "../ports/PetPresentationPort";
import { startPetPresentationPolling } from "./petPresentationPolling";

describe("pet presentation polling", () => {
  it("does not overlap refreshes and stops after teardown", async () => {
    vi.useFakeTimers();
    let release!: () => void;
    const refresh = vi.fn(async () => new Promise<void>((resolve) => { release = resolve; })) as unknown as PetPresentationPort["refresh"];
    const port = { refresh, subscribe: vi.fn() } as unknown as PetPresentationPort;
    const stop = startPetPresentationPolling(port, "owner-1");
    await vi.advanceTimersByTimeAsync(0);
    expect(refresh).toHaveBeenCalledTimes(1);
    await vi.advanceTimersByTimeAsync(10_000);
    expect(refresh).toHaveBeenCalledTimes(1);
    release();
    await vi.advanceTimersByTimeAsync(2_000);
    expect(refresh).toHaveBeenCalledTimes(2);
    stop();
    release();
    await vi.advanceTimersByTimeAsync(10_000);
    expect(refresh).toHaveBeenCalledTimes(2);
    vi.useRealTimers();
  });
});
