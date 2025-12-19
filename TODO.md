# RipDoc Roadmap (keep this updated!)

Feel free to add more intermediate sub-tasks as needed.

- [x] organise into subcommands
- [x] show `// ...` between search results with large distance appart
- [ ] functionality/subcommands for fetching:
  - [ ] examples
  - [x] READMEs
- [x] 'with filename' support on module list (-l) show the originating .rs path for each
  - [x] json format support for module/symbol list
    - [ ] TOON-like compact format
  - [ ] ability to dump source code files easily (without having to know path of cache registry)
    - [ ] possibly filter out/exclude comments/docstrings on source code dump as the docs would likely already be in context
- [x] Allow 'or' searches, e.g. `ripdoc print gix --search "init|clone|fetch|remote|config"`
- [ ] Allow searching version numbers without specifying last digit e.g. `ripdoc print bat@0.24`

---

## Known Issue: Hang on Certain Crates (e.g., ratatui) - FIXED

### Summary

When running `ripdoc print` on certain crates, the process hangs indefinitely without producing any output. This was observed specifically with the `ratatui` crate due to re-export cycles or redundant module re-exports.

### Resolution

A `visited` set was added to `RenderState` to track modules that have already been rendered. If a module is encountered again (e.g., via re-export), it is skipped to prevent infinite recursion and redundant output. This significantly improves performance on crates with many re-exports and resolves the hang.
