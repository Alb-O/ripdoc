# RipDoc Roadmap (keep this updated!)

Feel free to add more intermediate sub-tasks as needed.

- [ ] organise into subcommands
- [ ] functionality/subcommands for fetching:
  - [ ] examples
  - [ ] READMEs
- [x] 'with filename' support on module list (-l) show the originating .rs path for each
  - [ ] json/toon format support for module list
  - [ ] ability to dump source code files easily (without having to know path of cache registry)
    - [ ] possibly filter out/exclude comments/docstrings on source code dump as the docs would likely already be in context
- [ ] friendlier CLI UX
  - [ ] arg positioning flexibility
  - [ ] assumptive handling of minor mistakes that would otherwise cause errors/no-ops