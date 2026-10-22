import { useEffect, useState } from "react";
import { idlePetPresentation, type PetPresentation } from "../domain/petPresentation";
import type { PetPresentationPort } from "../ports/PetPresentationPort";

export function usePetPresentation(port: PetPresentationPort) {
  const [presentation, setPresentation] = useState<PetPresentation>(idlePetPresentation);

  useEffect(() => {
    let disposed = false;
    let unsubscribe: (() => void) | undefined;
    void port.subscribe((next) => {
      if (!disposed) setPresentation(next);
    }).then((stop) => {
      if (disposed) stop();
      else unsubscribe = stop;
    });
    return () => {
      disposed = true;
      unsubscribe?.();
    };
  }, [port]);

  return presentation;
}
