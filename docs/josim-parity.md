# rflux-sim vs JoSIM Feature Parity

Status: initial baseline.

This document tracks what "fully aligned with JoSIM" means at the feature level.

Status legend:

- `done`: implemented in-tree and validated.
- `partial`: some related capability exists, but it is not JoSIM-level yet.
- `planned`: accepted target, not implemented yet.
- `out-of-scope`: intentionally not a parity target.

## Baseline statement

Current `rflux` is not yet a simulator peer to JoSIM.

Today the repository has:

- a dedicated `rflux-sim` crate skeleton,
- structural verification in `rflux-flow`,
- a direct `simulate_text(...)` API in Rust and Python,
- a direct `simulate_file(...)` API in Rust and Python,
- generated transient deck text,
- event-only simulation summaries,
- optional external executable invocation,
- explicit simulation mode selection through Rust, PyO3, and Python,
- minimal line-oriented result parsing.

Today the repository does not have:

- a native SPICE frontend,
- a native transient solver,
- Josephson device stamping,
- native waveform generation,
- direct simulation APIs beyond verification-oriented entry points.

## Parity matrix

| Capability area | JoSIM capability baseline | rflux current state | Status | Target milestone | Tracking note |
|-----------------|---------------------------|---------------------|--------|------------------|---------------|
| Dedicated simulator crate | CLI binary plus library distribution | `rflux-sim` crate exists, owns shared config/result types, exposes direct simulation entrypoints, and now contains a narrow native linear transient path | partial | 6.0 | Broaden solver and workflow coverage |
| Explicit backend selection | Distinct simulator execution path selection | `simulation_mode` is exposed across Rust, PyO3, and Python for `auto`, `event_only`, `external_josim`, `internal_transient`; the internal path now executes the current linear subset through both direct deck APIs and the linearized verification deck path | partial | 6.0 | Extend beyond the current subset |
| SPICE deck input | Reads SPICE syntax circuit netlists | Minimal direct deck parsing exists for `.param` and `.tran`, but no general SPICE frontend yet | partial | 6.1 | Start from JoSIM example subset |
| `.include` support | Supported | Relative `.include` expansion exists for file-backed decks | partial | 6.1 | Extend beyond the current file-backed relative include subset |
| `.param` support | Supported with expression parsing | Minimal scalar `.param` parsing exists with parameter references and engineering suffixes, including spaced and comma-separated assignment forms such as `a = 1p, b=2` | partial | 6.1 | Extend beyond the current small expression subset |
| Expression evaluation | Supported | Minimal scalar expression support exists for parameterized deck values, including scientific-notation numeric literals such as `1e-6` in current parser-supported expression contexts | partial | 6.1 | Share evaluator across parser/elaboration |
| `.subckt` hierarchy | Supported | Minimal `.subckt` / `.ends` flattening exists for `X...` instances, including nested parameter passthrough through instance assignments and clearer unsupported-syntax diagnostics | partial | 6.1 | Flattening is acceptable initially |
| Parameter override in subcircuits | Supported | Simple instance parameter override exists, including `params:` marker syntax and nested passthrough of `name=value` arguments | partial | 6.1 | Important for library cells |
| Transient analysis | JoSIM currently focuses on transient analysis | External hook still exists, and `internal_transient` now runs a real linear `R/L/C/V/I` timestep loop for a narrow subset, including linearized flow verification decks, adaptive internal substepping with a scaled absolute-plus-relative error norm, outer-step segmentation and aligned substep boundaries near supported source event breakpoints, left-limit step assembly across repeated-time `PWL(...)` discontinuities, and explicit non-convergence diagnostics when the current refinement cap is exhausted | partial | 6.2 | Need broader device coverage and JoSIM-class semantics |
| Phase/voltage analysis handling | JoSIM documents phase-mode conventions and output handling | No native analog mode support | planned | 6.2 | Need documented `rflux` result semantics |
| `.tran` options such as print-window control | Supported by JoSIM | Minimal `.tran` parsing exists including optional print-window fields, trailing `uic`, application of supported `.ic` startup values, acceptance of keyword-style `.tran` fields such as `tstep=... tstop=...` (including spaced `=` forms), and acceptance of minimal `.nodeset` startup hints for the internal linear subset; in this subset, `.tran/.ic/.nodeset` control cards accept case-insensitive spellings, and `.ic`/`.nodeset` assignments accept both compact `V(n)=...` and spaced/comma forms such as `V(n) = ...`; minimal `.option`/`.options` parsing is accepted (case-insensitive card names), `.option reltol/abstol` feeds the internal nonlinear residual gate (including `rel`/`relerr` and `abs`/`abserr` aliases), `.option itl/itl1/itl4/maxiter/maxiters` maps to internal Newton max-iteration control, and these currently supported option keys also accept space-separated pairs such as `reltol 1e-4` | partial | 6.1 / 6.2 | Prioritize only options used in benchmarks |
| Passive devices (R/L/C) | Supported | Native internal transient currently solves a narrow linear `R/L/C` subset | partial | 6.2 | Required before JJ support |
| Independent sources | Supported | Native internal transient currently solves independent DC `V/I` sources in the current subset, including transient decks that carry a trailing `AC ...` clause on otherwise-DC sources and `DC=...` / `DC = ...` keyword assignment forms | partial | 6.2 | Expand source families further |
| Pulse / PWL / sinusoidal sources | Supported | Native internal transient now supports minimal `PULSE(...)` including one-shot, periodic, finite-cycle (`Ncycles`) forms, and keyword-style pulse argument forms such as `v1=... v2=... td=... tr=... tf=... pw=... per=...` (including optional spaces around `=`), plus `PWL(...)`, `EXP(...)`, and `SIN(...)` subsets including optional SIN damping and phase; `SIN(...)` now also accepts keyword-style forms such as `vo=... va=... freq=... td=... theta=... phase=...`, and `EXP(...)` accepts keyword-style forms such as `v1=... v2=... td1=... tau1=... td2=... tau2=...`; supported function-source names are case-insensitive, may be followed by optional whitespace before `(`, argument lists may use commas or whitespace separators, and repeated-time `PWL(...)` points are interpreted as step discontinuities | partial | 6.1 / 6.2 | Needed for example deck parity |
| File/custom waveform sources | Supported in JoSIM | Native internal transient now supports a minimal file-driven waveform bootstrap through `PWL(file=...)` / `PWL(path=...)`, loading time-value pairs from text files (comma or whitespace separated) into the existing PWL source path with explicit diagnostics for unreadable or malformed waveform files; broader JoSIM-class custom source forms remain pending | partial | 6.3 | Expand beyond the current PWL-file subset |
| Josephson junction (RCSJ) | Core JoSIM capability | Native internal transient now accepts a minimal nonlinear JJ subset (`J ... [model] icrit= rn= [cj=]`) solved through the iterative nonlinear stamp path with history-integrated phase and `sin(phi)` supercurrent linearization, including spaced `name = value` assignment forms; minimal `.model <name> jj(...)` / `.model <name> jj ...` cards are now parsed for `icrit`/`rn`/`cj` defaults and can be referenced by `J` instances (with per-instance assignment override), but this is still a bootstrap rather than full JoSIM RCSJ semantics | partial | 6.3 | Expand to full phase-dynamics RCSJ and JoSIM-compatible model semantics |
| Pi-junction support | Supported in JoSIM releases | Native internal transient now supports a minimal pi-junction bootstrap for the JJ subset through `pi` flag parsing (`pi=0/1` or boolean-like values) on both `J` instance assignments and `.model ... jj(...)` defaults, implemented as a critical-current sign inversion in the current nonlinear stamp path; broader JoSIM-class pi-junction semantics remain pending | partial | 6.3 | Expand beyond the current sign-flip bootstrap |
| Multi-harmonic CPR | Supported in JoSIM releases | Native internal transient now supports a minimal second-harmonic CPR bootstrap in the JJ subset (`icrit2`/`ic2`/`cp2` on `J` instances and `.model ... jj(...)` defaults), contributing an added `I2*sin(2phi)` term and Jacobian linearization in the current nonlinear stamp path; higher-order/JoSIM-class CPR model coverage remains pending | partial | 6.3 | Extend beyond second harmonic and align with JoSIM model semantics |
| Transmission line support | Supported | Native internal transient now supports a minimal `T` subset with `z0` plus finite `td` through a bounded delayed surrogate stamp (and exact coupling when `td=0`), and now also supports optional attenuation controls (`loss=` in [0,1] amplitude ratio loss or `alpha=` in 1/s with attenuation `exp(-alpha*td)`) plus spaced `name = value` assignment forms, while full JoSIM-class delay semantics are still pending | partial | 6.3 | Replace the current surrogate with full delay-line semantics |
| Mutual inductance (`K`) | Supported | Native internal transient now supports a minimal linear `K L1 L2 k` subset for previously declared or forward-declared inductor names, with bounded coefficient parsing for bare/keyword forms (including spaced `coupling = ...`) and coupled inductor companion-model stamping | partial | 6.3 | Expand beyond the current linear mutual-inductor subset |
| Thermal/noise options | Supported in JoSIM releases | Native internal transient now supports a minimal stochastic-noise bootstrap via `.option tnoise=...` / `.option noise=...` (non-negative sigma), with optional `.option temp=...` and `.option tnom=...` controls used for a temperature-ratio noise scale factor; seeded deterministic perturbations are applied on independent source stamps plus a minimal resistor-noise contribution in the current subset, but this remains a lightweight compatibility path rather than full thermal/noise device modeling | partial | 6.3 | Expand toward device- and temperature-aware JoSIM-class noise semantics |
| Deterministic RNG seed | Supported via `.option seed=` | Internal transient parses `.option seed=...` (including spaced `seed = ...`, parameter expressions, comma-separated `.option` lists, and multi-line `.option` decks where non-seed lines do not clear an earlier seed), surfaces it in result metadata, and now uses it to drive reproducible internal noise sampling when noise options are enabled | partial | 6.3 | Extend seeded stochastic paths beyond the current minimal source-perturbation bootstrap |
| CSV output | Supported | Native internal transient now emits sampled waveform CSV output and also accepts `.option csvout=...` / `.option waveform=...` / `.option waveform_path=...` / `.option raw_file=...` to write the CSV artifact to an explicit destination path (with validation for empty paths) | partial | 6.5 | Keep schema stable and document long-term format contract |
| Raw waveform output | Supported | Native internal transient now produces waveform artifacts directly (default temp-file path plus optional option-controlled explicit path), while external passthrough remains available | partial | 6.5 | Broaden artifact formats and compatibility with external tooling |
| Stdout/script summary | Supported | Result parsing now accepts a standardized `SIM_*` summary key set (`SIM_EVENTS`, `SIM_RESULT`, `SIM_WAVEFORM_PATH`, `SIM_VIOLATIONS`, `SIM_WORST_DELAY_PS`, detail lines) while preserving backward compatibility with existing `RFLOW_*` and legacy aliases | partial | 6.5 | Publish and lock the summary contract across CLI/script integrations |
| Error diagnostics | Mature parser/runtime diagnostics | Limited to external command failures and flow checks | partial | 6.1-6.5 | Track parser vs solver diagnostics separately |
| Cross-platform executable use | JoSIM ships on major platforms | External hook works on Windows/Unix in tests | partial | 6.5 | Internal backend should be platform-neutral |
| Embeddable library API | `libjosim` exists | No simulator library API yet | planned | 6.5 | Rust API is the parity target, not C++ ABI |
| Python embedding | JoSIM has Python-adjacent ecosystem support | PyO3/Python now expose direct `simulate_text(...)`, but solver/front-end scope is still narrow | partial | 6.4 / 6.5 | Expand from deck text API to broader simulator workflows |
| Benchmark evidence | JoSIM has examples and mature regression history | Initial in-repo phase-6 smoke benchmark decks now exist for `K`, delayed `T`, and minimal `JJ` internal-transient runs, a Python waveform comparison helper (`python/scripts/compare_internal_external_waveforms.py`) now provides first-pass internal-vs-external CSV error metrics (including optional JSON output), a summary helper (`python/scripts/summarize_waveform_compare_results.py`) can aggregate threshold results into markdown and fail on missing/failed decks, an optional pytest integration (`python/tests/test_waveform_compare.py`) runs thresholded checks when `josim` is available (auto-skip otherwise), and compare utility behavior is covered by `python/tests/test_waveform_compare_utils.py` plus `python/tests/test_waveform_compare_summary_utils.py`; full automated JoSIM numeric-correlation gating is still pending | partial | 6.6 | Add automated JoSIM-vs-rflux numeric correlation checks |

## Already reusable from current rflux code

These areas are not simulator parity by themselves, but they are useful foundations:

- `rflux-ir` already provides a graph-level representation that can seed deck generation and result annotation.
- `rflux-flow` already knows how to generate a transient deck artifact for verification-oriented use.
- current Python tests already define a minimal structured result shape for delays, violations, waveform paths, and backend labels.
- Phase 5 characterization already benefits from simulation-derived arc and violation details, which provides a concrete consumer for `rflux-sim`.

## Explicit non-parity targets

The following should not be used to judge phase completion:

| Item | Reason |
|------|--------|
| CMake or C++ implementation style | `rflux` is a Rust-first project |
| Binary name or installation layout identical to JoSIM | operationally useful compatibility is enough |
| Plotting helper scripts identical to JoSIM | output data compatibility matters more than plotting tool parity |
| Full SPICE coverage on day one | only the SFQ-relevant JoSIM subset is needed first |

## Benchmark set to assemble

Minimum benchmark coverage for progress reviews:

| Bucket | Purpose | Closure signal |
|--------|---------|----------------|
| Passive sanity decks | Validate parser and transient engine basics | waveform and numeric sanity |
| Simple JJ cell decks | Validate RCSJ implementation | delay/phase agreement against JoSIM |
| PTL / transmission decks | Validate interconnect handling | waveform integrity and timing agreement |
| Hierarchical subcircuit decks | Validate `.subckt` and parameter propagation | matching elaborated behavior |
| Characterization decks from `rflux-flow` | Validate integration with existing flow consumers | same extracted delays and violation summaries |

## Progress update rules

When updating this document:

1. Change status only when there is code and a validation command or test to point to.
2. Prefer `partial` over `done` unless native `rflux-sim` behavior exists in-tree.
3. Record gaps in result fidelity separately from parser coverage gaps.
4. Do not count external JoSIM invocation through `verify_layout(...)` as native parity.

## Correlation evidence workflow (manual CI)

Use this workflow when collecting milestone-6.6 parity evidence without changing default CI behavior.

1. Ensure the runner has an executable JoSIM command available.
2. Open GitHub Actions and run workflow `CI` with `workflow_dispatch`.
3. Set input `run_external_waveform_compare=true`.
4. Optionally set `josim_command` to a full path or wrapper command.
5. Confirm job `waveform-compare-optional` runs and archive generated compare artifacts.

For local reproduction of the same compare path:

```bash
RFLOW_JOSIM_COMMAND=josim uv run pytest python/tests/test_waveform_compare.py -rs
uv run python python/scripts/summarize_waveform_compare_results.py --result-dir python/tests/benchmarks/phase6 --markdown-output python/tests/benchmarks/phase6/waveform_compare_summary.md
```

Use the summary markdown plus `*.compare.json` files as review evidence when updating parity status rows.