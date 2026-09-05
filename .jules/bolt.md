## 2024-11-20 - Global `cargo fmt` Noise
**Learning:** Running `cargo fmt` globally on this project results in thousands of lines of unrelated formatting diffs, polluting pull requests and obscuring actual optimizations.
**Action:** When implementing targeted optimizations, avoid running global `cargo fmt`. If required to fix specific formatting issues, format only the modified lines or files rather than the entire project.

## 2024-11-20 - Unused Import Side Effects
**Learning:** Adding a trait import (e.g. `use std::fmt::Write as _;`) globally to every file out of convenience causes `unused import` warnings in files that don't actually use the trait, which can fail strict CI pipelines.
**Action:** Only add required trait imports directly into the files and scopes where they are actually used.
