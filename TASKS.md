# Tasks

Current executable work that may be picked up asynchronously. Keep entries short; remove completed work. `STATUS.md`/`CAPABILITIES.md` owns implemented truth and `ROADMAP.md`/`DIRECTION.md` owns future direction.

## T-0001 - Add cold-agent task routing

- Area: documentation
- Next: Add a compact task-routing map near the repository entry point so common changes route to the owning source subsystem, relevant authority, and first proving lane without reading the full STATUS or TECHNICAL_DESIGN. Keep the map at ownership/change-class level and do not duplicate formulas, physics contracts, or mutable implementation detail.
- Paths: `README.md`
- Verify: `python ci.py gate --docs && python ../tools/check_standards.py`
- Depends: none
- Basis: `7c820acac47f3f3306598dbdb19196b323b74518`
- Reviewed: `2026-08-18T02:48:25Z`
