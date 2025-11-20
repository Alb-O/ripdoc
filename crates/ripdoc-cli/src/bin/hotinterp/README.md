## Experimental Hot Interpreter (PoC)

The workspace also bundles an experimental Rust “interpreter” that demonstrates Subsecond-powered hotpatching. The binary is compile-gated behind the `hot-interpreter` feature and lives inside `ripdoc-cli`.

```sh
cargo run -p ripdoc-cli --features hot-interpreter --bin hotinterp path/to/script.rs
```

`hotinterp` watches the provided script file, generates a tiny helper crate that links against [`subsecond`](https://crates.io/crates/subsecond), and reloads the resulting dynamic library whenever you save. The entrypoint is wrapped in `subsecond::call`, so when you rebuild the generated crate (triggered automatically on save) the running session is rewound to the interpreter entrypoint instead of restarting the host process.

Scripts are regular Rust modules. Define `pub fn hot_main(ctx: &mut ScriptContext) -> anyhow::Result<()>` and use the shared `ScriptContext` helper to stash state across reloads:

```rust
use crate::ScriptContext; // provided by the generated crate

pub fn hot_main(ctx: &mut ScriptContext) -> anyhow::Result<()> {
    let tick = ctx.cycle();
    ctx.emit_line(format!("tick #{tick}"));

    if tick == 0 {
        ctx.set_number("sum", 0.0);
    }

    let sum = ctx.number("sum").unwrap_or(0.0) + 1.5;
    ctx.set_number("sum", sum);
    ctx.emit_line(format!("running sum: {sum}"));
    ctx.set_text("status", "alive");
    Ok(())
}
```

`ScriptContext` offers a handful of batteries-included helpers:

- `emit_line(&str)` buffers a line that `hotinterp` prints after each run.
- `cycle()` reports how many times the script has executed (across reloads).
- `set_number` / `number` store simple floating-point registers.
- `set_text` / `text` do the same for string state.

Flags:

- `--once` runs the script one time and exits.
- `--release` compiles the generated helper crate in release mode (default is dev for faster rebuilds).

This PoC is intentionally small, but it sketches the workflow of editing a Rust “script”, saving, and letting Subsecond provide the hotpatch magic without restarting the host process.
