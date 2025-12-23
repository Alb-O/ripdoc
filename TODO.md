# RipDoc Roadmap (keep this updated!)

OVERALL GOAL: Skelebuild UX Hardening

## P0 — Make `inject` work with heredoc/pipes by default ✅ DONE

- [x] Update `ripdoc skelebuild inject` to auto-read stdin when `<CONTENT>` is missing and stdin is **not** a TTY.
  - [x] Detection: if positional `<CONTENT>` not provided AND `stdin.is_terminal()` is false, treat input as if `--from-stdin` was passed.
  - [x] Ensure behavior works for:
    - [x] `cat file | ripdoc skelebuild inject --at 0`
    - [x] `ripdoc skelebuild inject --at 0 <<'EOF' ... EOF`
  - [x] Ensure behavior does **not** trigger when:
    - [x] User is typing in an interactive terminal (stdin is a TTY).
    - [x] `<CONTENT>` is provided positionally.
- [x] Add an explicit error message when `<CONTENT>` is missing and stdin **is** a TTY.
  - [x] Replace/override the Clap "required arguments were not provided: <CONTENT>" path for this command.
  - [x] Error text MUST include exact next steps (copy/paste ready):
    - [x] `ripdoc skelebuild inject --from-stdin --at 0 <<'EOF' ... EOF`
    - [x] `ripdoc skelebuild inject --at 0 "your content here"`
- [x] Add CLI help examples for `inject` that include all supported input modes.
  - [x] Include examples for: positional, `--from-stdin` pipe, `--from-stdin` heredoc.
  - [x] Ensure `ripdoc skelebuild inject --help` shows these examples.

## P0 — Make `add-changed` explain empty results (no-hunks) deterministically ✅ DONE

- [x] Extend `ripdoc skelebuild add-changed` to emit a structured "why empty" report when no hunks are found.
  - [x] Always print the resolved revspec(s) used (the exact string passed).
  - [x] Print counts in a fixed format:
    - [x] total changed files discovered
    - [x] total hunks discovered before filtering
    - [x] files filtered out by `--only-rust`
    - [x] hunks filtered out by `--only-rust`
- [x] When `--only-rust` is present and it filtered everything, print a prescriptive message:
  - [x] "All changes were filtered out by `--only-rust`" (exact phrase).
  - [x] Print a list of the changed files that were excluded (at least first 20; if more, print "+N more").
- [x] Add a built-in hint generator for common revspec mistakes when empty results occur.
  - [x] If the revspec range contains only non-rust file changes and `--only-rust` is set, print:
    - [x] "Try expanding the range (e.g. `HEAD~2..HEAD`) or removing `--only-rust`."
  - [x] Do **not** guess a specific alternate range unless you can compute a safe one (see next task).
- [x] (Optional but deterministic) If repository has at least one earlier commit in range with rust changes, compute and print one concrete suggestion.
  - [x] Implement: walk backwards up to 50 commits from `HEAD` until a commit touches `*.rs`.
  - [x] Suggest: `--git <that_commit>..HEAD` (exact command string).
  - [x] If not found in 50 commits, print "No Rust-touching commit found in last 50 commits."

## P0 — Canonicalize file entry keys so `add-file` targets always match later commands ✅ DONE

- [x] Change `add-file` storage format to include a canonical, stable match key:
  - [x] Canonical key MUST be the **repo-root-relative** path using forward slashes (POSIX style), e.g. `crates/gala_proxy/src/push.rs`.
  - [x] Always store the original absolute path as metadata, but do not use it as the primary match key.
- [x] Update all "entry lookup by spec" paths (including `inject --after-target/--before-target`) to match file entries by:
  - [x] exact canonical repo-relative path
  - [x] exact absolute path (backward compatibility)
  - [x] exact match on a canonicalized version of user input (normalize `./`, redundant separators)
- [x] Update `add-file` output to print the exact canonical key in a machine-copyable line.
  - [x] Format MUST be:
    - [x] `Entry key: <canonical_key>`
  - [x] Also print a ready-to-run inject command using that key:
    - [x] `ripdoc skelebuild inject --after-target "<canonical_key>" --from-stdin <<'EOF' ... EOF`
- [x] Add regression tests to ensure canonicalization and matching behave identically on:
  - [x] `./crates/.../push.rs`
  - [x] `crates/.../push.rs`
  - [x] absolute path `/home/.../crates/.../push.rs`
  - [x] mixed separators or redundant path segments.

## P1 — Remove confusion between "target" entries and "non-target" entries for insertion commands ✅ DONE

- [x] Modify `--after-target` / `--before-target` to match any entry that has a stable "entry key", not only rustdoc targets.
  - [x] Define "stable entry key" for:
    - [x] rustdoc targets (existing)
    - [x] raw source file entries (from `add-file`)
    - [x] injected blocks (if they have IDs/labels; if not, do not include)
- [x] If you intentionally want "target-only" semantics, implement *both* of these flags:
  - [x] `--after-entry` / `--before-entry` (matches any entry key)
  - [x] `--after-target` / `--before-target` (targets only)
  - [x] In that case: update help text to clearly state the difference, with one concrete example for each.
  - Note: Decided to make `--after-target`/`--before-target` match any entry (targets + raw sources). This is more intuitive for users.
- [x] Update error messages when no match is found to include:
  - [x] "Available keys:" followed by the first 10 keys from the current doc.
  - [x] "Run: `ripdoc skelebuild status --keys`" (see next section).

## P1 — Make `add` failures self-healing with exact, copy-pasteable suggestions ✅ DONE

- [x] When `ripdoc skelebuild add` fails due to "No path match found", print top suggestions automatically.
  - [x] Extract the last segment of the provided spec (e.g. `rewrite_history_stateful`).
  - [x] Search in rustdoc inventory for:
    - [x] exact name matches
    - [x] suffix matches on path segments
  - [x] Print up to 5 suggestions, each on its own line, each fully qualified and copy-pasteable.
- [x] Add a deterministic alias resolution attempt for common crate-prefix mistakes.
  - [x] If user spec starts with `<something>::` and there exists a `crate::` equivalent where you replace the first segment with `crate`, attempt match.
  - [x] If match succeeds, print:
    - [x] "Interpreted `<original>` as `<resolved>`" and proceed (unless `--strict` is set; see next task).
- [x] Add `--strict` flag to `add` (and possibly other commands) to disable all heuristics.
  - [x] Default behavior: heuristics ON (agent-friendly).
  - [x] With `--strict`: no auto-rewrite; only print suggestions.

## P2 — Make "status" immediately actionable (keys first, minimal friction) ✅ DONE

- [x] Add `ripdoc skelebuild status --keys` output mode.
  - [x] It MUST print, for each entry, a single line containing:
    - [x] entry index
    - [x] entry type (target/raw-source/injection/other)
    - [x] exact stable entry key (what users should pass to `--after-*` / `--before-*`)
  - [x] Output MUST be stable and machine-parsable (fixed columns or delimiter).
- [x] When any command fails due to missing/unknown target/entry, include in the error message:
  - [x] `Run: ripdoc skelebuild status --keys`
  - [x] plus an inline preview of the first 5 keys (to reduce one extra command).
  - Note: Implemented as first 10 keys for better usability.

## P2 — Documentation + golden-path guide for agents

- [ ] Add a “Golden Path” section to the instructions that agents are expected to follow, as a strict sequence:
  - [ ] `new`
  - [ ] `add-changed` (with safe range guidance)
  - [ ] `status --keys`
  - [ ] `inject` (stdin + key usage)
  - [ ] `build`
- [ ] Add a “Common Failure Modes” section with exact symptom → exact fix mapping.
  - [ ] Include at minimum:
    - [ ] Clap missing `<CONTENT>` → “use `--from-stdin` or pipe; or rely on auto-stdin”
    - [ ] “No changed hunks found” → inspect revspec + `--only-rust`
    - [ ] “No path match found” → use printed suggestions or `list`/`status --keys`
    - [ ] “No target matches … after-target” → use canonical entry key from `add-file` output
- [ ] Add CI checks that run the Golden Path commands against a tiny fixture repo and assert:
  - [ ] no fatal errors
  - [ ] output includes the canonical key lines
  - [ ] injection via heredoc works without `--from-stdin` when stdin is not a TTY (if you implement auto-stdin)

## P2 — Test coverage (prevent regressions)

- [ ] Add integration tests for `inject` input modes:
  - [ ] positional content
  - [ ] piped stdin without `--from-stdin` (should succeed after auto-stdin change)
  - [ ] heredoc without `--from-stdin` (should succeed after auto-stdin change)
  - [ ] interactive TTY with missing content (should produce the explicit “use --from-stdin” error)
- [ ] Add integration tests for `add-file` key output and subsequent matching:
  - [ ] verify printed `Entry key:` is repo-relative
  - [ ] verify `inject --after-target "<Entry key>"` works
- [ ] Add integration tests for `add-changed` empty-report:
  - [ ] repo state where HEAD~1..HEAD changes only docs and `--only-rust` is set
  - [ ] assert output contains the structured counts and the explicit “filtered out by --only-rust” statement

