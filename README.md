# Emanduite

Desktop-first RAD workspace untuk merancang proyek admin panel. Fase pertama
berfokus pada Tauri, Blueprint v1, secure secret boundary, dan SQLite; generator
Next.js dikerjakan setelah fondasi desktop stabil.

## Development

Prasyarat: Node.js 22+, Rust 1.97+, dan dependency sistem Tauri v2.

```powershell
npm install
npm run tauri dev
```

Quality gate lokal:

```powershell
npm run phase1:check
cargo clippy --manifest-path .\src-tauri\Cargo.toml --all-targets -- -D warnings
npm run tauri -- build --no-bundle
```

Konteks implementasi lengkap tersedia di `docs/dev/Phase-1.md`.
