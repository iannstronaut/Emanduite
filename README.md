# Emanduite

Emanduite adalah workspace **desktop-first** untuk merancang database dan
admin panel sebelum aplikasi web dibuat. Proyek ini dibuat untuk mengurangi
pekerjaan berulang saat memulai aplikasi operasional: merancang tabel,
relasi, peran, hak akses, menu, serta CRUD secara manual dan tidak konsisten.

Pengguna bekerja dari aplikasi desktop untuk membuat sebuah Blueprint v1
berbasis SQLite. Blueprint tersebut menjadi sumber konfigurasi yang dapat
ditinjau, dimigrasikan secara aman, dan pada tahap berikutnya digenerate
menjadi aplikasi admin Next.js. Pendekatan ini menjaga desain database dan
aturan akses tetap jelas sebelum kode frontend/backend dihasilkan.

## Yang dikerjakan Emanduite

- Membuat dan membuka proyek SQLite, termasuk tabel sistem dan akun
  superadmin bawaan.
- Merancang entitas, kolom, relasi, menu, role, dan permission dari satu
  Blueprint v1.
- Menjadikan entitas yang tampil sebagai menu sebagai resource akses, sehingga
  permission dapat diberikan per role tanpa memasukkan resource satu per satu.
- Menjalankan preview dan perubahan skema SQLite dengan alur yang terkontrol.
- Menghasilkan starter admin Next.js dengan Prisma, autentikasi, akses berbasis
  role, dan antarmuka bergaya shadcn/ui.
- Menyediakan AI Design untuk menyusun usulan skema yang tetap harus ditinjau
  dan disetujui pengguna sebelum diterapkan.

Milestone A mencakup Blueprint v1, workspace SQLite, configuration tools, safe
migration, workflow runner terkontrol, diagnostics, recovery, dan redacted
support bundle. Generator Next.js dikembangkan setelah fondasi desktop ini
stabil.

## Penggunaan OpenAI GPT-5.6 Sol

AI Design dapat menggunakan OpenAI melalui protokol **OpenAI Compatible / Chat
Completions**. Untuk memakai GPT-5.6 Sol, buka **Settings > AI Provider** pada
proyek yang sedang aktif, lalu isi:

| Field | Nilai |
| --- | --- |
| Base URL | `https://api.openai.com/v1` |
| Model | `gpt-5.6-sol` |
| API key | API key OpenAI Anda |

Klik **Save AI settings**. Emanduite menyimpan API key di OS keyring, bukan di
file Blueprint. Setelah koneksi tersimpan, daftar model akan dimuat dari
provider; Anda dapat memilih `gpt-5.6-sol` dari dropdown atau mengisinya
langsung bila belum muncul di daftar.

Selanjutnya buka **AI Design**, tulis kebutuhan aplikasi dalam bahasa biasa,
misalnya tabel produk, pemasok, dan pergerakan stok. Emanduite mengirimkan
permintaan desain beserta konteks Blueprint yang ringkas ke endpoint
`/chat/completions` pada Base URL tersebut. Hasilnya berupa usulan tabel dan
relasi yang dapat diedit, dipreview, lalu **Apply** hanya setelah pengguna
menyetujui perubahan.

Jangan memasukkan API key ke repository, `.env` proyek, atau file Blueprint.
Jika key perlu diganti atau dicabut, gunakan **Remove stored key** pada halaman
AI Provider, kemudian buat API key baru di akun OpenAI.

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
