# Fuzzing

Fuzz targets for the code that parses user input or file contents. They use
[cargo-fuzz](https://github.com/rust-fuzz/cargo-fuzz), which needs a nightly toolchain:

```sh
rustup toolchain install nightly --profile minimal
cargo +nightly install cargo-fuzz --locked
cp Cargo.lock fuzz/Cargo.lock   # the dependency versions the workspace is tested with
cargo +nightly fuzz run trigger -- -max_total_time=60
```

| Target            | Input                                    | Checks                                                             |
| ----------------- | ---------------------------------------- | ------------------------------------------------------------------ |
| `trigger`         | bind triggers (`[binds]` keys)           | no panic; a single key displays as text that parses back to itself |
| `action`          | actions, including `Transform` output    | no panic                                                           |
| `query`           | the picker query, with arbitrary columns | no panic                                                           |
| `status_template` | status-line templates                    | no panic                                                           |
| `markdown`        | Markdown preview of any file             | no panic, at any width                                             |
| `mermaid`         | Mermaid diagrams rendered as text        | no panic, at any width                                             |

CI runs each target for a short time on every push (`fuzz` job in `.github/workflows/ci.yml`).
A crash leaves its input in `fuzz/artifacts/<target>/`: add it as a regression test next to
the code, then fix the bug. `fuzz/corpus/`, `fuzz/artifacts/` and `fuzz/Cargo.lock` are not
committed.

Found so far: control characters and `@-…` chords breaking the trigger round trip
(`binds.rs`), and a panic in `mermaid-text` 0.57 on non-ASCII class names, fixed upstream in
0.58 (`utils/mermaid.rs`).
