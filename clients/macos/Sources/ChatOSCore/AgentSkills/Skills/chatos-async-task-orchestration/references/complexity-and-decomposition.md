# Complexity and decomposition

Choose the smallest graph that preserves meaningful ownership and validation. Complexity is demonstrated by the work, not by prompt length.

Create a graph when the request has independently verifiable deliverables, distinct specialist/tool needs, useful parallel work, real staged dependencies, independent risk verification, or alternatives that later need synthesis. An unknown defect can justify separate diagnosis, fix, and verification when diagnosis determines the write scope and independent retesting materially strengthens acceptance, even though all stages serve one repaired feature. Prefer one Task for one tightly coupled artifact when the inputs and failure location are already known, the same minimal capabilities cover the work, and intermediate stages have no independent value.

For every proposed Task ask: What unique output does it own? Can it be accepted independently? Does it need a different capability, context, write boundary, or schedule? Can it run in parallel, or is its prerequisite real?

Merge Tasks with identical outputs and acceptance criteria. Split a catch-all Task when its outputs span multiple artifacts, disciplines, unrelated path ownership, or conflicting authority such as mutation and independent acceptance. Typical shapes include: focused known fix; investigation -> implementation -> verification; shared contract -> parallel platform implementations -> integration; parallel research -> synthesis; parallel campaign components -> package review.

Graph size is not a quality metric. Use as many Tasks as clear ownership requires, normally 2-8 for compound work; never manufacture Tasks to reach a count.
