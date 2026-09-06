---
name: visual-observation-targeting
description: Locate visible macOS targets from fresh screenshots and convert screenshot pixels to global display points without confusing the AI cursor or physical pointer.
metadata:
  chatos.role: leaf
---

# Visual observation and targeting

Begin with a full-display `observe_screen` when the target, active display, or foreground app is uncertain. Once a stable work area is located, use the smallest useful global region while retaining enough surrounding UI to recognize the target and result.

The AI cursor is a blue-purple orbit reticle. Its cyan center is the only hotspot. Decorative rings, ticks, glow, trail, edge indicators, and the physical macOS arrow are not clickable locations.

## Coordinate workflow

1. Read screenshot pixel dimensions and returned display bounds.
2. Identify the target center in screenshot pixels.
3. Convert proportionally into global display points, accounting for crop origin and display scale.
4. Call `move_mouse` with the global point.
5. Inspect the returned screenshot and confirm the cyan center is inside the intended control.

A region must have positive dimensions and fit one active display. If the UI may have moved beyond the crop, return to a wider observation. Use PNG or native width only when thin lines or small text cannot be read reliably.

Read [coordinate examples](references/examples.md) before multi-display or cropped targeting.
