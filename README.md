# Emanduite

Desktop-first RAD workspace untuk merancang proyek admin panel. Fase ketiga
menambahkan Visual Schema Editor yang aman, migration preview/backup SQLite,
Entity/Form Config, authorization matrix, Auth Config, Global Config, dan
Extension Editor. Generator Next.js dikerjakan setelah fondasi desktop stabil.

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
cargo clippy --manifest-path .\src-tauri\Cargo.toml --all-targets -- -D warnings
npm run tauri -- build --no-bundle
```

Konteks implementasi tersedia di `docs/dev/Phase-1.md`, `docs/dev/Phase-2.md`,
dan `docs/dev/Phase-3.md`.
