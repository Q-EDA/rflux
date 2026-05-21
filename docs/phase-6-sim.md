# Phase 6 - rflux-sim and JoSIM Parity

Status: started.

This phase creates a real `rflux-sim` module and drives it toward functional parity with JoSIM where that parity strengthens `rflux` as an SFQ EDA stack.

The target is not a blind clone of JoSIM internals. The target is:

- JoSIM-compatible simulation input and result expectations for SFQ use cases.
- A pure-Rust simulator core that fits the existing `rflux-ir` / `rflux-flow` / PyO3 architecture.
- Continued support for `rflux`'s mixed-flow strategy: precise simulation for local electrical truth, event/timing abstractions for larger design loops.

See also: [josim-parity.md](./josim-parity.md)

## Current baseline

Current repository state now has a substantial `rflux-sim` slice with a native internal-transient bootstrap:

- `crates/sim` is now a workspace member and exposes shared simulation config/result types plus the current generated-deck runner.
- `rflux-flow::verify_layout(...)` still provides structural checks plus a lightweight `simulate_hook(...)`.
- explicit `simulation_mode` selection now flows through Rust, PyO3, and Python for `auto`, `event_only`, `external_josim`, and `internal_transient`.
- `rflux-sim` now also exposes a direct `simulate_text(...)` path with a growing SPICE-subset frontend (`.param`, `.tran`, `.option`, `.ic/.nodeset`, source families, `.model` JJ subset, `.subckt` flattening).
- `rflux-sim` now also exposes `simulate_file(...)` with relative `.include` expansion for file-backed decks.
- `rflux-sim` now supports a minimal `.subckt` / `.ends` flattening path with `X...` instantiation and simple instance parameter override.
- The current hook can:
  - generate a transient deck text artifact,
  - emit an internal `event_only` summary when no external simulator is configured,
  - run a native internal-transient path for a defined subset (including passive elements, supported sources, transmission-line bootstrap, and minimal nonlinear JJ bootstrap),
  - emit native waveform CSV artifacts (default temp path or option-controlled output path),
  - invoke an external executable with the generated deck path,
  - parse a line-oriented result contract with standardized `SIM_*` summary keys (`SIM_EVENTS`, `SIM_RESULT`, `SIM_WAVEFORM_PATH`, `SIM_VIOLATIONS`, `SIM_WORST_DELAY_PS`, detail lines) while preserving compatibility with existing `RFLOW_*` and legacy aliases.
- The current implementation still cannot:
  - parse general SPICE decks,
  - solve the full JoSIM/SFQ analog device space beyond the current bootstrap subsets,
  - provide full JoSIM-class JJ/noise/phase-model semantics,
  - guarantee long-term stable output schema/versioning across all integration surfaces,
  - cover JoSIM-class transient deck semantics end-to-end.

That means the project has completed the crate-boundary bootstrap, but has not yet earned any claim of native JoSIM-class simulation parity.

## Exit criteria

Phase 6 should only be considered complete when all of the following are true:

- `crates/sim` exists and is a first-class workspace crate.
- `rflux-sim` can execute a defined SPICE-subset transient analysis without requiring an external simulator.
- `rflux-sim` can ingest the JoSIM-oriented benchmark deck subset documented in [josim-parity.md](./josim-parity.md).
- `rflux-flow` can choose between:
  - internal transient simulation,
  - external JoSIM execution,
  - event-only abstraction.
- Python exposes the same backend selection and returns structured simulation results.
- Regression decks and benchmark comparisons are checked into the repository.

## Non-goals for this phase

- Full C++ ABI compatibility with `libjosim`.
- Reproducing JoSIM's build system, plotting helpers, or implementation details.
- Replacing high-level event-driven or STA-based analysis in flows where full transient simulation is unnecessary.
- Supporting all of SPICE before the SFQ-relevant subset is solid.

## Workstreams

## 6.0 Scope freeze and architecture slice

Goal: define what "fully aligned with JoSIM" means for `rflux` before code lands.

Deliverables:

- Freeze the parity matrix in [josim-parity.md](./josim-parity.md) with one status per row.
- Add `crates/sim` to the workspace with placeholder public API only.
- Define three simulator backends in one shared config model:
  - `event_only`
  - `internal_transient`
  - `external_josim`
- Define one canonical simulation report model shared across Rust, PyO3, and Python.

Current progress in 6.0:

- done: `crates/sim` crate exists and owns shared simulation config/result types.
- done: explicit simulation mode selection is wired through Rust, PyO3, and Python.
- done: direct `simulate_text(...)` entry exists in Rust and Python, so simulation is no longer only reachable through verification-oriented flow APIs.
- done: `internal_transient` now reaches a native linear transient path for a narrow `R/C/V/I` subset.
- pending: remove remaining verification-hook framing from the public API surface.

Validation:

- `cargo check -p rflux-flow -p rflux-py -p rflux-sim`
- Python smoke test for backend selection round-trip

Exit gate:

- No simulation result fields remain "verification-hook-only".

## 6.1 Netlist frontend and deck compatibility

Goal: accept the subset of JoSIM/SPICE syntax needed by SFQ transient benchmarks.

Must-have input features:

- `.include`
- `.param`
- expression evaluation with engineering suffixes
- `.subckt` / `.ends`
- instance parameter override
- transient command parsing (`.tran`)
- basic options parsing needed by later milestones
- source waveform parsing needed by benchmark decks

Recommended scope order:

1. parser and AST
2. elaboration / parameter substitution
3. subcircuit flattening or hierarchical execution model
4. deck normalization and diagnostics

Current progress in 6.1:

- done: minimal `.param` parsing with scalar expressions and engineering suffixes.
- done: minimal `.param` parsing now also accepts spaced and comma-separated assignment forms (for example `a = 1p, b=2`) in the current parser subset.
- done: minimal `.tran` parsing including optional print-window fields and keyword-style field forms such as `tstep=... tstop=...` / `tstart=... tprint=...`, including spaced `=` assignments and trailing `uic`.
- done: key deck control cards now accept case-insensitive spellings in the current parser paths (including `.title/.param/.include/.tran/.subckt/.ends` and internal-transient `.ic/.nodeset/.option(s)` handling).
- done: relative `.include` expansion through file-backed deck entry points.
- done: minimal `.subckt` / `.ends` flattening with pin mapping and simple parameter override.
- done: `params:` marker syntax in `.subckt` headers and `X...` instances.
- done: nested subcircuit parameter passthrough now rewrites `name=value` instance assignments during elaboration.
- done: unsupported nested `.subckt`, mismatched `.ends`, unsupported in-body control cards, and malformed extra instance tokens now fail with explicit `.subckt` diagnostics.
- pending: broader syntax coverage and deeper subcircuit semantics.

Validation:

- parser golden tests for real JoSIM example decks
- round-trip normalization snapshots
- failure tests for unsupported syntax with actionable errors

Exit gate:

- benchmark decks in the chosen subset load without external preprocessing.

## 6.2 Internal transient kernel bootstrap

Goal: replace the current placeholder simulation hook with a real transient engine.

Current bootstrap progress:

- done: `internal_transient` now executes a minimal linear transient path for flat or flattened decks containing `R`, `L`, `C`, `V`, and `I` elements.
- done: the same internal linear subset now also accepts minimal mutual inductance cards `K L1 L2 k` (including forward-declared `K` lines), using coupled inductor companion-model stamping for linked inductor branch currents and accepting bare or keyword coefficient forms (including spaced `coupling = ...`).
- done: the transmission-line bootstrap now accepts both zero-delay and finite-delay `T` cards through a bounded delayed surrogate discretization (exact coupling at `td=0`, timestamped-history delayed interpolation for `td>0`) and optional attenuation controls (`loss=` / `alpha=`) in the supported subset; in this subset, `loss` is an amplitude-ratio loss in [0,1], while `alpha` is interpreted in 1/s with attenuation `exp(-alpha*td)`.
- done: the transmission-line and JJ bootstrap parsers now also accept spaced `name = value` assignment forms for their supported parameter sets.
- done: the internal step solve path now routes through an explicit nonlinear-iteration interface (`config + residual gate + nonlinear stamp hook`) and already uses it for a minimal nonlinear JJ subset (`J ... [model] icrit= rn= [cj=]`) as the first RCSJ bootstrap, with history-integrated phase and `sin(phi)` supercurrent linearization; minimal `.model <name> jj(...)` / `.model <name> jj ...` defaults for `icrit`/`rn`/`cj` are now accepted and can be referenced by `J` instances with per-instance parameter override.
- done: minimal pi-junction support is now available in the JJ bootstrap subset through `pi` flag parsing on both `J` instance assignments and `.model ... jj(...)` defaults (`pi=0/1` plus boolean-like forms), currently implemented as a critical-current sign inversion in the nonlinear stamp path.
- done: minimal multi-harmonic CPR bootstrap support is now available in the JJ subset through a second-harmonic current term (`icrit2`/`ic2`/`cp2`) on both `J` instance assignments and `.model ... jj(...)` defaults, contributing `I2*sin(2phi)` plus its Jacobian term in the nonlinear stamp path.
- done: minimal custom waveform/file-driven source support is now available through `PWL(file=...)` / `PWL(path=...)`, which loads time-value pairs from text files (comma or whitespace separated) into the current PWL source path with explicit read/format diagnostics.
- done: minimal thermal/noise bootstrap support is now available through `.option tnoise=...` / `.option noise=...` (non-negative sigma) with deterministic seed-driven sampling and optional `.option temp=...` / `.option tnom=...` temperature scaling; when combined with `.option seed=...`, internal source perturbation noise is reproducible across runs, and the current subset also includes a lightweight resistor-noise contribution.
- done: independent `PULSE(...)`, minimal `PWL(...)`, `EXP(...)`, and minimal `SIN(...)` sources are now supported for the current narrow source subset; `PULSE(...)` accepts one-shot, periodic, finite-cycle (`Ncycles`), and keyword-style pulse argument forms (`v1=...`, `v2=...`, `td=...`, `tr=...`, `tf=...`, `pw=...`, `per=...`) including optional spaces around `=`, `SIN(...)` accepts both positional forms and keyword-style forms such as `vo=...`, `va=...`, `freq=...`, `td=...`, `theta=...`, and `phase=...`, and `EXP(...)` now accepts keyword-style forms such as `v1=...`, `v2=...`, `td1=...`, `tau1=...`, `td2=...`, and `tau2=...`; supported function-source names are parsed case-insensitively, may be followed by optional whitespace before `(`, their argument lists may use either commas or plain whitespace separators, repeated-time `PWL(...)` points are treated as right-continuous step breakpoints, and plain transient `DC` sources now tolerate a trailing `AC ...` clause by ignoring it.
- done: plain transient `DC` sources now also accept `DC=...` and spaced `DC = ...` keyword assignment forms, including the common `AC=...` / `AC = ...` trailer variants that are ignored by the current internal subset.
- done: `.tran` parsing now accepts the common trailing `uic` flag, and minimal `.ic` voltage initial conditions are applied for the supported linear subset when `uic` is present (including spaced assignment forms such as `V(out) = 1`).
- done: minimal `.nodeset V(node)=...` cards are now accepted and used as startup hints for the supported internal linear subset, including spaced/comma assignment forms.
- done: minimal `.option seed=...` parsing is now accepted for internal-transient decks (including spaced `seed = ...`, parameterized expressions, and comma-separated option lists), and multi-line `.option` decks now preserve the latest parsed seed when later option lines do not restate it; the parsed seed is surfaced in internal result metadata for correlation bookkeeping.
- done: minimal `.option reltol=...` / `.option abstol=...` parsing is now accepted (including spaced/comma assignment forms plus `rel`/`relerr` and `abs`/`abserr` aliases), the currently supported option keys also accept space-separated `name value` pairs (for example `reltol 1e-4`), and these tolerances are wired into the internal nonlinear Newton residual normalization gate for JJ/delayed-line iteration control.
- done: minimal `.option`/`.options` parsing is now accepted with case-insensitive card names; iteration controls `.option itl=...` / `.option itl1=...` / `.option itl4=...` / `.option maxiter(s)=...` are accepted (including spaced/comma assignment forms) and are wired into the internal nonlinear Newton max-iteration control, with explicit validation diagnostics for invalid values.
- done: the current scalar expression path now accepts scientific-notation numeric literals (for example `1e-6`) in parser-supported contexts, which removes a common deck compatibility friction point for `.param` and `.option` values.
- done: unsupported-device and unsupported-source cases now return explicit internal-transient unavailability reasons instead of a generic placeholder.
- done: `rflux-flow` now emits a linearized verification deck that can reach the native internal transient path for simple route-level checks.
- done: minimal waveform capture now writes sampled internal-transient voltages to a CSV artifact.
- done: waveform CSV output now also supports explicit destination control through `.option csvout=...` (plus aliases `.option waveform=...` / `.option waveform_path=...` / `.option raw_file=...`) in the internal-transient path.
- done: the internal solver now uses bounded adaptive substepping with a coarse-vs-refined error check for dynamic-source and inductor cases instead of relying on a single coarse outer step, using a scaled absolute-plus-relative error norm, splitting each outer transient step at netlist source breakpoints, adding extra refinement pressure when the current step crosses a source event breakpoint (including `SIN` delay onset), aligning substep boundaries to supported source breakpoints within the current step, and using left-limit source values during step assembly for repeated-time `PWL(...)` discontinuities.
- done: when adaptive substepping still exceeds the current error threshold at the configured refinement cap, the internal solver now reports an explicit non-convergence diagnostic instead of silently accepting the last refined state.
- done: `PULSE(...)` source parsing now accepts both periodic and 6-argument one-shot forms in the supported subset.
- pending: broader convergence control and broader device/source coverage.

Must-have kernel capabilities:

- transient analysis only, matching JoSIM's current primary analysis mode
- sparse system assembly abstraction
- timestep loop with stable integration strategy
- initial condition handling
- convergence and diagnostics reporting
- waveform capture API independent from output format

Design constraints:

- keep the simulator core pure Rust when feasible
- do not couple the engine to Python or CLI concerns
- allow wasm-safe parsing/report structures even if the solver later gains native-only acceleration paths

Validation:

- unit tests on linear passive fragments
- JJ-free transient deck comparisons against known reference outputs
- deterministic replay tests for identical seeds/options
- in-repo phase-6 smoke benchmark decks (`K`, delayed `T`, minimal `JJ`) that execute through `simulate_file(..., internal_transient)`
- waveform numeric comparison helper via `python/scripts/compare_internal_external_waveforms.py` to compare internal CSV traces against external simulator CSV traces on shared node columns
- optional pytest wrapper via `python/tests/test_waveform_compare.py` with per-deck thresholds in `python/tests/benchmarks/phase6/waveform_thresholds.json`; this test auto-skips when `josim` is unavailable and can be pointed to a custom binary via `RFLOW_JOSIM_COMMAND`

Exit gate:

- internal backend can simulate a JJ-free linear `R/C/V/I` deck subset, including pulse-drive cases, and return structured summaries plus sampled waveform CSV output.

## 6.3 SFQ device and interconnect models

Goal: make the internal engine electrically meaningful for JoSIM-class SFQ circuits.

Must-have models for parity-driven progress:

- RCSJ Josephson junction model
- resistor / capacitor / inductor support required by SFQ decks
- independent source families used in SFQ examples
- transmission line support required by PTL-style interconnect studies
- mutual inductance support when used by target benchmarks

Follow-on models:

- pi-junction support
- multi-harmonic CPR support
- deterministic and thermal noise options
- custom waveform/file-driven sources

Validation:

- model-level regression tests
- numeric comparison on published or repository-selected reference decks
- targeted comparisons against JoSIM on representative SFQ cells

Exit gate:

- representative JJ-based decks run internally with bounded error against JoSIM reference outputs.

## 6.4 Backend integration into rflux flow

Goal: make simulation a first-class flow stage instead of a verification side hook.

Deliverables:

- move `simulate_hook(...)` responsibility behind `rflux-sim`
- `rflux-flow` chooses backend by explicit config rather than ad hoc external command presence
- `characterize_compound_cell(...)` and `verify_layout(...)` consume the unified simulator API
- timing and characterization flows can request:
  - waveform paths,
  - delay extraction,
  - violation summaries,
  - endpoint-resolved details

Validation:

- Rust tests for backend dispatch
- Python tests for `verify_layout(...)` and characterization across all three backends

Exit gate:

- there is no remaining duplicate simulation result shaping logic in `rflux-flow` or `python/rflux`.

## 6.5 Output, CLI, and Python usability

Goal: make the simulator usable for both direct simulation and embedding.

Deliverables:

- stable Rust API in `rflux-sim`
- PyO3 bindings for deck loading, backend config, execution, and result retrieval
- Python facade helpers for common workflows
- native output support for:
  - CSV-like tabular export
  - raw waveform artifact export or an equivalent documented format
  - stdout summary mode for scripting

Optional but useful:

- a root CLI entry or dedicated subcommand for transient runs
- deck normalization / lint mode

Validation:

- end-to-end CLI tests
- Python notebook/script smoke workflow

Exit gate:

- users can run `rflux` simulation without going through `verify_layout(...)`.

## 6.6 Correlation, benchmarks, and release gate

Goal: prove parity with evidence rather than feature claims.

Deliverables:

- benchmark deck suite drawn from:
  - JoSIM examples,
  - `rflux` characterization decks,
  - small SFQ macro/regression cases
- comparison reports covering:
  - pass/fail convergence behavior,
  - key waveform alignment,
  - delay and violation extraction agreement,
  - runtime and memory envelope
- documented unsupported cases and acceptable error bars

Validation:

- `uv run cargo test --workspace`
- `uv run pytest`
- dedicated benchmark command or script stored under `python/scripts/` or a Rust test harness
- manual CI correlation run via `.github/workflows/ci.yml` `workflow_dispatch` input `run_external_waveform_compare=true` (optional `josim_command`), which triggers `waveform-compare-optional` without affecting default push/PR CI

Exit gate:

- parity status in [josim-parity.md](./josim-parity.md) is updated with measured evidence.

## Milestone checklist

Use this section as the live progress tracker.

| Milestone | Status | Owner | Notes |
|-----------|--------|-------|-------|
| 6.0 Scope freeze and API slice | in progress | TBD | `crates/sim` exists; backend config/result types moved out of flow |
| 6.1 Deck parser and elaboration | in progress | TBD | `.param`, `.tran`, `.include`, `.subckt`, and nested parameter passthrough are in place |
| 6.2 Internal transient bootstrap | in progress | TBD | Passive/source subset expanded with adaptive stepping plus bootstrap `K`/`T`/minimal `JJ` coverage |
| 6.3 JJ and SFQ device models | not started | TBD | Correlate against JoSIM |
| 6.4 Flow integration | not started | TBD | Replace current hook layering |
| 6.5 CLI/Python usability | not started | TBD | Direct simulator entrypoint |
| 6.6 Correlation and release gate | in progress | TBD | Compare script, JSON summary helper, and optional manual CI gate are landed |

## Risk register

| Risk | Why it matters | Mitigation |
|------|----------------|------------|
| Trying to support all SPICE syntax too early | Parser scope will dominate the phase and stall solver work | Freeze a JoSIM benchmark subset first |
| Treating current external-hook tests as simulator coverage | Creates false confidence and weakens milestone discipline | Keep hook coverage and internal-solver coverage reported separately |
| Tight coupling to `rflux-flow` | Makes standalone simulation and future reuse harder | Land `rflux-sim` as an independent crate with narrow integration points |
| Numeric mismatch against JoSIM on JJ-heavy circuits | "Feature complete" without correlation is not useful | Gate milestone closure on benchmark comparisons |
| Overfitting to JoSIM output text | Makes internal engine design brittle | Normalize everything into structured report types |

## Suggested task order

1. Add `crates/sim` and shared report/config types.
2. Land parser + elaboration for a frozen JoSIM subset.
3. Land passive/source transient engine.
4. Add JJ and transmission-line models.
5. Replace `verify_layout(...)` hook plumbing with backend dispatch.
6. Add benchmark correlation and update the parity matrix.