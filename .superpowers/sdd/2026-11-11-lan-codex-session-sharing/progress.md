# SDD ledger — plan: docs/superpowers/plans/2026-11-11-lan-codex-session-sharing.md

Ruling: Keep task headings prefixed with the literal English word `Task` while keeping descriptions in Chinese — the installed plan tooling only recognizes `Task N` headings — the cost if wrong is that future plan runners cannot extract scoped task briefs.
Pre-flight: Task 2 produces `SnapshotV1` consumed by Tasks 3 and 5; Task 3's `read_snapshot` returns `SnapshotV1` consumed by Task 4 and Task 8; Task 5's `Invitation` and transfer methods are consumed by Tasks 6 and 7; Task 8's `ImportRecord` is consumed by Task 9. Types align by name; exact nested fields will be defined during the task-owned implementations.
Task 1: Ruling: Use generated Tauri scaffold outputs as setup-generated files — bootstrap files are explicitly project scaffolding and the TDD skill allows generated code/configuration exceptions — the cost if wrong is that template defaults may need review beyond the smoke test.
Task 1: complete (tests: `npm test -- --run` → 2 passed; `npm run build` → PASS; `cargo check --manifest-path src-tauri/Cargo.toml` → PASS; `npm run tauri:build` → PASS, macOS arm64 `.app` and `.dmg`)
