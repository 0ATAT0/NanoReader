# Working on Quiet Reader

Read README.md and the modules on the path you will change before editing.

- Keep API and credentials in Rust; treat document HTML as untrusted.
- Keep persisted state in the SQLite owner; avoid parallel stores and generic wrapper layers.
- Keep the frontend small, with semantic design tokens and accessible native controls.
- No private library content, tokens or local app data in this repository.
- Run npm run check, npm test, npm run build, cargo fmt --check, cargo clippy and cargo test before submitting changes.
- Document user-visible API limitations. Do not invent undocumented synchronization behavior.
