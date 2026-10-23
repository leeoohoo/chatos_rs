# Coordinate examples

## Positive

A crop begins at global coordinate `(400, 200)`, covers `800 × 600` global units, and is encoded as `1600 × 1200` pixels. Its returned scale is `0.5` global units per screenshot pixel. A target at pixel `(600, 400)` maps to global coordinate `(700, 400)`. Move there and verify the cyan center visually. Use the same formula with negative Windows virtual-desktop origins; do not clamp them to zero.

## Negative

Click the numeric location immediately because the conversion seems correct. The window may have moved or the crop may use a different scale.

If the reticle lies outside a crop, the screenshot may show a directional edge marker. Enlarge or reposition the crop; never treat the marker as the cursor hotspot.
