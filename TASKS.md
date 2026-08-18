# Tasks

Current executable work that may be picked up asynchronously. Keep entries short; remove completed work. `STATUS.md`/`CAPABILITIES.md` owns implemented truth and `ROADMAP.md`/`DIRECTION.md` owns future direction.

## T-0001 - Protect Markdown authority graph

- Area: documentation
- Next: Extend the existing --docs verification lane with a fast mechanical Markdown authority check: required current authority pages, local links, concrete repository routes/aliases, and README/STATUS/TESTING ownership relationships. Keep rustdoc as its own proof and avoid brittle prose linting or duplicating subsystem semantics.
- Paths: `TESTING.md`
- Verify: `python ci.py gate --docs && python ../tools/check_standards.py`
- Depends: none
- Basis: `755b8567d2380bfc1bbe41ae9a61e0446629d190`
- Reviewed: `2026-08-18T03:37:13Z`
