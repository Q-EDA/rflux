# Alignment Scorecard Weekly Report Template

Use this file as a weekly status report for Phase A and later alignment tracking.

## 1. Report metadata

```md
Week window:
Report date:
Owner:
Git SHA (optional):
```

## 2. Domain score snapshot

| Domain | Planned max | Current pass score | Delta vs last week | Notes |
|---|---:|---:|---:|---|
| Yosys-aligned core flow | 25 |  |  |  |
| Quaigh-aligned optimization | 20 |  |  |  |
| JoSIM-aligned simulation and correlation | 30 |  |  |  |
| Productization and release governance | 25 |  |  |  |
| Total | 100 |  |  |  |

## 3. MUST item status

Rule: any failed MUST item means alignment gate is not satisfied.

| Item ID | Status (PASS/FAIL/WAIVED) | Evidence command or job | Evidence link/path | Owner | ETA if FAIL |
|---|---|---|---|---|---|
| Y-01 |  |  |  |  |  |
| Y-02 |  |  |  |  |  |
| Q-01 |  |  |  |  |  |
| Q-02 |  |  |  |  |  |
| J-01 |  |  |  |  |  |
| J-02 |  |  |  |  |  |
| J-03 |  |  |  |  |  |
| P-01 |  |  |  |  |  |
| P-02 |  |  |  |  |  |
| P-03 |  |  |  |  |  |

## 4. SHOULD item status

| Item ID | Status (PASS/FAIL/WAIVED) | Evidence command or job | Evidence link/path | Owner | ETA if FAIL |
|---|---|---|---|---|---|
| Y-03 |  |  |  |  |  |
| Q-03 |  |  |  |  |  |
| J-04 |  |  |  |  |  |
| P-04 |  |  |  |  |  |

## 5. Gate decision

```md
Alignment gate result: pass / fail
Blocking MUST items:
Waivers approved by:
```

## 6. Top risks and next actions

```md
Risk 1:
Action 1:

Risk 2:
Action 2:

Risk 3:
Action 3:
```

## 7. References

- Scorecard: `docs/alignment-scorecard.md`
- Full plan: `docs/full-alignment-plan.md`
- PR template: `.github/PULL_REQUEST_TEMPLATE.md`
- Baseline status artifact: `waveform-baseline-status/linux.json` and `waveform-baseline-status/windows.json`
