import type { PetPresentationPort } from "../ports/PetPresentationPort";

export function startPetPresentationPolling(port: PetPresentationPort, ownerUserId: string) {
  let stopped = false;
  let timer: number | undefined;

  const poll = async () => {
    let delay = 2_000;
    try {
      await port.refresh(ownerUserId);
    } catch {
      delay = 4_000;
    }
    if (!stopped) timer = window.setTimeout(() => void poll(), delay);
  };
  void poll();

  return () => {
    stopped = true;
    if (timer !== undefined) window.clearTimeout(timer);
  };
}
