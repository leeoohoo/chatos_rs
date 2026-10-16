# Complexity and decomposition

Choose the smallest graph that preserves meaningful ownership and validation. Complexity is demonstrated by the work, not by prompt length.

Create a graph when the request has independently verifiable deliverables, distinct specialist/tool needs, useful parallel work, real staged dependencies, independent risk verification, or alternatives that later need synthesis. Prefer one Task for one tightly coupled artifact when the same worker and capabilities cover it and intermediate stages have no independent value.

For every proposed Task ask: What unique output does it own? Can it be accepted independently? Does it need a different capability, context, write boundary, or schedule? Can it run in parallel, or is its prerequisite real?

Merge Tasks with identical outputs and acceptance criteria. Split a catch-all Task when its outputs span multiple artifacts, disciplines, or unrelated path ownership. Typical shapes include: focused fix; investigation -> implementation -> verification; shared contract -> parallel platform implementations -> integration; parallel research -> synthesis; parallel campaign components -> package review.

Graph size is not a quality metric. Use as many Tasks as clear ownership requires, normally 2-8 for compound work; never manufacture Tasks to reach a count.
