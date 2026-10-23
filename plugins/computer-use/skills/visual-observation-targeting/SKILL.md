---
name: visual-observation-targeting
description: Locate visible macOS or Windows targets from fresh screenshots and convert screenshot pixels with current metadata without confusing the AI cursor or physical pointer.
metadata:
  chatos.role: leaf
---

# Visual observation and targeting

Begin with a full-display `observe_screen` when the target, active display, or foreground app is uncertain. Once a stable work area is located, use the smallest useful global region while retaining enough surrounding UI to recognize the target and result.

The AI cursor is a blue-purple orbit reticle. Its cyan center is the only hotspot. Decorative rings, ticks, glow, trail, edge indicators, and the physical system pointer are not clickable locations.

## Coordinate workflow

1. Read screenshot pixel dimensions and returned display bounds.
2. Identify the target center in screenshot pixels.
3. Convert into the platform's global coordinates using that screenshot's `captureRegionGlobal`, `globalPointsPerScreenshotPixelX`, and `globalPointsPerScreenshotPixelY`.
4. Call `move_mouse` with the global point.
5. Inspect the returned screenshot and confirm the cyan center is inside the intended control.

A region must have positive dimensions and fit one active display. macOS coordinates are global display points; Windows coordinates are virtual-desktop physical pixels and may be negative on displays left of or above the primary display. Always trust the current screenshot metadata rather than assuming a Retina/DPI scale. If the UI may have moved beyond the crop, return to a wider observation. Use PNG or native width only when thin lines or small text cannot be read reliably.

Read [coordinate examples](references/examples.md) before multi-display or cropped targeting.
