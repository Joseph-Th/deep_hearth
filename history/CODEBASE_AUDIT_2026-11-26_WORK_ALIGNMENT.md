# Codebase audit: local work alignment

## Corrected defect

The text-play `begin` adapter aligned the real clock before dispatching the ordinary work command. Its sensory interruption checks did not guarantee that the actor remained in the original voxel. At night, neutral control could settle momentum in a neighboring cell and silently reinterpret the requested local task there. The adapter now compares its original and aligned voxel before dispatch. The actual elapsed time and body movement remain authoritative; an interrupted request does not admit work or consume a work-start frame.

## Reproduction and evidence

- Seed 24318, a trusted test save with the clock at simulation tick 18001 (night) and the independent opening colony removed to isolate the motion/sensory case. Ordinary commands `e 1`, `wait 10`, `e 3`, `begin gather 1 100` move the actor from (8,2,8) to (9,2,8). The pre-fix regression failed because the command started gathering 100 g of berries at the destination and advanced an additional control frame.
- The corrected regression asserts that the position changes, no work is admitted, the interruption names the location change, and the entire resulting serialized state matches an ordinary `wait boundary` from the same starting state. Existing alignment tests retain same-location admission and interruption on pickup at the boundary.
- `cargo test --locked -j 4 --bin text-play`: 126 passed, including the regression.
- `python ci.py quick`: all four checks passed.
- Rebuilt executable and ran the same command sequence using an isolated temporary save loaded through the actual CLI. At control tick 1296144, the transcript reported 58 elapsed alignment control ticks and explicitly interrupted admission because the location changed; no gathering-start receipt or active work appeared. The temporary save was removed.

## Other reviewed boundaries

Reviewed fluid placement and custody, natural-spring movement and footing, production availability and timed-work scheduling, player resource projections, visibility/attention, and control-span stepping. Generic fluid stores do not yet have the material-backed vessel installation contract needed for a universal footing rule; natural springs already have a terrain support contract. No speculative change to those capabilities was made. No verified obsolete compatibility path was found in the reviewed areas. This is a targeted audit, not proof of the absence of additional defects. Existing unrelated terrain/custody modifications were preserved and excluded from the commit. The executable and tests ran against the combined working tree.
