# Coordinates and reliable targeting

Use this reference when mapping a screenshot target to `move_mouse`, working across Retina or multiple displays, or diagnosing coordinate uncertainty.

## Coordinate spaces

Every observation provides:

- `captureRegionGlobal`: the screenshot rectangle in global macOS display points.
- `screenshotPixelWidth` and `screenshotPixelHeight`: the returned image dimensions in pixels.
- `globalPointsPerScreenshotPixelX` and `globalPointsPerScreenshotPixelY`: the scale used to map image pixels to global points.
- `selectedDisplay.frame` and `displays[].frame`: display rectangles in global points.
- `globalDesktopBounds`: the union of all display rectangles.
- `virtualCursorGlobal`: the click hotspot in global points.
- `cursorScreenshotPixel`: that hotspot in screenshot pixels when it is inside the capture, otherwise `null`.

The visible MCP cursor is a blue-purple AI orbit reticle rather than a system arrow. Both coordinate fields refer to the cyan point at the exact center of the reticle. Do not compensate for an arrow-tip offset. The orbit arcs, four ticks, glow, and movement trail are decorative; the physical macOS arrow can remain visible elsewhere and is not represented by `virtualCursorGlobal`.

Both coordinate spaces use a top-left origin: x increases to the right and y increases downward. A display positioned left of or above the main display can have negative global x or y values.

Do not use native Retina pixels directly for mouse movement. `move_mouse` requires global display points.

## Pixel-to-global conversion

For a target at screenshot pixel `(imageX, imageY)`:

```text
globalX = captureRegionGlobal.x
        + imageX * globalPointsPerScreenshotPixelX

globalY = captureRegionGlobal.y
        + imageY * globalPointsPerScreenshotPixelY
```

Use metadata from the same screenshot in which the target was identified. Do not reuse a scale or region from an earlier screenshot after changing display, region, or `max_image_width`.

Example:

```text
captureRegionGlobal = { x: 240, y: 90, width: 1000, height: 700 }
globalPointsPerScreenshotPixelX = 0.8
globalPointsPerScreenshotPixelY = 0.8
target screenshot pixel = (625, 300)

global target = (240 + 625 * 0.8, 90 + 300 * 0.8)
              = (740, 330)
```

Move to `(740, 330)`, then confirm the returned `cursorScreenshotPixel` and cyan hotspot against the control before clicking.

## Choosing the hotspot

- Aim for the interior of a visible control, away from borders, resize handles, adjacent controls, text selection edges, and disclosure arrows unless the arrow itself is the target.
- For text fields, choose an empty interior area or the intended insertion location.
- For small icons, increase screenshot width or use a tighter region before targeting.
- If an overlay, tooltip, reticle ring, or cursor trail obscures the target, move away, observe again, then approach the newly measured point.
- The cyan center hotspot is the actual click coordinate. The surrounding AI orbit rings, reticle ticks, glow, and trail are decorative and intentionally do not resemble the macOS system arrow.

## Region workflow

1. Observe a full display and identify a stable global rectangle around the relevant window or panel.
2. Observe that rectangle as `region` for higher useful detail.
3. Convert the target from that crop using the crop's own `captureRegionGlobal` and scale.
4. Call `move_mouse` with the target and the same region so the returned screenshot includes the hotspot.
5. Verify, then call the action with the same region while the cursor remains inside it.
6. If the action changes layout or leaves the region, expand or discard the crop and reacquire the target.

Regions cannot span displays. If a proposed region crosses a display boundary, use a full screenshot of the relevant display or separate per-display observations.

## Multiple displays

- Inspect `displays[]` rather than assuming the main display contains the target.
- `display_id` selects a display. When omitted, selection may follow the requested region, then the virtual cursor, then the main display.
- Preserve negative coordinates exactly when moving on a display left of or above the main display.
- Before clicking on another display, perform a fresh observation of that display, derive the global coordinate from that observation, and verify the move result there.

## Common failure patterns

### Pointer is outside a crop

For read-only observation, use the edge indicator to understand direction. Before an action with that region, call `move_mouse` to a verified point inside it. If the target itself is uncertain, enlarge the crop first.

### Click lands beside the target

Do not add an arbitrary offset. Re-read `captureRegionGlobal` and both scale values from the target screenshot, recompute the point, and inspect the cyan hotspot returned by `move_mouse`.

### Screenshot changed after measurement

Treat the coordinate as stale. Re-observe and recompute after scrolling, resizing, switching apps, opening menus, or any animation that changes layout.

### No visible response after action

Inspect the returned screenshot for focus, disabled controls, an intervening modal, loading state, or a wrong foreground app. Reacquire the target rather than repeating blind clicks.

### The exact result is not visible

Use a purpose-built read-only check if available. Otherwise navigate only as far as authorized to expose visible evidence. If no reliable evidence is available, report the action as unverified rather than successful.
