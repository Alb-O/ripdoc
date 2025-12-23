# RipDoc Roadmap (keep this updated!)

OVERALL GOAL: Skelebuild UX Hardening

## P0 — Make `inject` work with heredoc/pipes by default

- [ ] Update `ripdoc skelebuild inject` to auto-read stdin when `<CONTENT>` is missing and stdin is **not** a TTY.
  - [ ] Detection: if positional `<CONTENT>` not provided AND `stdin.is_terminal()` is false, treat input as if `--from-stdin` was passed.
  - [ ] Ensure behavior works for:
    - [ ] `cat file | ripdoc skelebuild inject --at 0`
    - [ ] `ripdoc skelebuild inject --at 0 <<'EOF' ... EOF`
  - [ ] Ensure behavior does **not** trigger when:
    - [ ] User is typing in an interactive terminal (stdin is a TTY).
    - [ ] `<CONTENT>` is provided positionally.
- [ ] Add an explicit error message when `<CONTENT>` is missing and stdin **is** a TTY.
  - [ ] Replace/override the Clap “required arguments were not provided: <CONTENT>” path for this command.
  - [ ] Error text MUST include exact next steps (copy/paste ready):
    - [ ] `ripdoc skelebuild inject --from-stdin --at 0 <<'EOF' ... EOF`
    - [ ] `ripdoc skelebuild inject --at 0 "your content here"`
- [ ] Add CLI help examples for `inject` that include all supported input modes.
  - [ ] Include examples for: positional, `--from-stdin` pipe, `--from-stdin` heredoc.
  - [ ] Ensure `ripdoc skelebuild inject --help` shows these examples.

## P0 — Make `add-changed` explain empty results (no-hunks) deterministically

- [ ] Extend `ripdoc skelebuild add-changed` to emit a structured “why empty” report when no hunks are found.
  - [ ] Always print the resolved revspec(s) used (the exact string passed).
  - [ ] Print counts in a fixed format:
    - [ ] total changed files discovered
    - [ ] total hunks discovered before filtering
    - [ ] files filtered out by `--only-rust`
    - [ ] hunks filtered out by `--only-rust`
- [ ] When `--only-rust` is present and it filtered everything, print a prescriptive message:
  - [ ] “All changes were filtered out by `--only-rust`” (exact phrase).
  - [ ] Print a list of the changed files that were excluded (at least first 20; if more, print “+N more”).
- [ ] Add a built-in hint generator for common revspec mistakes when empty results occur.
  - [ ] If the revspec range contains only non-rust file changes and `--only-rust` is set, print:
    - [ ] “Try expanding the range (e.g. `HEAD~2..HEAD`) or removing `--only-rust`.”
  - [ ] Do **not** guess a specific alternate range unless you can compute a safe one (see next task).
- [ ] (Optional but deterministic) If repository has at least one earlier commit in range with rust changes, compute and print one concrete suggestion.
  - [ ] Implement: walk backwards up to 50 commits from `HEAD` until a commit touches `*.rs`.
  - [ ] Suggest: `--git <that_commit>..HEAD` (exact command string).
  - [ ] If not found in 50 commits, print “No Rust-touching commit found in last 50 commits.”

## P0 — Canonicalize file entry keys so `add-file` targets always match later commands

- [ ] Change `add-file` storage format to include a canonical, stable match key:
  - [ ] Canonical key MUST be the **repo-root-relative** path using forward slashes (POSIX style), e.g. `crates/gala_proxy/src/push.rs`.
  - [ ] Always store the original absolute path as metadata, but do not use it as the primary match key.
- [ ] Update all “entry lookup by spec” paths (including `inject --after-target/--before-target`) to match file entries by:
  - [ ] exact canonical repo-relative path
  - [ ] exact absolute path (backward compatibility)
  - [ ] exact match on a canonicalized version of user input (normalize `./`, redundant separators)
- [ ] Update `add-file` output to print the exact canonical key in a machine-copyable line.
  - [ ] Format MUST be:
    - [ ] `Entry key: <canonical_key>`
  - [ ] Also print a ready-to-run inject command using that key:
    - [ ] `ripdoc skelebuild inject --after-target "<canonical_key>" --from-stdin <<'EOF' ... EOF`
- [ ] Add regression tests to ensure canonicalization and matching behave identically on:
  - [ ] `./crates/.../push.rs`
  - [ ] `crates/.../push.rs`
  - [ ] absolute path `/home/.../crates/.../push.rs`
  - [ ] mixed separators or redundant path segments.

## P1 — Remove confusion between “target” entries and “non-target” entries for insertion commands

- [ ] Modify `--after-target` / `--before-target` to match any entry that has a stable “entry key”, not only rustdoc targets.
  - [ ] Define “stable entry key” for:
    - [ ] rustdoc targets (existing)
    - [ ] raw source file entries (from `add-file`)
    - [ ] injected blocks (if they have IDs/labels; if not, do not include)
- [ ] If you intentionally want “target-only” semantics, implement *both* of these flags:
  - [ ] `--after-entry` / `--before-entry` (matches any entry key)
  - [ ] `--after-target` / `--before-target` (targets only)
  - [ ] In that case: update help text to clearly state the difference, with one concrete example for each.
- [ ] Update error messages when no match is found to include:
  - [ ] “Available keys:” followed by the first 10 keys from the current doc.
  - [ ] “Run: `ripdoc skelebuild status --keys`” (see next section).

## P1 — Make `add` failures self-healing with exact, copy-pasteable suggestions

- [ ] When `ripdoc skelebuild add` fails due to “No path match found”, print top suggestions automatically.
  - [ ] Extract the last segment of the provided spec (e.g. `rewrite_history_stateful`).
  - [ ] Search in rustdoc inventory for:
    - [ ] exact name matches
    - [ ] suffix matches on path segments
  - [ ] Print up to 5 suggestions, each on its own line, each fully qualified and copy-pasteable.
- [ ] Add a deterministic alias resolution attempt for common crate-prefix mistakes.
  - [ ] If user spec starts with `<something>::` and there exists a `crate::` equivalent where you replace the first segment with `crate`, attempt match.
  - [ ] If match succeeds, print:
    - [ ] “Interpreted `<original>` as `<resolved>`” and proceed (unless `--strict` is set; see next task).
- [ ] Add `--strict` flag to `add` (and possibly other commands) to disable all heuristics.
  - [ ] Default behavior: heuristics ON (agent-friendly).
  - [ ] With `--strict`: no auto-rewrite; only print suggestions.

## P2 — Make “status” immediately actionable (keys first, minimal friction)

- [ ] Add `ripdoc skelebuild status --keys` output mode.
  - [ ] It MUST print, for each entry, a single line containing:
    - [ ] entry index
    - [ ] entry type (target/raw-source/injection/other)
    - [ ] exact stable entry key (what users should pass to `--after-*` / `--before-*`)
  - [ ] Output MUST be stable and machine-parsable (fixed columns or delimiter).
- [ ] When any command fails due to missing/unknown target/entry, include in the error message:
  - [ ] `Run: ripdoc skelebuild status --keys`
  - [ ] plus an inline preview of the first 5 keys (to reduce one extra command).

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

