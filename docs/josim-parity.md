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
| `.include` support | Supported | Relative `.include` expansion exists for file-backed decks, and minimal `.lib <path> [section]` support now selects the matching `.lib ...` / `.endl` section block (with section-end name consistency checks, comma-delimited token tolerance, keyword-style `section=...` / `sec=...` token support, and inline semicolon section-token comment tolerance) when requested in the same file-backed path | partial | 6.1 | Extend beyond the current file-backed include/library subset |
| `.param` support | Supported with expression parsing | Minimal scalar `.param` parsing exists with parameter references and engineering suffixes, including spaced and comma-separated assignment forms such as `a = 1p, b=2` | partial | 6.1 | Extend beyond the current small expression subset |
| Expression evaluation | Supported | Minimal scalar expression support exists for parameterized deck values, including scientific-notation numeric literals such as `1e-6` in current parser-supported expression contexts | partial | 6.1 | Share evaluator across parser/elaboration |
| `.subckt` hierarchy | Supported | Minimal `.subckt` / `.ends` flattening exists for `X...` instances, including nested parameter passthrough through instance assignments, nested `.subckt` definition support, same-scope duplicate-definition diagnostics, and clearer unsupported-syntax diagnostics | partial | 6.1 | Flattening is acceptable initially |
| Parameter override in subcircuits | Supported | Simple instance parameter override exists, including `params:` marker syntax and nested passthrough of `name=value` arguments | partial | 6.1 | Important for library cells |
| Transient analysis | JoSIM currently focuses on transient analysis | External hook still exists, and `internal_transient` now runs a real linear `R/L/C/V/I` timestep loop for a narrow subset, including linearized flow verification decks, adaptive internal substepping with a scaled absolute-plus-relative error norm, outer-step segmentation and aligned substep boundaries near supported source event breakpoints, left-limit step assembly across repeated-time `PWL(...)` discontinuities, and explicit non-convergence diagnostics when the current refinement cap is exhausted | partial | 6.2 | Need broader device coverage and JoSIM-class semantics |
| Phase/voltage analysis handling | JoSIM documents phase-mode conventions and output handling | Native analog mode is still limited, but Rust/PyO3/Python simulation reports now expose explicit JoSIM-alignment semantics (`josim_alignment_level`, `josim_alignment_available`, `josim_next_step`) derived from the shared quality-gate policy for event-only, internal-transient, and external-JoSIM backends | partial | 6.2 | Expand from report-level semantics to broader native analog behavior |
| `.tran` options such as print-window control | Supported by JoSIM | Minimal `.tran` parsing exists including optional print-window fields, trailing `uic`, application of supported `.ic` startup values, acceptance of keyword-style `.tran` fields such as `tstep=... tstop=...` (including spaced `=` forms), and acceptance of minimal `.nodeset` startup hints for the internal linear subset; in this subset, `.tran/.ic/.nodeset` control cards accept case-insensitive spellings, and `.ic`/`.nodeset` assignments accept both compact `V(n)=...` and spaced/comma forms such as `V(n) = ...`; minimal `.option`/`.options` parsing is accepted (case-insensitive card names), `.option reltol/abstol` feeds the internal nonlinear residual gate (including `rel`/`relerr` and `abs`/`abserr` aliases), `.option itl/itl1/itl4/maxiter/maxiters` maps to internal Newton max-iteration control, and these currently supported option keys also accept space-separated pairs such as `reltol 1e-4` | partial | 6.1 / 6.2 | Prioritize only options used in benchmarks |
| `.measure` extraction | Supported by JoSIM | Internal transient now supports a narrow `.measure` / `.meas` subset for scalar voltage measurements: `.measure tran <name> max|min|pp|avg|rms|final V(node[,ref]) [FROM=<time>] [TO=<time>]`, `.measure tran <name> find V(node[,ref]) AT=<time>`, and `.measure tran <name> find V(node[,ref]) WHEN V(node[,ref])=<v>|V(node[,ref]) VAL=<v> RISE|FALL|CROSS=<n|LAST> [TD=<time>]`. It also supports first-order delay measurements using `.measure tran <name> TRIG V(node[,ref])=<v>|V(node[,ref]) VAL=<v> RISE|FALL|CROSS=<n|LAST> [TD=<time>] TARG|TARGET V(node[,ref])=<v>|V(node[,ref]) VAL=<v> RISE|FALL|CROSS=<n|LAST> [TD=<time>]`, surfaced separately as `delay_details`. Scalar results are surfaced as `measurement_details` through Rust, PyO3, Python, and CLI JSON; unresolved probes, empty windows, missing crossings, unavailable samples, and non-finite results are surfaced as `measurement_warnings`. | partial | 6.5 | Broaden expression and advanced timing-measure semantics |
| Passive devices (R/L/C) | Supported | Native internal transient currently solves a narrow linear `R/L/C` subset, and the current parser now accepts both positional passive values and minimal keyword-style `R/L/C` forms such as `resistance=...`, `capacitance = ...`, `inductance=...`, and space-separated pairs like `resistance 50` | partial | 6.2 | Required before JJ support |
| Independent sources | Supported | Native internal transient currently solves independent DC `V/I` sources in the current subset, including transient decks that carry a trailing `AC ...` clause on otherwise-DC sources and `DC=...` / `DC = ...` keyword assignment forms | partial | 6.2 | Expand source families further |
| Pulse / PWL / sinusoidal sources | Supported | Native internal transient now supports minimal `PULSE(...)` including one-shot, periodic, finite-cycle (`Ncycles`) forms, and keyword-style pulse argument forms such as `v1=... v2=... td=... tr=... tf=... pw=... per=...` (including optional spaces around `=`), plus alias spellings such as `low/high`, `delay/rise/fall/width/period`, and `cycles=...`; plus `PWL(...)`, `EXP(...)`, and `SIN(...)` subsets including optional SIN damping and phase; `SIN(...)` now also accepts keyword-style forms such as `vo=... va=... freq=... td=... theta=... phase=...`, plus short aliases such as `f=...` and `phi=...`, and `EXP(...)` accepts keyword-style forms such as `v1=... v2=... td1=... tau1=... td2=... tau2=...` plus endpoint aliases `low/high` and timing aliases `delay1/delay2` plus `tau_rise/tau_fall`; supported function-source names are case-insensitive, may be followed by optional whitespace before `(`, argument lists may use commas or whitespace separators, and repeated-time `PWL(...)` points are interpreted as step discontinuities | partial | 6.1 / 6.2 | Needed for example deck parity |
| File/custom waveform sources | Supported in JoSIM | Native internal transient now supports a minimal file-driven waveform bootstrap through `PWL(file=...)` / `PWL(path=...)`, loading time-value pairs from text files (comma or whitespace separated) into the existing PWL source path with explicit diagnostics for unreadable or malformed waveform files; `simulate_file(...)` now resolves relative waveform-file paths from the deck directory, including when a `PWL(file=...)` source originates from an included file, so repository benchmarks can carry sidecar waveform assets alongside nested include hierarchies; broader JoSIM-class custom source forms remain pending | partial | 6.3 | Expand beyond the current PWL-file subset |
| Josephson junction (RCSJ) | Core JoSIM capability | Native internal transient now accepts a minimal nonlinear JJ subset (`J ... [model] icrit= rn= [cj=]`) solved through the iterative nonlinear stamp path with history-integrated phase and `sin(phi)` supercurrent linearization, including spaced `name = value` assignment forms, comma-separated assignment tokens such as `icrit=..., rn=..., cj=...`, and space-separated pair forms such as `icrit 0.5m rn 20` or `model jjmod rn 25`; minimal `.model <name> jj(...)` / `.model <name> jj ...` cards are now parsed for `icrit`/`rn`/`cj` defaults, native `.model ... jj(... cpr={...})` coefficients through the supported six-coefficient subset, and inline `J... cpr={...}` coefficients through the same first-six-terms subset when a basis `icrit` is available, and can be referenced by `J` instances either positionally or through keyword-style `model=...` / `modelname=...` references (with per-instance assignment override), while the external JoSIM compatibility path now preserves the current benchmark-safe subset for inline parameters, keyword-style model references, benchmark-safe model-default-plus-inline overrides for `icrit`/`rn`/`cj`, supported `pi` sign-flip semantics, native six-coefficient `.model ... cpr={...}` input, native inline `J... cpr={...}` input, and supported second-, third-, fourth-, fifth-, and sixth-harmonic lowering into native JoSIM `CPR={...}` coefficients including pure second-harmonic `CPR={0,1}` emission when no primary `icrit` term is present, pure third-harmonic `CPR={0,0,1}` emission when the third harmonic is the only available basis current, pure fourth-harmonic `CPR={0,0,0,1}` emission when the fourth harmonic is the only available basis current, pure fifth-harmonic `CPR={0,0,0,0,1}` emission when the fifth harmonic is the only available basis current, and higher-harmonic lowering through the sixth CPR coefficient when present; the current benchmark-safe phase-6 warning manifest is empty because no remaining shipped JJ benchmark needs translation warnings in that subset | partial | 6.3 | Expand to full phase-dynamics RCSJ and JoSIM-compatible model semantics |
| Pi-junction support | Supported in JoSIM releases | Native internal transient now supports a minimal pi-junction bootstrap for the JJ subset through `pi` flag parsing (`pi=0`, `pi=1`, and other nonzero integer forms such as `pi=-1` or `pi=2`, plus boolean-like values on the native path) on both `J` instance assignments and `.model ... jj(...)` defaults, implemented as a critical-current sign inversion in the current nonlinear stamp path; the external JoSIM compatibility path now also normalizes nonzero integer `pi` spellings into the same sign-flip behavior instead of surfacing translation warnings, while broader JoSIM-class pi-junction semantics remain pending | partial | 6.3 | Expand beyond the current sign-flip bootstrap |
| Multi-harmonic CPR | Supported in JoSIM releases | Native internal transient now supports a minimal second-, third-, fourth-, fifth-, and sixth-harmonic CPR bootstrap in the JJ subset (`icrit2`/`ic2`/`cp2`, `icrit3`/`ic3`/`cp3`, `icrit4`/`ic4`/`cp4`, `icrit5`/`ic5`/`cp5`, and `icrit6`/`ic6`/`cp6` on `J` instances, plus native `.model ... jj(... cpr={...})` and inline `J... cpr={...}` coefficients through the first six terms), contributing added `I2*sin(2phi)`, `I3*sin(3phi)`, `I4*sin(4phi)`, `I5*sin(5phi)`, and `I6*sin(6phi)` terms plus their Jacobian linearization in the current nonlinear stamp path; higher-order/JoSIM-class CPR model coverage beyond sixth harmonic remains pending | partial | 6.3 | Extend beyond sixth harmonic and align with JoSIM model semantics |
| Transmission line support | Supported | Native internal transient now supports a minimal `T` subset with `z0` plus finite `td` through a bounded delayed surrogate stamp (and exact coupling when `td=0`), and now also supports optional attenuation controls (`loss=` in [0,1] amplitude ratio loss or `alpha=` in 1/s with attenuation `exp(-alpha*td)`) plus keyword parameters supplied as `name=value`, spaced `name = value`, space-separated pairs such as `z0 50 td 1p`, and comma-separated cards such as `z0=50, td=1p`, plus aliases such as `zo`, `tau`, and `atten`; finite `td` now increases internal substep pressure and also contributes startup-window breakpoint / boundary alignment under coarse transient steps, including delayed arrival of connected source breakpoint events across single-hop, chained, and parallel `T` links along simple arrival paths, with extra intermediate chain arrivals and bare intermediate or dangling `td` breakpoints now filtered unless they reach observed non-transmission endpoints, while full JoSIM-class delay semantics are still pending | partial | 6.3 | Replace the current surrogate with full delay-line semantics |
| Mutual inductance (`K`) | Supported | Native internal transient now supports a minimal linear `K L1 L2 k` subset for previously declared or forward-declared inductor names, with bounded coefficient parsing for bare/keyword forms including `coupling=...`, spaced `coupling = ...`, and space-separated pairs such as `coupling 0.9` / `k 0.9`, plus coupled inductor companion-model stamping | partial | 6.3 | Expand beyond the current linear mutual-inductor subset |
| Thermal/noise options | Supported in JoSIM releases | Native internal transient now supports a minimal stochastic-noise bootstrap via `.option tnoise=...` / `.option noise=...` (non-negative sigma), with optional `.option temp=...` and `.option tnom=...` controls used for a temperature-ratio noise scale factor; the current parser also accepts clearer aliases such as `noise_sigma`, `sigma`, `temperature`, and `nominal_temperature`, Celsius shorthand forms such as `tempc` and `tnomc`, plus Kelvin-explicit forms such as `temperature_k` and `nominal_temperature_k`; seeded deterministic perturbations are applied on independent source stamps plus a minimal resistor-noise contribution in the current subset, but this remains a lightweight compatibility path rather than full thermal/noise device modeling | partial | 6.3 | Expand toward device- and temperature-aware JoSIM-class noise semantics |
| Deterministic RNG seed | Supported via `.option seed=` | Internal transient parses `.option seed=...` (including spaced `seed = ...`, parameter expressions, comma-separated `.option` lists, and multi-line `.option` decks where non-seed lines do not clear an earlier seed), surfaces it in result metadata, and now uses it to drive reproducible internal noise sampling when noise options are enabled | partial | 6.3 | Extend seeded stochastic paths beyond the current minimal source-perturbation bootstrap |
| CSV output | Supported | Native internal transient emits sampled waveform CSV and supports explicit output-path options; reports now expose explicit waveform format metadata (`waveform_format=csv_v1`) through Rust, PyO3, Python, and CLI JSON for machine-readable schema handling | done | 6.5 | CSV contract is now explicit for integrations |
| Raw waveform output | Supported | Native/internal and external flows both surface waveform artifact paths, and reports now classify artifact format explicitly (`waveform_format`: `csv_v1` or `external_passthrough`) so embedding layers can handle passthrough artifacts deterministically | done | 6.5 | Path+format contract is now explicit even when external tool format varies |
| Stdout/script summary | Supported | Result parsing accepts standardized `SIM_*` summary keys while preserving `RFLOW_*` and legacy aliases, and now reports machine-readable summary-contract classification (`external_summary_contract`: `sim_v1` / `legacy` / `mixed`) through Rust, PyO3, and Python simulation reports | done | 6.5 | Contract surface is now explicit and embeddable |
| Error diagnostics | Mature parser/runtime diagnostics | Limited to external command failures and flow checks | partial | 6.1-6.5 | Track parser vs solver diagnostics separately |
| Cross-platform executable use | JoSIM ships on major platforms | External command policy is now surfaced as embeddable APIs (`rflux_sim::is_supported_external_command`, Python `is_supported_external_command(...)`) and validated against Windows/Unix-style command forms in tests | done | 6.5 | Platform-neutral command-policy probe is available to integrations |
| Embeddable library API | `libjosim` exists | `rflux-sim` now exposes embeddable Rust APIs (`parse_deck`, `parse_deck_file`, `simulate_text`, `simulate_file`) plus shared quality-gate report surfaces, validated by crate unit coverage | done | 6.5 | Rust API is the parity target, not C++ ABI |
| Python embedding | JoSIM has Python-adjacent ecosystem support | PyO3/Python expose direct `simulate_text(...)` and `simulate_file(...)` with explicit backend selection and structured JoSIM-alignment/quality-gate result semantics (`josim_alignment_level`, `josim_alignment_available`, `josim_next_step`, `josim_quality_passed`, `josim_quality_status`) for embedding workflows | done | 6.4 / 6.5 | Parity target is embedding API capability, not JoSIM ecosystem breadth |
| Benchmark evidence | JoSIM has examples and mature regression history | Initial in-repo phase-6 smoke benchmark decks now exist for pulse-driven, sinusoidal, exponential, and file-driven PWL passive waveform, included-subckt, nested-subckt, chained-include, source-bearing included-subckt, file-driven-PWL included-subckt, chained-include file-driven-PWL included-subckt, included-subckt delayed-`T` passive decks, chained-include delayed-`T` passive decks, included-subckt delayed-`T` plus file-driven-PWL passive decks, chained-include delayed-`T` plus file-driven-PWL passive decks, included-subckt mutual-`K` passive decks, chained-include mutual-`K` passive decks, source-bearing included-subckt mutual-`K` passive decks, chained-include source-bearing mutual-`K` passive decks, included-subckt `JJ` decks with an included model card, included-subckt file-driven `JJ` decks, included-subckt delayed-`T` `JJ` decks, included source-bearing mutual-`K` `JJ` decks, included file-driven source-bearing mutual-`K` `JJ` decks, included delayed file-driven source-bearing mutual-`K` `JJ` decks, chained-include `JJ` decks with a leaf-defined model card, chained-include delayed-`T` `JJ` decks, chained-include source-bearing mutual-`K` `JJ` decks, chained-include file-driven source-bearing mutual-`K` `JJ` decks, chained-include delayed file-driven source-bearing mutual-`K` `JJ` decks, source-bearing chained-include `JJ` decks, file-driven chained-include `JJ` decks, delayed-source chained-include `JJ` decks, plus standalone `K`, delayed `T`, minimal `JJ`, `pi`-junction, and supported multi-harmonic internal-transient runs through the sixth harmonic, including native `.model ... cpr={...}` and inline `J... cpr={...}` coverage through six coefficients. The current threshold manifest now contains 105 phase-6 decks, including 42 `josephson-junction` decks, after promoting dedicated `.lib` section-keyword regression alongside the earlier native-CPR and harmonic model-override regressions, fourth-harmonic model-override regression, third-harmonic model-override regression, second-harmonic model-override regression, fifth-harmonic model regression, pure third-harmonic regression, pure fourth-harmonic regression, pure fifth-harmonic regression, fifth-order native-`cpr` instance regression, fifth-order native-`cpr` model regression, fourth-harmonic model, native-`cpr` fourth-coefficient model, native-`cpr` fourth-coefficient instance, native-model-`cpr`, native-instance-`cpr`, third-harmonic, integer-`pi`, `modelname=...`, and source-syntax additions covering keyword-form `PULSE(...)` / `EXP(...)` / `SIN(...)`, alias-form `EXP(...)` / `SIN(...)`, mixed-case `sIn(...)`, and `SIN (...)` with whitespace before `(`. The external JoSIM translation path now preserves the minimal inline/model-card JJ parameter subset used by these current benchmarks instead of silently dropping back to JoSIM default model semantics by rewriting inline `J` parameters into synthetic `.model ... jj(...)` cards, remapping model-card `cj` to JoSIM's `cap` alias, rewriting supported integer `pi` semantics into a negative `icrit` sign flip for generated model cards, preserving supported native `.model ... cpr={...}` input and inline `J... cpr={...}` input through the first six coefficients, lowering supported second-, third-, fourth-, fifth-, and sixth-harmonic alias semantics into native JoSIM `CPR={...}` coefficients, including pure second-harmonic `CPR={0,1}` emission when no primary `icrit` term is present, pure third-harmonic `CPR={0,0,1}` emission when the third harmonic is the only available basis current, pure fourth-harmonic `CPR={0,0,0,1}` emission when the fourth harmonic is the only available basis current, pure fifth-harmonic `CPR={0,0,0,0,1}` emission when the fifth harmonic is the only available basis current, pure sixth-harmonic `CPR={0,0,0,0,0,1}` emission when the sixth harmonic is the only available basis current, and rewriting keyword-form `PULSE(...)` / `EXP(...)` / `SIN(...)`, alias-form `EXP(...)` / `SIN(...)`, mixed-case `sIn(...)`, and `SIN (...)` source calls into the positional JoSIM syntax accepted by the external simulator path. The external-warning flow remains available as a separate manifest-driven review lane, but the current benchmark-safe phase-6 warning-contract file is empty because no shipped JJ benchmark now needs translation warnings in that supported subset. A Python waveform comparison helper (`python/scripts/compare_internal_external_waveforms.py`) now provides first-pass internal-vs-external CSV error metrics, sorts nodes by descending error, emits structured JSON with threshold/failing-node/top-worst-node details, and exits non-zero on threshold breach so it can act as a gateable compare command; a summary helper (`python/scripts/summarize_waveform_compare_results.py`) can aggregate threshold results into markdown plus deck-level JSON, preserving failing-node details and worst-node summaries and failing on missing/failed decks; the optional CI waveform-compare job now runs `python/scripts/run_waveform_compare_manifest.py --validate-pass`, stages review artifacts through `python/scripts/prepare_waveform_compare_artifacts.py`, runs `python/scripts/run_external_warning_manifest.py --validate-pass`, stages the warning-review bundle through `python/scripts/prepare_external_warning_artifacts.py`, and uploads both review bundles for a single parity review packet even when the warning-contract subset is empty. An optional pytest integration (`python/tests/test_waveform_compare.py`) runs thresholded checks when `josim` is available (auto-skip otherwise), and compare utility behavior is covered by `python/tests/test_waveform_compare_utils.py` plus `python/tests/test_waveform_compare_summary_utils.py`. Current Windows-local numeric evidence includes focused promoted deck-level checks for `jj_pi_model_smoke.cir`, `jj_pi_warning_smoke.cir`, `jj_modelname_keyword_smoke.cir`, `jj_second_harmonic_model_smoke.cir`, `jj_second_harmonic_model_override_smoke.cir`, `jj_second_harmonic_warning_smoke.cir`, `jj_third_harmonic_model_smoke.cir`, `jj_third_harmonic_model_override_smoke.cir`, `jj_third_harmonic_pure_smoke.cir`, `jj_fourth_harmonic_model_smoke.cir`, `jj_fourth_harmonic_model_override_smoke.cir`, `jj_fourth_harmonic_pure_smoke.cir`, `jj_fifth_harmonic_model_smoke.cir`, `jj_fifth_harmonic_model_override_smoke.cir`, `jj_fifth_harmonic_pure_smoke.cir`, `jj_native_cpr_model_smoke.cir`, `jj_native_cpr_model_fourth_smoke.cir`, `jj_native_cpr_model_fifth_smoke.cir`, `jj_native_cpr_model_override_fifth_smoke.cir`, `jj_native_cpr_instance_smoke.cir`, `jj_native_cpr_instance_fourth_smoke.cir`, `jj_native_cpr_instance_fifth_smoke.cir`, `pulse_keyword_source_smoke.cir`, `pulse_keyword_mixed_case_source_smoke.cir`, `pulse_keyword_space_before_paren_source_smoke.cir`, `exp_alias_source_smoke.cir`, `exp_keyword_source_smoke.cir`, `exp_keyword_mixed_case_source_smoke.cir`, `exp_keyword_space_before_paren_source_smoke.cir`, `sin_alias_source_smoke.cir`, `sin_keyword_source_smoke.cir`, `sin_keyword_mixed_case_source_smoke.cir`, `sin_keyword_space_before_paren_source_smoke.cir`, `sin_mixed_case_source_smoke.cir`, and `sin_space_before_paren_source_smoke.cir`, plus the latest refreshed full-manifest run `uv run python python/scripts/run_waveform_compare_manifest.py --josim-command "C:\tools\JoSIM-v2.7-windows-x64\bin\josim-cli.exe" --result-dir target\\waveform-compare-full-105 --validate-pass --previous-summary-json python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json --validate-no-regression`, which completed with `failures=0` across the current 105-deck phase-6 baseline, including 42 `josephson-junction` decks; the full-run worst observed `worst_max_abs_v = 1.5877853e-03 V` on `sin_mixed_case_source_smoke.cir`, and the worst JJ observed `worst_max_abs_v = 6.71065936427136e-04 V` on `include_chain_source_k_jj_subckt_smoke.cir`. The latest `History Diff` adds `sin_phase_alias_keyword_mixed_case_source_smoke.cir` as `NEW -> PASS`. Manual manifest-driven JoSIM numeric correlation is now automated for on-demand CI review; the remaining gap is a same-platform Linux approved baseline for default Ubuntu no-regression enforcement | partial | 6.6 | Promote Linux approved baseline and enable default no-regression gate on Ubuntu runners |

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

## Correlation evidence workflow (default Windows CI)

Use this workflow when collecting or reviewing milestone-6.6 parity evidence through the default Windows CI quality gate.

1. On push and pull request events, let workflow `CI` run normally; job `waveform-compare-gate` now runs by default on `windows-latest`.
2. The gate downloads JoSIM `v2.7`, uses the repo-tracked Windows approved baseline by default, and enforces `--validate-pass` plus `--validate-no-regression` with zero tolerance unless workflow-dispatch inputs override that behavior.
3. For an ad hoc review run, open GitHub Actions and run workflow `CI` with `workflow_dispatch`.
4. Optionally set `josim_command` to a full path or wrapper command.
5. Optionally set `previous_summary_json` to a repo-relative approved baseline summary JSON if the run should emit a `History Diff` against an earlier approved baseline.
6. If `previous_summary_json` is left empty, optionally set `baseline_platform` so the workflow can auto-stage `python/tests/benchmarks/phase6/waveform_compare_summary.<platform>-approved-baseline.json` when that repo-tracked baseline exists.
7. Optionally set `validate_no_regression` and `regression_tolerance_v` if the run should relax or tighten drift enforcement relative to the resolved approved baseline.
8. Confirm job `waveform-compare-gate` runs and archive both the generated waveform-compare artifacts and the external-warning review artifacts.
9. Treat `waveform_compare_summary.current.json` as the current run record, `waveform_compare_summary.approved-baseline.json` as the optional staged prior baseline when one was provided explicitly or auto-resolved by platform, and `waveform_compare_summary.candidate-baseline.json` as the file to preserve from an approved green run for the next baseline promotion.
10. Use `manifest.json` in the uploaded artifact to confirm the role of each summary file, the threshold manifest path and SHA-256, the `josim_command`, optional `baseline_platform`, optional `validate_no_regression` / `regression_tolerance_v` settings, and the Python version that produced the run, the `summary_overview` quick-look block (`deck_count`, `category_count`, `passing_deck_count`, `failure_count`, `failing_decks`, `missing_decks`, `worst_deck`, `worst_max_abs_v`, `failing_categories`), the `category_overview` quick-look list (`category`, `deck_count`, `failure_count`, `worst_deck`, `worst_max_abs_v`), the `hotspot_overview` quick-look block (`top_deck_hotspots`, `top_category_hotspots`), the `history_diff_overview` quick-look block (`has_history_diff`, and when present `failure_delta`, `deck_change_count`, `category_change_count`, `top_deck_change`, `top_category_change`), the `validation_overview` quick-look block (`validated_deck_count`, `validated_category_count`, `validated_failure_count`, `all_decks_passed`), and the GitHub Actions run context (`workflow`, `run_id`, `run_attempt`, `sha`, `ref_name`).

For local reproduction of the same compare path that the default Windows CI gate now uses:

```bash
RFLOW_JOSIM_COMMAND=josim uv run pytest python/tests/test_waveform_compare.py -rs
uv run python python/scripts/run_waveform_compare_manifest.py --josim-command josim --deck include_delay_source_pwl_k_jj_subckt_smoke.cir --deck include_chain_delay_source_pwl_k_jj_subckt_smoke.cir --validate-pass
uv run python python/scripts/run_waveform_compare_manifest.py --josim-command josim --result-dir target/waveform-compare-full --validate-pass
uv run python python/scripts/run_waveform_compare_manifest.py --josim-command josim --result-dir target/waveform-compare-full --validate-no-regression --regression-tolerance-v 0.0
uv run python python/scripts/run_external_warning_manifest.py --josim-command josim --validate-pass --result-dir target/external-warning-manifest
uv run python python/scripts/summarize_waveform_compare_results.py --result-dir python/tests/benchmarks/phase6 --markdown-output python/tests/benchmarks/phase6/waveform_compare_summary.md --json-output python/tests/benchmarks/phase6/waveform_compare_summary.json
uv run python python/scripts/summarize_waveform_compare_results.py --result-dir python/tests/benchmarks/phase6 --previous-summary-json path/to/previous_waveform_compare_summary.json --markdown-output python/tests/benchmarks/phase6/waveform_compare_summary_with_diff.md --json-output python/tests/benchmarks/phase6/waveform_compare_summary_with_diff.json
```

Linux gate path (workflow_dispatch):

1. Run workflow `CI` with input `run_waveform_compare_linux=true`.
2. Provide `josim_command_linux` pointing to a Linux-available JoSIM binary/command.
3. The Linux gate now defaults `validate_no_regression_linux=true`; during first bootstrap runs, if no explicit baseline and no repo-tracked Linux approved baseline exist yet, the workflow auto-downgrades no-regression for that run and emits a notice.
4. Optional: provide `previous_summary_json_linux` to force a specific history baseline source.
4. Review artifact `waveform-compare-results-linux` and, for green runs, promote `waveform_compare_summary.candidate-baseline.{json,md}` via `python/scripts/promote_waveform_approved_baseline.py --platform linux`.

The summary helper path also has an explicit CI smoke anchor now instead of relying only on the broad Python suite:

- `uv run pytest python/tests/test_waveform_compare_summary_utils.py -q`

Use the summary markdown plus `*.compare.json` files as review evidence when updating parity status rows. The new `run_waveform_compare_manifest.py` helper is the shortest path when a milestone review only needs a subset of decks or categories rather than the full manifest, for example the newest deep JJ combinations. When `waveform_compare_summary.approved-baseline.json` already exists inside the chosen `--result-dir`, `run_waveform_compare_manifest.py` now auto-discovers it and forwards it as `--previous-summary-json`, so repeated local runs in the same result directory automatically produce a `History Diff` without having to restate the baseline path each time. The same runner can also auto-resolve a repo-tracked platform baseline from `python/tests/benchmarks/phase6/waveform_compare_summary.<platform>-approved-baseline.json` via `--baseline-platform <platform>`, which is the intended promotion path for a future Linux baseline. The runner also supports `--validate-no-regression` plus `--regression-tolerance-v` to fail when a current run worsens relative to that resolved baseline. In CI artifacts, `waveform_compare_summary.current.json` is the current run record, `waveform_compare_summary.approved-baseline.json` is the optional staged prior baseline from the workflow input or platform auto-resolution, `waveform_compare_summary.candidate-baseline.json` is the copy meant to be archived from an approved run, and `manifest.json` records the intended role of each uploaded summary file plus the run's `josim_command`, optional `baseline_platform`, optional no-regression settings, Python version, threshold manifest path and SHA-256, a `summary_overview` quick-look block for triage (`PASS/FAIL/MISSING` distribution plus worst-deck context), a `category_overview` quick-look list for bucket-level triage, a `hotspot_overview` quick-look block that surfaces the worst deck-level nodes and category-level hotspots without opening the full summary JSON, a `history_diff_overview` quick-look block that reports whether a previous-summary comparison is present, how many deck/category changes were detected, and which deck/category moved most, a `validation_overview` quick-look block derived after `--validate-pass`, a `validation_contract` block stating that `waveform_compare_summary.validation.json` uses the same schema as the current summary but is generated with `--validate-pass` and is only present for zero-failure runs, and GitHub Actions run context. The same default gate now uploads the adjacent external-warning review bundle generated by `python/scripts/prepare_external_warning_artifacts.py`: `target/external-warning-review/*.warning.json`, `selected_external_warning_contracts.json`, `external_warning_summary.current.{md,json}`, plus that bundle's own `manifest.json` and `README.txt`, so the remaining unsupported-boundary evidence travels with the numeric compare evidence instead of being a separate manual step. When a prior approved summary JSON exists, run the third command or supply the workflow input to generate a `History Diff` view for review.

Tracked repo baseline note: the repo-tracked Windows approved baseline has now been refreshed from the latest green full-manifest run and is stored at `python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.json` (and `.md`). The promoted baseline captures the current 105-deck / 42-JJ phase-6 manifest with `failures=0`; the overall worst deck is `sin_mixed_case_source_smoke.cir` at `1.5877853e-03 V`, and the worst JJ deck is `include_chain_source_k_jj_subckt_smoke.cir` at `6.71065936427136e-04 V`. The latest summary also records `History Diff` entries showing `sin_phase_alias_keyword_mixed_case_source_smoke.cir` as `NEW -> PASS`. It is suitable as a stable Windows-local reference and as a manually supplied `previous_summary_json` for diff inspection. The corresponding future Linux promotion target is `python/tests/benchmarks/phase6/waveform_compare_summary.linux-approved-baseline.json`. Do not wire the Windows baseline as the default Ubuntu CI reference, because cross-platform numeric drift would blur a strict no-regression gate; promote a Linux-captured approved baseline into that Linux-named path before enabling `validate_no_regression` by default on the current workflow runner.

Local Windows evidence note: the repo-tracked approved baseline and the focused promoted-deck compare artifacts now supersede the earlier 15-deck and 35-deck milestone snapshots for routine parity review. Use `python/tests/benchmarks/phase6/waveform_compare_summary.windows-approved-baseline.{json,md}` as the current Windows reference, and inspect focused `target/waveform-compare-*` result directories only when reviewing a newly promoted slice such as sixth-harmonic, six-coefficient native CPR support, or `.lib` section-selection keyword support (`section` and `sec` forms).

Current phase-6 baseline decks and thresholds:

| Deck | Category | Threshold (V) | Why it is in the baseline |
|------|----------|---------------|----------------------------|
| `pulse_rc_smoke.cir` | source-waveform | `0.1` | Covers the first pulse-driven passive waveform baseline so source-shape regressions enter the same thresholded gate as device/interconnect smoke decks. |
| `sin_rc_smoke.cir` | source-waveform | `0.1` | Covers the first sinusoidal passive waveform baseline so continuous-wave source behavior is tracked in the same thresholded gate. |
| `exp_rc_smoke.cir` | source-waveform | `0.1` | Covers the first exponential passive waveform baseline so rise/fall shaping semantics are tracked in the same thresholded gate. |
| `pwl_file_rc_smoke.cir` | source-waveform | `0.1` | Covers the first file-driven PWL waveform baseline so sidecar waveform assets and relative source-file resolution are tracked in the same thresholded gate. |
| `include_subckt_smoke.cir` | hierarchical-subckt | `0.1` | Covers the first file-backed `.include` + parameterized `.subckt` baseline so hierarchy expansion and include resolution are tracked in the same thresholded gate. |
| `nested_subckt_smoke.cir` | hierarchical-subckt | `0.1` | Covers nested included `.subckt` parameter passthrough so flattened hierarchy propagation is tracked in the same thresholded gate. |
| `include_chain_smoke.cir` | hierarchical-subckt | `0.1` | Covers chained `.include` expansion so include-of-include file assembly is tracked in the same thresholded gate. |
| `include_source_subckt_smoke.cir` | hierarchical-subckt | `0.1` | Covers a source-bearing included `.subckt` so source flattening and hierarchy assembly are tracked in the same thresholded gate. |
| `include_source_pwl_subckt_smoke.cir` | hierarchical-subckt | `0.1` | Covers a file-driven `PWL(file=...)` source inside an included `.subckt`, so source-file locality and hierarchy assembly are tracked in the same thresholded gate. |
| `include_chain_source_pwl_subckt_smoke.cir` | hierarchical-subckt | `0.1` | Covers a file-driven `PWL(file=...)` source inside a chained include hierarchy, so include-of-include source-file locality is tracked in the same thresholded gate. |
| `t_delay_smoke.cir` | transmission-delay | `0.2` | Covers the current delayed-`T` smoke path and guards basic transport-delay behavior. |
| `include_delay_subckt_smoke.cir` | transmission-delay | `0.2` | Covers a delayed-`T` element inside an included `.subckt`, so transport-delay behavior is tracked under file-backed hierarchy flattening too. |
| `include_chain_delay_subckt_smoke.cir` | transmission-delay | `0.2` | Covers a delayed-`T` element through a chained include hierarchy, so transport-delay behavior is tracked under include-of-include flattening too. |
| `include_delay_source_pwl_subckt_smoke.cir` | transmission-delay | `0.2` | Covers a delayed-`T` element driven by a file-based `PWL(file=...)` source inside an included `.subckt`, so transport-delay behavior and source-file locality are tracked together under file-backed hierarchy flattening. |
| `include_chain_delay_source_pwl_subckt_smoke.cir` | transmission-delay | `0.2` | Covers a delayed-`T` element driven by a file-based `PWL(file=...)` source through a chained include hierarchy, so transport-delay behavior and source-file locality are tracked together under include-of-include flattening. |
| `k_mutual_smoke.cir` | mutual-inductance | `0.2` | Covers the current linear mutual-inductor path and catches coupling regressions. |
| `include_k_subckt_smoke.cir` | mutual-inductance | `0.2` | Covers a mutual-inductor pair inside an included `.subckt`, so coupling behavior is tracked under file-backed hierarchy flattening too. |
| `include_chain_k_subckt_smoke.cir` | mutual-inductance | `0.2` | Covers a mutual-inductor pair through a chained include hierarchy, so coupling behavior is tracked under include-of-include flattening too. |
| `include_source_k_subckt_smoke.cir` | mutual-inductance | `0.2` | Covers a source-bearing mutual-inductor pair inside an included `.subckt`, so coupling behavior and source-bearing hierarchy semantics are tracked together under file-backed flattening. |
| `include_chain_source_k_subckt_smoke.cir` | mutual-inductance | `0.2` | Covers a source-bearing mutual-inductor pair through a chained include hierarchy, so coupling behavior and source-bearing include-of-include semantics are tracked together. |
| `jj_minimal_smoke.cir` | josephson-junction | `0.3` | Covers the current bootstrap nonlinear JJ path with a looser tolerance matching present model maturity. |
| `include_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a JJ inside an included `.subckt` with its `.model` card defined in the included file, so nonlinear device parsing, model lookup, and hierarchy flattening are tracked together. |
| `include_source_pwl_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a file-driven `PWL(file=...)` source feeding a JJ inside an included `.subckt`, so nonlinear model lookup and source-file locality are tracked together under file-backed hierarchy semantics. |
| `include_delay_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a delayed-`T` source path feeding a JJ inside an included `.subckt`, so nonlinear model lookup and transport-delay semantics are tracked together under file-backed hierarchy semantics. |
| `include_source_k_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a source-bearing mutual-inductor path feeding a JJ inside an included `.subckt`, so nonlinear model lookup and mutual-coupling semantics are tracked together under file-backed hierarchy semantics. |
| `include_source_pwl_k_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a file-driven `PWL(file=...)` source feeding a mutual-inductor path and JJ inside an included `.subckt`, so nonlinear model lookup, mutual-coupling semantics, and source-file locality are tracked together under file-backed hierarchy semantics. |
| `include_delay_source_pwl_k_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a file-driven `PWL(file=...)` source feeding a delayed transport path, a mutual-inductor path, and a JJ inside an included `.subckt`, so nonlinear model lookup, transport-delay semantics, mutual-coupling semantics, and source-file locality are tracked together under file-backed hierarchy semantics. |
| `include_chain_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a JJ through a chained include hierarchy with its `.model` card defined in the leaf included file, so nonlinear model lookup and include-of-include hierarchy flattening are tracked together. |
| `include_chain_delay_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a delayed-`T` source path feeding a JJ through a chained include hierarchy, so nonlinear model lookup and transport-delay semantics are tracked together under include-of-include flattening. |
| `include_chain_source_k_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a source-bearing mutual-inductor path feeding a JJ through a chained include hierarchy, so nonlinear model lookup and mutual-coupling semantics are tracked together under include-of-include flattening. |
| `include_chain_source_pwl_k_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a file-driven `PWL(file=...)` source feeding a mutual-inductor path and JJ through a chained include hierarchy, so nonlinear model lookup, mutual-coupling semantics, and source-file locality are tracked together under include-of-include flattening. |
| `include_chain_delay_source_pwl_k_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a file-driven `PWL(file=...)` source feeding a delayed transport path, a mutual-inductor path, and a JJ through a chained include hierarchy, so nonlinear model lookup, transport-delay semantics, mutual-coupling semantics, and source-file locality are tracked together under include-of-include flattening. |
| `include_chain_source_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a source-bearing JJ through a chained include hierarchy, so nonlinear model lookup and source-bearing include-of-include hierarchy semantics are tracked together. |
| `include_chain_source_pwl_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a file-driven `PWL(file=...)` source feeding a JJ through a chained include hierarchy, so nonlinear model lookup and source-file locality are tracked together under include-of-include flattening. |
| `include_chain_delay_source_pwl_jj_subckt_smoke.cir` | josephson-junction | `0.3` | Covers a file-driven `PWL(file=...)` source feeding a JJ through a chained delayed-`T` hierarchy, so nonlinear model lookup, transport-delay semantics, and source-file locality are tracked together under include-of-include flattening. |

Recent `.lib` section-selection additions (now included in the 105-deck manifest and Windows approved baseline):

- `lib_section_keyword_smoke.cir` (`.lib <path> section = TT`)
- `lib_sec_keyword_smoke.cir` (`.lib <path> sec = TT`)
- `lib_section_equals_spaced_value_smoke.cir` (`.lib <path> section= TT`)
- `lib_sec_equals_attached_value_smoke.cir` (`.lib <path> sec =TT`)
- `lib_section_header_equals_spaced_value_smoke.cir` (`.lib/.endl section= TT` library-block headers)
- `lib_sec_header_equals_attached_value_smoke.cir` (`.lib/.endl sec =TT` library-block headers)

Recent source-syntax additions (now included in the 105-deck manifest and Windows approved baseline):

- `pulse_alias_source_smoke.cir` (`PULSE(low=... high=... delay=... rise=... fall=... width=... period=... cycles=...)` alias keyword form)
- `pulse_keyword_source_smoke.cir` (`PULSE(v1=... v2=... td=... tr=... tf=... pw=... per=... ncycles=...)`)
- `pulse_keyword_comma_source_smoke.cir` (`PULSE(v1=..., v2=..., td=..., tr=..., tf=..., pw=..., per=..., ncycles=...)` comma-separated keyword form)
- `pulse_keyword_mixed_case_comma_source_smoke.cir` (`pUlSe(v1=..., v2=..., td=..., tr=..., tf=..., pw=..., per=..., ncycles=...)` mixed-case comma-separated keyword form)
- `pulse_keyword_mixed_case_source_smoke.cir` (`pUlSe(v1=... v2=... td=... tr=... tf=... pw=... per=... ncycles=...)` mixed-case function spelling)
- `pulse_keyword_space_before_paren_source_smoke.cir` (`PULSE (v1=... v2=... td=... tr=... tf=... pw=... per=... ncycles=...)` with whitespace before `(`)
- `pulse_mixed_case_source_smoke.cir` (`pUlSe(...)` mixed-case function spelling)
- `pulse_space_before_paren_source_smoke.cir` (`PULSE (...)` with whitespace before `(`)
- `exp_alias_source_smoke.cir` (`EXP(low=... high=... delay1=... tau_rise=... delay2=... tau_fall=...)`)
- `exp_keyword_source_smoke.cir` (`EXP(v1=... v2=... td1=... tau1=... td2=... tau2=...)`)
- `exp_keyword_comma_source_smoke.cir` (`EXP(v1=..., v2=..., td1=..., tau1=..., td2=..., tau2=...)` comma-separated keyword form)
- `exp_keyword_mixed_case_comma_source_smoke.cir` (`eXp(v1=..., v2=..., td1=..., tau1=..., td2=..., tau2=...)` mixed-case comma-separated keyword form)
- `exp_keyword_mixed_case_source_smoke.cir` (`eXp(v1=... v2=... td1=... tau1=... td2=... tau2=...)` mixed-case function spelling)
- `exp_keyword_space_before_paren_source_smoke.cir` (`EXP (v1=... v2=... td1=... tau1=... td2=... tau2=...)` with whitespace before `(`)
- `exp_mixed_case_source_smoke.cir` (`eXp(...)` mixed-case function spelling)
- `exp_space_before_paren_source_smoke.cir` (`EXP (...)` with whitespace before `(`)
- `sin_alias_source_smoke.cir` (`SIN(offset=... amplitude=... f=... td=... damping=... phi=...)`)
- `sin_mixed_case_source_smoke.cir` (`sIn(...)` mixed-case function spelling)
- `sin_space_before_paren_source_smoke.cir` (`SIN (...)` with whitespace before `(`)
- `sin_keyword_source_smoke.cir` (`SIN(vo=... va=... freq=... td=... theta=... phi=...)`)
- `sin_keyword_comma_source_smoke.cir` (`SIN(vo=..., va=..., freq=..., td=..., theta=..., phi=...)` comma-separated keyword form)
- `sin_keyword_phase_comma_source_smoke.cir` (`SIN(vo=..., va=..., freq=..., td=..., theta=..., phase=...)` comma-separated keyword form with `phase` alias)
- `exp_keyword_mixed_case_space_before_paren_source_smoke.cir` (`eXp (v1=... v2=... td1=... tau1=... td2=... tau2=...)` mixed-case keyword spelling with whitespace before `(`)
- `sin_keyword_mixed_case_space_before_paren_source_smoke.cir` (`sIn (vo=... va=... freq=... td=... theta=...)` mixed-case keyword spelling with whitespace before `(`)
- `pulse_keyword_mixed_case_space_before_paren_source_smoke.cir` (`pUlse (v1=... v2=... td=... tr=... tf=... pw=... per=...)` mixed-case keyword spelling with whitespace before `(`)
- `sin_phase_alias_keyword_space_before_paren_source_smoke.cir` (`SIN (vo=... va=... freq=... td=... theta=... phase=...)` phase= alias with whitespace before `(`)
- `sin_phase_alias_keyword_mixed_case_space_before_paren_source_smoke.cir` (`sIn (vo=... va=... freq=... td=... theta=... phase=...)` phase= alias with mixed-case keyword spelling and whitespace before `(`)
- `sin_phase_alias_keyword_mixed_case_comma_source_smoke.cir` (`sIn(vo=..., va=..., freq=..., td=..., theta=..., phase=...)` phase= alias with mixed-case keyword spelling and comma-separated keyword arguments)
- `sin_phase_alias_keyword_mixed_case_source_smoke.cir` (`sIn(vo=... va=... freq=... td=... theta=... phase=...)` phase= alias with mixed-case compact keyword spelling)
- `sin_phase_alias_keyword_source_smoke.cir` (`SIN(vo=... va=... freq=... td=... theta=... phase=...)` phase= alias with compact keyword spelling)
- `pulse_alias_mixed_case_comma_source_smoke.cir` (`pUlSe(low=..., high=..., delay=..., rise=..., fall=..., width=..., period=..., cycles=...)` alias form with mixed-case keyword spelling and comma-separated arguments)
- `pulse_alias_mixed_case_space_before_paren_source_smoke.cir` (`pUlSe (low=... high=... delay=... rise=... fall=... width=... period=... cycles=...)` alias form with mixed-case keyword spelling and whitespace before `(`)
- `exp_alias_mixed_case_comma_source_smoke.cir` (`eXp(low=..., high=..., delay1=..., tau_rise=..., delay2=..., tau_fall=...)` alias form with mixed-case keyword spelling and comma-separated arguments)
- `exp_alias_mixed_case_space_before_paren_source_smoke.cir` (`eXp (low=... high=... delay1=... tau_rise=... delay2=... tau_fall=...)` alias form with mixed-case keyword spelling and whitespace before `(`)
- `sin_keyword_mixed_case_comma_source_smoke.cir` (`sIn(vo=..., va=..., freq=..., td=..., theta=..., phi=...)` mixed-case comma-separated keyword form)
- `sin_keyword_mixed_case_source_smoke.cir` (`sIn(vo=... va=... freq=... td=... theta=... phi=...)` mixed-case function spelling)
- `sin_keyword_space_before_paren_source_smoke.cir` (`SIN (vo=... va=... freq=... td=... theta=... phi=...)` with whitespace before `(`)

The threshold manifest in `python/tests/benchmarks/phase6/waveform_thresholds.json` is the review source of truth for this first correlation baseline.

The summary artifacts now also include a category-level rollup with hotspot nodes, a manifest quick-look hotspot overview, and an optional history-diff section driven by a previous summary JSON, so reviewers can quickly see whether `transmission-delay`, `mutual-inductance`, `josephson-junction`, or newer baseline buckets are regressing first, which node/deck is driving that category, and whether the latest run is better or worse than the last reviewed baseline.





























