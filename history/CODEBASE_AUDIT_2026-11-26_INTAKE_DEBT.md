# Codebase audit: intake debt and documentation checks

Date: 2026-11-26

## Confirmed defects

- Direct food and drink reserve-target planning subtracted outstanding locomotion costs from the current reserve with a zero fallback, discarding the unpaid portion. The survival settlement owner still charged the full cost. A regression using an actual jump, depleted reserves, selected physical food/water, and canonical simulation settlement reproduced both shortfalls: 49.3938 kJ against 50 kJ and 259.849 mL against 260 mL. The planners retain and price the unpaid cost using checked arithmetic; public projections without pending movement retain their existing contract.
- Current-prose history checks also applied to dated evidence under `history/`, contradicting the documentation authority distinction. Only that prose check now exempts the root history directory. The checker regression preserves restrictions elsewhere, including nested directories called history.

## Verification

- Focused reproduction: the first setup attempt used a walking sample without whole-microliter hydration expenditure; the corrected jump setup reproduced both target failures before the fix.
- `python ci.py quick`: all four checks passed.
- `python ci.py audit --all`: 2,857 core tests and 187 gameplay tests passed; 202.4 seconds total. Gameplay variation `0x8E65BD44B81BF995` (settlement, ordinary exact-local after disclosed bootstrap).
- Rebuilt text-play executable, seed 24318: walked to berries, gathered 500 g, ate from the selected stack, walked to wild nuts, gathered 500 g. Final carried matter 989.721 g (489.721 g berries and 500 g nuts), consistent with 10.279 g eaten. Ordinary play sample did not reproduce the exact exhausted-reserve setup; the focused simulation regression does.

## Scope and remaining uncertainty

Reviewed survival settlement and projections, movement expenditure, timed work, terrain support and fluid custody, respawn, environment assessment, and text-play action paths. Generic fluid-store footing warrants further investigation: some existing capability fixtures deliberately accept locations without ordinary world support, and ordinary vessel acquisition is not present. No parallel footing policy was introduced or fixture contract changed in this pass.

The full audit ran against the combined working tree, which contained twelve pre-existing terrain-related modifications. Those files are excluded from the audit commits. This is a risk-directed review, not proof of exhaustive correctness.
