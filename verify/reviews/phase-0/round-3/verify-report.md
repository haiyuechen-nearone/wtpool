# Verify report, phase 0

**Verdict: PASS**

- Commit: the round 3 freeze
- spec-base: spec-base
- Claude Code: 2.1.306 (Claude Code)
- Run: phase0-r3

## Needs you

Nothing.

## Oracle changes since the freeze

```
none
```

## verify/accepted.txt since the freeze

```
none
```

## Spec changes since spec-base

```
none
```

## Failures

None.

## Flaky checks

None: no layer of this phase retries.

## Budgets

None in this phase.

## Coverage

42 of 42 changed lines covered by unit or int. Lines no oracle layer covers:

- crates/wt/src/main.rs:347 (unit only)
- crates/wt/src/main.rs:353 (unit only)

## Check results

| Check | Layer | Result | Seconds |
|---|---|---|---|
| `harness:self_test` | harness | pass | 3.8 |
| `lint__no_suppressions` | static | pass | 0.0 |
| `oracle_unchanged` | harness | pass | 0.0 |
| `repo_context__should_be_unchanged` | harness | pass | 0.0 |
| `static:clippy` | static | pass | 0.6 |
| `static:clippy_all_features` | static | pass | 0.6 |
| `static:doc_links` | static | pass | 0.0 |
| `static:fmt` | static | pass | 0.1 |
| `static:rustdoc` | static | pass | 0.7 |
| `int:build` | int | pass | 1.0 |
| `int:build_release` | int | pass | 3.5 |
| `release__should_have_no_verify_feature` | int | pass | 0.0 |
| `unit` | unit | pass | 25.6 |
| `version__should_list_version_commit_and_features` | int | pass | 0.0 |
| `cover` | cover | pass | 0.0 |
| `isolation__left_no_trace` | report | pass | 0.0 |
| `trace_complete` | report | pass | 0.0 |
