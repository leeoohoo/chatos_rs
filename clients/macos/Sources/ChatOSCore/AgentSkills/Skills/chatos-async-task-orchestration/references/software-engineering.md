# Software engineering scenarios

Split feature work when architecture or API decisions gate multiple implementations, when platform paths can proceed independently, or when integration verification needs independent ownership. Keep a small cohesive change in one Task if one worker can implement and verify it safely.

Expected outputs should name changed modules, migrations, tests, artifacts, and compatibility notes. Acceptance criteria should include observable behavior plus proportionate tests, builds, type checks, or manual verification.

For uncertain failures use `diagnose -> fix -> verify`; skip a ceremonial diagnosis Task when evidence is already sufficient. Review Tasks must inspect actual manifests, source entry points, representative implementations, and tests or CI rather than relying on README text.

Give parallel coders genuinely disjoint possible write sets, not just disjoint stated objectives. If two coders may modify one file—even different functions—prefer sequential prerequisites or a single write owner with read-only proposals from others. Treat shared contracts, lockfiles, central registries, migrations, generated files, installs and broad formatting as conflict points. Establish uncertain write scope before dispatch, or serialize it; see [graph-patterns-and-dependencies.md](graph-patterns-and-dependencies.md). Use an integration Task when separately valid changes can still fail together. Verification must depend on the implementation version it evaluates and run checks itself rather than repeat the implementer's summary.
