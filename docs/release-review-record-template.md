# Release Review Record Template

Use this template to capture go/no-go decisions for candidate releases.

## 1. Candidate identity

```md
Candidate commit:
Candidate branch:
Review date:
Reviewer:
Target platform:
Change scope:
```

## 2. Validation commands actually run

```md
Rust validation command:
Python validation command:
Release artifact command:
CLI contract command:
Python API contract command:
Report schema contract command:
Week3 one-command baseline command:
```

## 3. Evidence artifacts

```md
Release artifact directory:
Release artifact manifest:
Release artifact README:
CLI contract baseline + diff summary:
Python API baseline + diff summary:
Report schema baseline + diff summary:
Week3 pipeline output root:
Week3 review manifest:
Week3 validation report:
Week3 summary markdown:
```

## 4. Compatibility decision

```md
CLI compatibility risk:
Python API compatibility risk:
Report schema compatibility risk:
Default behavior risk:
```

## 5. Sign-off

```md
Packaging DRI:
QA reviewer:
Documentation reviewer:
Release policy update required: yes/no
Support matrix update required: yes/no
Known limitations update required: yes/no
```

## 6. Final outcome

```md
Decision: go / conditional / no-go
Blocking issues:
Follow-up owner:
Follow-up due date:
```

## 7. Reference checklist docs

- [release-artifact-readiness-checklist.md](./release-artifact-readiness-checklist.md)
- [sim-release-readiness-checklist.md](./sim-release-readiness-checklist.md)
- [release-policy.md](./release-policy.md)