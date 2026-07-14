# Local Visualization Builder

Use this Skill to create a self-contained HTML visualization, simulator, chart, comparison, or interactive explainer in the authorized local workspace.

Use `write_visualization_html` to write the artifact locally. Produce accessible semantic HTML, responsive CSS, clear labels, and deterministic JavaScript. Keep the page self-contained: do not load remote scripts, fonts, analytics, trackers, or network resources. The Local Connector adds a restrictive Content Security Policy before saving the file.

After writing the artifact, report its workspace-relative path and summarize what the user can interact with. Do not claim it was created unless the tool returned a successful file result.
