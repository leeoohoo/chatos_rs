---
name: web-design-visual-system
description: Establish distinctive Web Design Studio art direction through composition, design tokens, typography, color, spacing, imagery, material, and restrained states.
metadata:
  chatos.role: leaf
---

# Visual system

Translate the brief into one explicit visual concept before choosing or polishing components. Record the intended audience impression, primary focal point, composition idea, type personality, palette role, imagery strategy, spacing rhythm, and one or two recognizable signature details. Establish global colors, radii, typography, and spacing rhythm through tokens when the decisions should repeat.

Use clear hierarchy, aligned edges, controlled contrast, purposeful imagery, deliberate whitespace, varied visual rhythm, and consistent component states. Supported style fields include background, fill/stroke, padding, radius, shadow, blur, backdrop blur, opacity, rotation, scale, overflow, typography, media fit/position, and blend mode. Use `customCss` only when a dedicated field cannot express the treatment.

Put only state differences into hover, active, and focus styles. Focus must remain visible. Avoid arbitrary gradients, excessive glass effects, identical card grids for every brief, or decoration that competes with content.

Judge the system from rendered screenshots, not token consistency alone. It must create a clear focal path, readable type hierarchy, intentional density changes, coherent image treatment, and a character appropriate to the product and audience. If the result could be relabeled for an unrelated product without visible redesign, it is too generic and needs another visual Step.

Do not start from admin-dashboard conventions unless the brief requires them. Do not let Ant Design, Chakra UI, shadcn/ui, or another library's defaults become the art direction. Apply interaction states only after the static visual hierarchy works.

Read [visual examples](references/examples.md) before a new visual direction.
