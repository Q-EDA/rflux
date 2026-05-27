# Release Notes Template

Use this template when preparing candidate or formal release notes.

## 1. Release metadata

```md
Release version:
Release date:
Commit/tag:
Author:
Release level: Dev Snapshot / Alpha / Beta / GA
Scope summary:
```

## 2. Highlights

```md
-
-
-
```

## 3. Fixes and behavior changes

```md
### Fixes
-

### Behavior changes
-
```

## 4. Compatibility and contract impact

Record all public contract impact here. If no change, explicitly write `none`.

```md
CLI contract diff summary:
Python API contract diff summary:
Report schema contract diff summary:

CLI contract baseline path:
Python API baseline path:
Report schema baseline path:
```

Recommended checks:

```bash
uv run python python/scripts/export_cli_command_surface.py --check
uv run python python/scripts/export_python_api_surface.py --check
uv run python python/scripts/export_report_schema_surface.py --check
```

## 5. Quality baseline and regression status

Use this section when release scope touches Week 3 timing/verify/sim quality inputs, thresholds, summary logic, or generated review artifacts.

```md
Week3 one-command check run: yes/no
Week3 no-regression enforced: yes/no
Week3 regression tolerance:
Week3 pipeline output root:
Week3 review manifest path:
Week3 validation report path:
Week3 summary markdown path:
Week3 decision summary:
```

Recommended command:

```bash
uv run python python/scripts/generate_week3_golden_results.py --validate-pass --validate-no-regression --regression-tolerance 0.0
```

## 6. Known limitations and risks

```md
-
-
```

## 7. Upgrade and rollback notes

```md
Upgrade actions:
Rollback actions:
User-facing caveats:
```

## 8. Evidence links

```md
Checklist record:
Artifact review bundle:
CI run link:
Issue links:
```