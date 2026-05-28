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
  - run a native internal-transient path for a defined subset (including passive elements, supported sources, transmission-line bootstrap, and minimal nonlinear JJ bootstrap with native CPR support through six coefficients),
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
- done: minimal `.lib <path> [section]` support now resolves file-backed library paths and, when a section is provided, extracts only the matching `.lib ...` / `.endl` block (including `.endl` section-name consistency checks) in the current parser subset, while tolerating common comma-delimited token forms, keyword-style section tokens (`section=...` / `sec=...`), and inline semicolon comment tails on section tokens.
- done: minimal `.subckt` / `.ends` flattening with pin mapping and simple parameter override.
- done: `params:` marker syntax in `.subckt` headers and `X...` instances.
- done: nested subcircuit parameter passthrough now rewrites `name=value` instance assignments during elaboration.
- done: nested `.subckt` definitions are now accepted in the current flattening path, while same-scope duplicate definitions, mismatched `.ends`, unsupported in-body control cards, and malformed extra instance tokens still fail with explicit `.subckt` diagnostics.
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

- done: `internal_transient` now executes a minimal linear transient path for flat or flattened decks containing `R`, `L`, `C`, `V`, and `I` elements; in the current passive subset, `R/L/C` values may now be provided either positionally or through minimal keyword-style forms such as `resistance=...`, `capacitance = ...`, `inductance=...`, or space-separated pairs like `resistance 50`.
- done: the same internal linear subset now also accepts minimal mutual inductance cards `K L1 L2 k` (including forward-declared `K` lines), using coupled inductor companion-model stamping for linked inductor branch currents and accepting bare or keyword coefficient forms, including `coupling=...`, spaced `coupling = ...`, and space-separated pairs such as `coupling 0.9` or `k 0.9`.
- done: the transmission-line bootstrap now accepts both zero-delay and finite-delay `T` cards through a bounded delayed surrogate discretization (exact coupling at `td=0`, timestamped-history delayed interpolation for `td>0`) and optional attenuation controls (`loss=` / `alpha=`) in the supported subset; in this subset, `loss` is an amplitude-ratio loss in [0,1], while `alpha` is interpreted in 1/s with attenuation `exp(-alpha*td)`, transmission-line keyword parameters may be provided as `name=value`, `name = value`, space-separated pairs such as `z0 50 td 1p loss 0.3`, comma-separated keyword cards such as `z0=50, td=1p, loss=0.1`, or common aliases such as `zo`, `tau`, and `atten`, and finite `td` now contributes both to internal substep pressure and to startup-window breakpoint / boundary alignment, including delayed arrival of source-side breakpoint events propagated through connected single-hop, chained, and parallel-path `T` elements along simple arrival paths; the current breakpoint filters now suppress both extra propagated chain arrivals and bare intermediate or dangling `td` breakpoints when a line segment does not terminate at observed non-transmission endpoints, so coarse `.tran` steps refine more aggressively around delayed-line dynamics without over-segmenting unobserved internal hops.
- done: the transmission-line and JJ bootstrap parsers now also accept spaced `name = value` assignment forms for their supported parameter sets.
- done: the internal step solve path now routes through an explicit nonlinear-iteration interface (`config + residual gate + nonlinear stamp hook`) and already uses it for a minimal nonlinear JJ subset (`J ... [model] icrit= rn= [cj=]`) as the first RCSJ bootstrap, with history-integrated phase and `sin(phi)` supercurrent linearization; minimal `.model <name> jj(...)` / `.model <name> jj ...` defaults for `icrit`/`rn`/`cj` are now accepted and can be referenced by `J` instances either positionally or through keyword-style `model=...` / `modelname=...` references, with per-instance parameter override, and current JJ instance/model override parsing now also tolerates comma-separated assignment tokens such as `icrit=..., rn=..., cj=...` plus space-separated pair forms such as `icrit 0.5m rn 20` and `model jjmod rn 25`.
- done: minimal pi-junction support is now available in the JJ bootstrap subset through `pi` flag parsing on both `J` instance assignments and `.model ... jj(...)` defaults (`pi=0`, `pi=1`, and other nonzero integer forms such as `pi=-1` or `pi=2`, plus boolean-like forms on the native internal path), currently implemented as a critical-current sign inversion in the nonlinear stamp path; the external JoSIM compatibility path now also normalizes nonzero integer `pi` spellings into the same sign-flip behavior instead of warninging on them.
- done: minimal multi-harmonic CPR bootstrap support is now available in the JJ subset through second-, third-, fourth-, fifth-, and sixth-harmonic current terms (`icrit2`/`ic2`/`cp2`, `icrit3`/`ic3`/`cp3`, `icrit4`/`ic4`/`cp4`, `icrit5`/`ic5`/`cp5`, and `icrit6`/`ic6`/`cp6`) on `J` instance assignments, plus direct native `cpr={...}` parsing through the sixth harmonic on both `.model ... jj(...)` defaults and inline `J...` instance parameters, contributing `I2*sin(2phi)` / `I3*sin(3phi)` / `I4*sin(4phi)` / `I5*sin(5phi)` / `I6*sin(6phi)` plus their Jacobian terms in the nonlinear stamp path.
- done: minimal custom waveform/file-driven source support is now available through `PWL(file=...)` / `PWL(path=...)`, which loads time-value pairs from text files (comma or whitespace separated) into the current PWL source path with explicit read/format diagnostics.
- done: the external `external_josim` compatibility path now also performs the minimum deck translation needed by the real Windows `josim-cli.exe` workflow used in local parity probes: it requests voltage-mode output (`-a 0`), strips/normalizes unsupported control syntax (including `.tran ... uic` on the external JoSIM path), inlines `PWL(file=...)` sources, rewrites JJ element prefixes from `J...` to JoSIM's `B...` form, rewrites inline JJ instance parameters into JoSIM-compatible synthetic `.model ... jj(...)` cards so minimal `icrit`/`rn`/`cj` semantics do not silently fall back to JoSIM defaults, rewrites keyword-style JJ model references such as `model=jjmod` into JoSIM's positional B-device form, now also merges the current benchmark-safe subset of inline JJ overrides (`icrit`/`rn`/`cj`) on top of referenced model defaults into synthetic JoSIM model cards, preserves native `.model ... jj(... cpr={...})` syntax and inline `J... cpr={...}` syntax through the supported six-coefficient subset, lowers supported second-, third-, fourth-, fifth-, and sixth-harmonic JJ semantics (`icrit2`/`ic2`/`cp2`, `icrit3`/`ic3`/`cp3`, `icrit4`/`ic4`/`cp4`, `icrit5`/`ic5`/`cp5`, and `icrit6`/`ic6`/`cp6`) into native JoSIM `CPR={...}` model arguments, including pure second-harmonic `CPR={0,1}` emission when no primary `icrit` term is present, pure third-harmonic `CPR={0,0,1}` emission when the third harmonic is the only available basis current, pure fourth-harmonic `CPR={0,0,0,1}` emission when the fourth harmonic is the only available basis current, pure fifth-harmonic `CPR={0,0,0,0,1}` emission when the fifth harmonic is the only available basis current, and pure sixth-harmonic `CPR={0,0,0,0,0,1}` emission when the sixth harmonic is the only available basis current, rewrites JJ model-card capacitance aliases from `cj` to JoSIM's `cap`, and rewrites mutual-inductor `coupling=...` keyword arguments into the positional syntax JoSIM expects.
- done: minimal thermal/noise bootstrap support is now available through `.option tnoise=...` / `.option noise=...` (non-negative sigma) with deterministic seed-driven sampling and optional `.option temp=...` / `.option tnom=...` temperature scaling; the current parser now also accepts clearer aliases such as `.option noise_sigma=...`, `.option sigma=...`, `.option temperature=...`, `.option nominal_temperature=...`, Celsius shorthand forms such as `.option tempc=...` / `.option tnomc=...`, and Kelvin-explicit forms such as `.option temperature_k=...` / `.option nominal_temperature_k=...`; when combined with `.option seed=...`, internal source perturbation noise is reproducible across runs, and the current subset also includes a lightweight resistor-noise contribution.
- done: independent `PULSE(...)`, minimal `PWL(...)`, `EXP(...)`, and minimal `SIN(...)` sources are now supported for the current narrow source subset; `PULSE(...)` accepts one-shot, periodic, finite-cycle (`Ncycles`), and keyword-style pulse argument forms (`v1=...`, `v2=...`, `td=...`, `tr=...`, `tf=...`, `pw=...`, `per=...`) including optional spaces around `=`, plus clearer aliases such as `low/high`, `delay/rise/fall/width/period`, and `cycles=...`; `SIN(...)` accepts both positional forms and keyword-style forms such as `vo=...`, `va=...`, `freq=...`, `td=...`, `theta=...`, and `phase=...`, and now also tolerates short aliases such as `f=...` and `phi=...`; `EXP(...)` now accepts keyword-style forms such as `v1=...`, `v2=...`, `td1=...`, `tau1=...`, `td2=...`, and `tau2=...`, plus endpoint aliases such as `low/high` and timing aliases such as `delay1/delay2` and `tau_rise/tau_fall`; supported function-source names are parsed case-insensitively, may be followed by optional whitespace before `(`, their argument lists may use either commas or plain whitespace separators, repeated-time `PWL(...)` points are treated as right-continuous step breakpoints, and plain transient `DC` sources now tolerate a trailing `AC ...` clause by ignoring it.
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
- in-repo phase-6 smoke benchmark decks (`K`, delayed `T`, minimal `JJ`, included-subckt `JJ`, included file-driven `JJ`, included delayed-`T` `JJ`, included source-bearing mutual-`K` `JJ`, included file-driven source-bearing mutual-`K` `JJ`, included delayed file-driven source-bearing mutual-`K` `JJ`, chained-include `JJ`, chained-include delayed-`T` `JJ`, chained-include source-bearing mutual-`K` `JJ`, chained-include file-driven source-bearing mutual-`K` `JJ`, chained-include delayed file-driven source-bearing mutual-`K` `JJ`, source-bearing chained-include `JJ`, file-driven chained-include `JJ`, delayed-source chained-include `JJ`) that execute through `simulate_file(..., internal_transient)`
- waveform numeric comparison helper via `python/scripts/compare_internal_external_waveforms.py` to compare internal CSV traces against external simulator CSV traces on shared node columns
- local real-JoSIM correlation evidence on Windows for both the full `josephson-junction` manifest subset and the full current phase-6 threshold manifest: `uv run python python/scripts/run_waveform_compare_manifest.py --josim-command "C:\tools\JoSIM-v2.7-windows-x64\bin\josim-cli.exe" --category josephson-junction --result-dir target\waveform-compare-jj-full --validate-pass` completes with `failures=0` and worst observed `worst_max_abs_v = 6.710659e-04 V`, while the latest refreshed full-threshold run `uv run python python/scripts/run_waveform_compare_manifest.py --josim-command "C:\tools\JoSIM-v2.7-windows-x64\bin\josim-cli.exe" --result-dir target\\waveform-compare-full-102 --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` now also completes with `failures=0` across the current 102-deck phase-6 baseline, including 42 `josephson-junction` decks; the full-run worst observed `worst_max_abs_v = 1.5877853e-03 V` on `sin_mixed_case_source_smoke.cir`, and the worst JJ observed `worst_max_abs_v = 6.71065936427136e-04 V` on `include_chain_source_k_jj_subckt_smoke.cir`
- dedicated `.lib` section-keyword focused evidence now also exists in the numeric manifest flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck lib_section_keyword_smoke.cir --josim-command "C:\tools\JoSIM-v2.7-windows-x64\bin\josim-cli.exe" --result-dir target\waveform-compare-lib-section-keyword-manifest --validate-pass` completes with `failures=0` and `worst_max_abs_v = 2.7894667572484692e-06 V`
- dedicated `.lib` section-keyword alias focused evidence now also exists in the numeric manifest flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck lib_sec_keyword_smoke.cir --josim-command "C:\tools\JoSIM-v2.7-windows-x64\bin\josim-cli.exe" --result-dir target\waveform-compare-lib-sec-keyword-manifest --validate-pass` completes with `failures=0` and `worst_max_abs_v = 2.7894667572484692e-06 V`
- dedicated `.lib` directive `section= TT` focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/lib_section_equals_spaced_value_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-lib-section-equals-spaced-value/result.json` reports `summary=PASS` with `worst_max_abs_v = 2.789467e-06 V`
- dedicated `.lib` directive `sec =TT` focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/lib_sec_equals_attached_value_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-lib-sec-equals-attached-value/result.json` reports `summary=PASS` with `worst_max_abs_v = 2.789467e-06 V`
- dedicated `.lib/.endl` library-block header `section= TT` focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/lib_section_header_equals_spaced_value_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-lib-section-header-equals-spaced-value/result.json` reports `summary=PASS` with `worst_max_abs_v = 2.789467e-06 V`
- dedicated `.lib/.endl` library-block header `sec =TT` focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/lib_sec_header_equals_attached_value_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-lib-sec-header-equals-attached-value/result.json` reports `summary=PASS` with `worst_max_abs_v = 2.789467e-06 V`
- dedicated keyword-form `EXP(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/exp_keyword_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-exp-keyword-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated alias-form `EXP(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/exp_alias_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-exp-alias-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated keyword-form `PULSE(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/pulse_keyword_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-pulse-keyword-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated alias-form `PULSE(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck pulse_alias_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-pulse-alias-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated comma-separated keyword-form `PULSE(v1=..., v2=..., td=..., tr=..., tf=..., pw=..., per=..., ncycles=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck pulse_keyword_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-pulse-keyword-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated mixed-case comma-separated keyword-form `pUlSe(v1=..., v2=..., td=..., tr=..., tf=..., pw=..., per=..., ncycles=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck pulse_keyword_mixed_case_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-pulse-keyword-mixed-case-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated keyword-form mixed-case `pUlSe(v1=... v2=... td=... tr=... tf=... pw=... per=... ncycles=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/pulse_keyword_mixed_case_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-pulse-keyword-mixed-case-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.818848e-04 V`
- dedicated keyword-form `PULSE (v1=... v2=... td=... tr=... tf=... pw=... per=... ncycles=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/pulse_keyword_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-pulse-keyword-space-before-paren-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.818848e-04 V`
- dedicated mixed-case `pUlSe(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/pulse_mixed_case_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-pulse-mixed-case-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated `PULSE (...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/pulse_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-pulse-space-before-paren-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated alias-form `SIN(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/sin_alias_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-sin-alias-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated mixed-case `sIn(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/sin_mixed_case_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-sin-mixed-case-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.587785e-03 V`
- dedicated `SIN (...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/sin_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-sin-space-before-paren-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.587785e-03 V`
- dedicated mixed-case `eXp(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/exp_mixed_case_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-exp-mixed-case-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated `EXP (...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/exp_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-exp-space-before-paren-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated keyword-form mixed-case `eXp(v1=... v2=... td1=... tau1=... td2=... tau2=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/exp_keyword_mixed_case_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-exp-keyword-mixed-case-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated keyword-form `EXP (v1=... v2=... td1=... tau1=... td2=... tau2=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/exp_keyword_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-exp-keyword-space-before-paren-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated comma-separated keyword-form `EXP(v1=..., v2=..., td1=..., tau1=..., td2=..., tau2=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/exp_keyword_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-exp-keyword-comma-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated mixed-case comma-separated keyword-form `eXp(v1=..., v2=..., td1=..., tau1=..., td2=..., tau2=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck exp_keyword_mixed_case_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-exp-keyword-mixed-case-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 3.692855e-04 V`
- dedicated keyword-form `SIN(...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/sin_keyword_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-sin-keyword-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated keyword-form mixed-case `sIn(vo=... va=... freq=... td=... theta=... phi=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/sin_keyword_mixed_case_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-sin-keyword-mixed-case-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated keyword-form `SIN (vo=... va=... freq=... td=... theta=... phi=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/sin_keyword_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-sin-keyword-space-before-paren-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated comma-separated keyword-form `SIN(vo=..., va=..., freq=..., td=..., theta=..., phi=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/compare_internal_external_waveforms.py python/tests/benchmarks/phase6/sin_keyword_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --json-output target/waveform-compare-sin-keyword-comma-source/result.json` reports `summary=PASS` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated mixed-case comma-separated keyword-form `sIn(vo=..., va=..., freq=..., td=..., theta=..., phi=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck sin_keyword_mixed_case_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-sin-keyword-mixed-case-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated comma-separated keyword-form `SIN(vo=..., va=..., freq=..., td=..., theta=..., phase=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck sin_keyword_phase_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-sin-keyword-phase-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated mixed-case keyword-form `eXp (v1=... v2=... td1=... tau1=... td2=... tau2=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck exp_keyword_mixed_case_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-exp-keyword-mixed-case-space-before-paren-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 9.919132e-04 V`
- dedicated mixed-case keyword-form `sIn (vo=... va=... freq=... td=... theta=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck sin_keyword_mixed_case_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-sin-keyword-mixed-case-space-before-paren-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.770375e-04 V`
- dedicated mixed-case keyword-form `pUlse (v1=... v2=... td=... tr=... tf=... pw=... per=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck pulse_keyword_mixed_case_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-pulse-keyword-mixed-case-space-before-paren-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated phase= alias keyword-form `SIN (vo=... va=... freq=... td=... theta=... phase=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck sin_phase_alias_keyword_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-sin-phase-alias-keyword-space-before-paren-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated phase= alias mixed-case keyword-form `sIn (vo=... va=... freq=... td=... theta=... phase=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck sin_phase_alias_keyword_mixed_case_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-sin-phase-alias-keyword-mixed-case-space-before-paren-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated phase= alias mixed-case comma keyword-form `sIn(vo=..., va=..., freq=..., td=..., theta=..., phase=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck sin_phase_alias_keyword_mixed_case_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-sin-phase-alias-keyword-mixed-case-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated alias mixed-case comma keyword-form `pUlSe(low=..., high=..., delay=..., rise=..., fall=..., width=..., period=..., cycles=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck pulse_alias_mixed_case_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-pulse-alias-mixed-case-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated alias mixed-case keyword-form `pUlSe (low=... high=... delay=... rise=... fall=... width=... period=... cycles=...)` source focused evidence with whitespace before `(` now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck pulse_alias_mixed_case_space_before_paren_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-pulse-alias-mixed-case-space-before-paren-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 1.000000e-03 V`
- dedicated alias mixed-case comma keyword-form `eXp(low=..., high=..., delay1=..., tau_rise=..., delay2=..., tau_fall=...)` source focused evidence now also exists in the numeric compare flow: `uv run python python/scripts/run_waveform_compare_manifest.py --deck exp_alias_mixed_case_comma_source_smoke.cir --josim-command "C:/tools/JoSIM-v2.7-windows-x64/bin/josim-cli.exe" --result-dir target/waveform-compare-exp-alias-mixed-case-comma-source --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression` reports `failures=0` with `worst_max_abs_v = 9.905453e-04 V`
- `.lib` section-selection syntax matrix is now covered by parser and focused numeric evidence for `section=...`, `section = ...`, `sec=...`, and `sec = ...` spellings on both directive-side selection and library-block headers.
- optional pytest wrapper via `python/tests/test_waveform_compare.py` with per-deck thresholds in `python/tests/benchmarks/phase6/waveform_thresholds.json`; this test auto-skips when `josim` is unavailable and can be pointed to a custom binary via `RFLOW_JOSIM_COMMAND`
- external `pi` JJ semantics are now preserved for the current benchmark-safe subset by rewriting supported integer `pi` spellings into a negative `icrit` sign flip in generated JoSIM model cards, so both top-level `jj_pi_model_smoke.cir` and the nonzero-integer regression deck `jj_pi_warning_smoke.cir` now belong to the numeric phase-6 JJ manifest instead of the warning-only boundary; the latter specifically covers real JoSIM-compatible integer forms through `.model ... jj(... pi=-1)` plus an instance-side `pi=2` override and passes a focused compare at `worst_max_abs_v = 5.944924791644059e-04 V`
- alternate JJ model keyword references are now also covered numerically: top-level `jj_modelname_keyword_smoke.cir` exercises `modelname=jjmod` instead of `model=jjmod` and passes a focused real-JoSIM waveform compare at `worst_max_abs_v = 1.21349187316889e-04 V`, so the phase-6 numeric manifest no longer relies on only one keyword spelling for `.model ... jj(...)` references
- supported second-harmonic JJ semantics are now also preserved for the current benchmark-safe subset by lowering `icrit2`/`ic2`/`cp2` into native JoSIM `CPR={...}` coefficients, so top-level `jj_second_harmonic_model_smoke.cir` belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 8.092015503915387e-05 V`, while the previously warning-only pure second-harmonic deck `jj_second_harmonic_warning_smoke.cir` now also belongs to the numeric phase-6 JJ manifest after tightening its `.tran` step and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 2.4437882593625543e-04 V`
- supported second-harmonic model-default-plus-inline-override semantics are now also preserved for the current benchmark-safe subset: top-level `jj_second_harmonic_model_override_smoke.cir` keeps second-harmonic CPR lowering from the `.model ... jj(...)` defaults while overriding `rn` at the instance site, and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.536703344938473e-05 V`
- supported third-harmonic JJ semantics are now also preserved for the current benchmark-safe subset by lowering `icrit3`/`ic3`/`cp3` into three-coefficient native JoSIM `CPR={...}` arguments, so top-level `jj_third_harmonic_model_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.502456649475379e-05 V`
- supported third-harmonic model-default-plus-inline-override semantics are now also preserved for the current benchmark-safe subset: top-level `jj_third_harmonic_model_override_smoke.cir` keeps third-harmonic CPR lowering from the `.model ... jj(...)` defaults while overriding `rn` at the instance site, and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 6.84285041106982e-05 V`
- pure third-harmonic JJ semantics are now also preserved for the current benchmark-safe subset by lowering `icrit3`/`ic3`/`cp3` into native JoSIM `CPR={0,0,1}` when no primary `icrit` basis is present, so top-level `jj_third_harmonic_pure_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.3659976166242744e-04 V`
- supported fourth-harmonic JJ semantics are now also preserved for the current benchmark-safe subset by lowering `icrit4`/`ic4`/`cp4` into the fourth native JoSIM `CPR={...}` coefficient, so top-level `jj_fourth_harmonic_model_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 8.139832268908662e-05 V`
- supported fourth-harmonic model-default-plus-inline-override semantics are now also preserved for the current benchmark-safe subset: top-level `jj_fourth_harmonic_model_override_smoke.cir` keeps fourth-harmonic CPR lowering from the `.model ... jj(...)` defaults while overriding `rn` at the instance site, and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 6.923493847489423e-05 V`
- supported fifth-harmonic JJ semantics are now also preserved for the current benchmark-safe subset by lowering `icrit5`/`ic5`/`cp5` into the fifth native JoSIM `CPR={...}` coefficient, so top-level `jj_fifth_harmonic_model_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 7.407041443428707e-05 V`
- supported fifth-harmonic model-default-plus-inline-override semantics are now also preserved for the current benchmark-safe subset: top-level `jj_fifth_harmonic_model_override_smoke.cir` keeps fifth-harmonic CPR lowering from the `.model ... jj(...)` defaults while overriding `rn` at the instance site, and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 6.977242181030554e-05 V`
- pure fourth-harmonic JJ semantics are now also preserved for the current benchmark-safe subset by lowering `icrit4`/`ic4`/`cp4` into native JoSIM `CPR={0,0,0,1}` when no primary `icrit` basis is present, so top-level `jj_fourth_harmonic_pure_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 1.902355718421819e-04 V`
- pure fifth-harmonic JJ semantics are now also preserved for the current benchmark-safe subset by lowering `icrit5`/`ic5`/`cp5` into native JoSIM `CPR={0,0,0,0,1}` when no primary `icrit` basis is present, so top-level `jj_fifth_harmonic_pure_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 1.6556239837954497e-04 V`
- native `.model ... jj(... cpr={1,0.2,0.05})` syntax is now also preserved through the supported three-coefficient subset instead of being silently degraded to first-harmonic-only semantics, so top-level `jj_native_cpr_model_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.2022903971380964e-05 V`
- native `.model ... jj(... cpr={1,0.2,0.05,0.01})` syntax is now also preserved through the supported four-coefficient subset instead of being truncated to lower-order terms, so top-level `jj_native_cpr_model_fourth_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.09772745045979e-05 V`
- native `.model ... jj(... cpr={1,0.2,0.05,0.01,0.005})` syntax is now also preserved through the supported six-coefficient subset instead of being truncated to lower-order terms, so top-level `jj_native_cpr_model_fifth_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.0734367247162754e-05 V`
- native `.model ... jj(... cpr={1,0.2,0.05,0.01,0.005})` model-default-plus-inline-override semantics are now also preserved for the current benchmark-safe subset: top-level `jj_native_cpr_model_override_fifth_smoke.cir` keeps the native five-coefficient JoSIM CPR model card while overriding `rn` at the instance site, and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 3.994670277649714e-05 V`
- native instance-side `J... cpr={1,0.2,0.05}` syntax is now also preserved through the supported three-coefficient subset instead of being tokenized away before JJ translation, so top-level `jj_native_cpr_instance_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.2022903971380964e-05 V`
- native instance-side `J... cpr={1,0.2,0.05,0.01}` syntax is now also preserved through the supported four-coefficient subset instead of being tokenized away before JJ translation, so top-level `jj_native_cpr_instance_fourth_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.09772745045979e-05 V`
- native instance-side `J... cpr={1,0.2,0.05,0.01,0.005}` syntax is now also preserved through the supported six-coefficient subset instead of being tokenized away before JJ translation, so top-level `jj_native_cpr_instance_fifth_smoke.cir` now belongs to the numeric phase-6 JJ manifest and passes a focused real-JoSIM waveform compare with `worst_max_abs_v = 4.0734367247162754e-05 V`
- the external-warning manifest path remains available for future unsupported-boundary review, but the current benchmark-safe phase-6 contract file is empty because pure second-harmonic, pure third-harmonic, pure fourth-harmonic, pure fifth-harmonic, and pure sixth-harmonic requests without a primary `icrit` basis no longer need translation warnings in the supported subset; CI still stages the resulting empty review packet via `python/scripts/prepare_external_warning_artifacts.py` so any future unsupported deck can be reintroduced without changing the workflow contract
- the current in-repo phase-6 smoke benchmark set now includes delayed-`T`, included-subckt delayed-`T`, chained-include delayed-`T`, included-subckt delayed-`T` with a file-driven PWL source, chained-include delayed-`T` with a file-driven PWL source, mutual-`K`, included-subckt mutual-`K`, chained-include mutual-`K`, source-bearing included-subckt mutual-`K`, chained-include source-bearing mutual-`K`, minimal `JJ`, included-subckt `JJ` with an included model card, included-subckt file-driven `JJ`, included-subckt delayed-`T` `JJ`, included source-bearing mutual-`K` `JJ`, included file-driven source-bearing mutual-`K` `JJ`, included delayed file-driven source-bearing mutual-`K` `JJ`, chained-include `JJ` with a leaf-defined model card, chained-include delayed-`T` `JJ`, chained-include source-bearing mutual-`K` `JJ`, chained-include file-driven source-bearing mutual-`K` `JJ`, chained-include delayed file-driven source-bearing mutual-`K` `JJ`, source-bearing chained-include `JJ`, file-driven chained-include `JJ`, delayed-source chained-include `JJ`, included-subckt, nested-subckt, chained-include, source-bearing included-subckt, file-driven-PWL included-subckt, and chained-include file-driven PWL passive RC decks so the first gate covers device/interconnect behavior, source-waveform fidelity, nonlinear JJ hierarchy semantics, source-file locality, transport-delay semantics, mutual-coupling semantics, and file-backed hierarchy semantics

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
- structured scalar measurement output for the currently supported internal `.measure tran <name> max|min|pp|avg|rms|final V(node[,ref]) [FROM=<time>] [TO=<time>]` subset via `measurement_details`
- structured point-sample measurement output for the currently supported internal `.measure tran <name> find V(node[,ref]) AT=<time>` subset via `measurement_details`
- structured crossing-sample measurement output for the currently supported internal `.measure tran <name> find V(node[,ref]) WHEN V(node[,ref])=<v>|V(node[,ref]) VAL=<v> RISE|FALL|CROSS=<n|LAST> [TD=<time>]` subset via `measurement_details`
- structured delay measurement output for the currently supported internal `.measure tran <name> TRIG V(node[,ref]) VAL=<v> RISE|FALL|CROSS=<n|LAST> [TD=<time>] TARG|TARGET V(node[,ref]) VAL=<v> RISE|FALL|CROSS=<n|LAST> [TD=<time>]` subset via `delay_details`
- structured measurement warnings for unresolved probes, empty windows, missing crossings, unavailable samples, and non-finite internal `.measure` results via `measurement_warnings`
- diagnostics bundle summaries for simulation reports now include `delay_detail_count`, `measurement_detail_count`, `measurement_warning_count`, and `violation_detail_count`, with the same counts mirrored into report snapshots and simulate-file completion events

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
- default Windows CI parity gate via `.github/workflows/ci.yml` job `waveform-compare-gate`, which downloads JoSIM `v2.7`, runs `python/scripts/run_waveform_compare_manifest.py --validate-pass --validate-no-regression` against the repo-tracked Windows baseline on push/PR, runs `python/scripts/run_external_warning_manifest.py --validate-pass`, stages those outputs through `python/scripts/prepare_waveform_compare_artifacts.py` and `python/scripts/prepare_external_warning_artifacts.py`, then uploads both the numeric waveform-compare bundle and the warning-contract review bundle from `target/`; `workflow_dispatch` keeps the same job but allows overriding `josim_command`, baseline source, and no-regression settings for ad hoc review runs
- release-candidate evidence for simulator-affecting changes must include one current waveform-compare summary, one candidate approved-baseline artifact, and either a `History Diff` review against the prior approved baseline or an explicit statement that no prior same-platform baseline exists yet
- when a same-platform approved baseline exists, release-candidate validation should also run `python/scripts/run_waveform_compare_manifest.py` or `python/scripts/summarize_waveform_compare_results.py` with `--validate-no-regression` and a declared `--regression-tolerance-v`

Release-readiness contract:

- treat `waveform_compare_summary.current.json` as the candidate run record under review
- treat `waveform_compare_summary.candidate-baseline.json` as the artifact to preserve only after the run is approved as the new baseline
- enforce strict no-regression by default only on the same-platform Windows gate; the Windows-local approved baseline now lives at `python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json`, while a future Linux baseline can enable the same policy on Linux runners without cross-platform drift
- milestone-6.6 closure requires both measured parity evidence in [josim-parity.md](./josim-parity.md) and a named owner sign-off path matching [ownership-matrix.md](./ownership-matrix.md)
- use [sim-release-readiness-checklist.md](./sim-release-readiness-checklist.md) as the executable candidate-review checklist for simulator-affecting changes

Exit gate:

- parity status in [josim-parity.md](./josim-parity.md) is updated with measured evidence.
- the release candidate records the summary artifact path, approved-baseline source, and whether no-regression validation was enforced or intentionally deferred.
- the DRI and required reviewers for the chosen baseline/gate decision are recorded per [ownership-matrix.md](./ownership-matrix.md).

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
| 6.6 Correlation and release gate | in progress | QA / benchmark + sim DRI | Compare script, approved-baseline workflow, repo-tracked Windows baseline, and default Windows CI no-regression gate are landed; same-platform Linux baseline promotion remains follow-on work rather than a blocker for the default gate |

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












