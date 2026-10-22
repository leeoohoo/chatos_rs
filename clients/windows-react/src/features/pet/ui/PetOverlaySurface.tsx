import { useEffect, useMemo, useState } from "react";
import { usePetPresentation } from "../application/usePetPresentation";
import type { PetAnimationState } from "../domain/petPresentation";
import type { PetPresentationPort } from "../ports/PetPresentationPort";
import "./pet.css";

const columns = 8;

export function PetOverlaySurface({ port }: { port: PetPresentationPort }) {
  const presentation = usePetPresentation(port);
  const [frame, setFrame] = useState(0);
  const [assetFailed, setAssetFailed] = useState(false);
  const assetUrl = useMemo(() => (
    navigator.userAgent.includes("Windows")
      ? "http://chatos-pet.localhost/fengtuan-atlas"
      : "chatos-pet://localhost/fengtuan-atlas"
  ), []);
  const animation = animationConfiguration(presentation.animationState);

  useEffect(() => {
    setFrame(0);
    if (window.matchMedia("(prefers-reduced-motion: reduce)").matches) return undefined;
    const timer = window.setInterval(() => setFrame((current) => (current + 1) % animation.frameCount), animation.duration);
    return () => window.clearInterval(timer);
  }, [animation.duration, animation.frameCount, presentation.animationState]);

  const x = `${(frame / (columns - 1)) * 100}%`;
  const y = `${(animation.row / 10) * 100}%`;
  return (
    <main aria-label="ChatOS pet is idle" className="pet-overlay" data-tauri-drag-region>
      {assetFailed ? (
        <div aria-hidden="true" className="pet-overlay__fallback" data-tauri-drag-region>🦊</div>
      ) : (
        <img
          alt=""
          className="pet-overlay__preload"
          onError={() => setAssetFailed(true)}
          src={assetUrl}
        />
      )}
      {!assetFailed && <div aria-hidden="true" className="pet-overlay__sprite" data-tauri-drag-region style={{ backgroundImage: `url(${assetUrl})`, backgroundPosition: `${x} ${y}` }} />}
    </main>
  );
}

function animationConfiguration(state: PetAnimationState) {
  switch (state) {
    case "running": return { row: 7, frameCount: 6, duration: 200 };
    case "review": return { row: 8, frameCount: 6, duration: 250 };
    case "waiting": return { row: 6, frameCount: 6, duration: 500 };
    case "succeeded": return { row: 4, frameCount: 5, duration: 500 };
    case "failed": return { row: 5, frameCount: 8, duration: 500 };
    default: return { row: 0, frameCount: 7, duration: 1_000 };
  }
}
