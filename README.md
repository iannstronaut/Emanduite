# Emanduite

Desktop-first RAD workspace untuk merancang proyek admin panel. Milestone A
mencakup Blueprint v1, workspace SQLite, configuration tools, safe migration,
workflow runner terkontrol, diagnostics, recovery, dan redacted support bundle.
Generator Next.js dikerjakan setelah fondasi desktop ini stabil.

## Development

Prasyarat: Node.js 22+, Rust 1.97+, dan dependency sistem Tauri v2.

```powershell
npm install
npm run tauri dev
```

Quality gate lokal:

```powershell
npm run phase1:check
npm run phase2:check
npm run phase3:check
npm run phase4:check
cargo clippy --manifest-path .\src-tauri\Cargo.toml --all-targets -- -D warnings
npm run tauri -- build --no-bundle
```

Konteks implementasi tersedia di `docs/dev/Phase-1.md` sampai
`docs/dev/Phase-4.md`.
