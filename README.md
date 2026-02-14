<div align="center">

<br>

<img src="https://img.shields.io/badge/lang-Rust-dea584?style=for-the-badge&logo=rust&logoColor=white" alt="Rust">
<img src="https://img.shields.io/badge/lines-2,909-blue?style=for-the-badge" alt="Lines">
<img src="https://img.shields.io/badge/tests-137_passing-brightgreen?style=for-the-badge" alt="Tests">
<img src="https://img.shields.io/badge/benchmarks-11-blueviolet?style=for-the-badge" alt="Benchmarks">
<img src="https://img.shields.io/badge/license-MIT-yellow?style=for-the-badge" alt="License">

<br><br>

# `$ cp`

**A modern, GNU-compatible file copy utility rewritten in Rust.**

Zero-copy kernel I/O &bull; Parallel directory traversal &bull; Sparse file detection &bull; Reflink support

<br>

```
cargo build --release
```

<br>

</div>

---

<br>

## Highlights

<table>
<tr>
<td width="50%">

### &nbsp; Zero-Copy Kernel I/O

Uses `copy_file_range`, `sendfile`, and `FICLONE` (reflink) syscalls to avoid unnecessary user-space memory copies. Automatic cascading fallback.

</td>
<td width="50%">

### &nbsp; Parallel Directory Copy

Raw `openat` / `mkdirat` / `readdir` syscalls with Rayon-powered parallelism. Auto-switches at **64 entries**.

</td>
</tr>
<tr>
<td>

### &nbsp; Sparse File Detection

`SEEK_HOLE` / `SEEK_DATA` to preserve file holes.
Supports `--sparse=auto|always|never`.

</td>
<td>

### &nbsp; Reflink / CoW

Instant copy-on-write cloning on Btrfs, XFS via `FICLONE` ioctl. Transparent fallback when unsupported.

</td>
</tr>
<tr>
<td>

### &nbsp; Full Metadata Preservation

Mode, ownership, timestamps (nanosecond), xattr, ACL, and hard links with correct ordering to prevent permission races.

</td>
<td>

### &nbsp; Security-Hardened

Same-file detection via inode, symlink loop protection, TOCTOU-safe operations, setuid handling, path traversal prevention.

</td>
</tr>
</table>

<br>

---

<br>

## Benchmarks vs GNU cp

<div align="center">

_Averaged over 3 runs &times; 5 iterations &mdash; Linux 6.18 &mdash; release profile (`opt-level=3`, LTO)_

</div>

<br>

| &nbsp; | Benchmark | GNU cp | Ours | Speedup |
|:---:|:---|---:|---:|:---:|
| &nbsp; | **Many small files** &nbsp; `1000 &times; 1 KB` | `20.0 ms` | `12.9 ms` | **`1.6x`** |
| &nbsp; | **Recursive archive** &nbsp; `500 &times; 4 KB, -a` | `12.9 ms` | `8.1 ms` | **`1.6x`** |
| &nbsp; | **Mixed sizes** &nbsp; `50&times;1K + 30&times;100K + 15&times;1M + 5&times;10M` | `53.3 ms` | `48.7 ms` | **`1.1x`** |
| &nbsp; | **Large file** &nbsp; `100 MB single file` | `79.2 ms` | `77.1 ms` | `1.0x` |
| &nbsp; | **Deep tree** &nbsp; `5 lvl &times; 4 dirs &times; 10 files` | `213 ms` | `204 ms` | `1.0x` |
| &nbsp; | **Hardlink-heavy** &nbsp; `50 files &times; 20 links` | `10.5 ms` | `10.8 ms` | `1.0x` |
| &nbsp; | **Symlink-heavy** &nbsp; `100 files + 400 symlinks` | `7.6 ms` | `7.8 ms` | `1.0x` |
| &nbsp; | **Sparse file** &nbsp; `100 MB, 50 MB hole` | `1.4 ms` | `2.2 ms` | `0.7x`&ast; |

> &ast; Sparse scan overhead from `SEEK_HOLE` / `SEEK_DATA` probing on small files.

<br>

<details>
<summary><b>Parallel threshold sweep</b></summary>
<br>

| Files in directory | Time |
|---:|---:|
| 32 | `2.8 ms` |
| 64 | `3.3 ms` |
| 128 | `3.4 ms` |
| 256 | `5.5 ms` |

Parallel I/O kicks in at **64 entries** (`PARALLEL_THRESHOLD` in `src/dir.rs`).

</details>

<details>
<summary><b>Startup overhead</b></summary>
<br>

**~1.8 ms** for a 1-byte file copy (50 runs average).

</details>

<br>

---

<br>

## Architecture

```
                        ┌─────────────────────────────────────────────────────────┐
                        │                     Copy Pipeline                       │
                        ├───────────┬──────────────┬──────────────┬──────────────┤
                        │           │              │              │              │
                        │  CLI      │   Target     │  Directory   │   Metadata   │
                        │  Parse    │  Resolution  │    Walk      │    Sync      │
                        │           │              │              │              │
                        │  Clap     │  same-file   │  fast path:  │  xattr       │
                        │  derives  │  self-copy   │  openat      │  chown       │
                        │  flags    │  path checks │  readdir     │  chmod       │
                        │  into     │              │  mkdirat     │  utimensat   │
                        │  Options  │              │              │  ACL         │
                        │           │              │  slow path:  │              │
                        │           │              │  walkdir     │              │
                        └───────────┴──────────────┴──────┬───────┴──────────────┘
                                                          │
                                              ┌───────────▼───────────┐
                                              │     Copy Engine       │
                                              │                       │
                                              │  FICLONE (reflink)    │
                                              │       ↓ fail          │
                                              │  copy_file_range      │
                                              │       ↓ fail          │
                                              │  sendfile             │
                                              │       ↓ fail          │
                                              │  read / write         │
                                              └───────────────────────┘
```

<br>

---

<br>

## Usage

```bash
# Basic copy
cp source.txt dest.txt

# Recursive copy preserving everything
cp -a my_project/ backup/

# Copy with progress bar
cp --progress large_file.iso /mnt/usb/

# CoW reflink (instant on Btrfs/XFS)
cp --reflink=auto vm_disk.qcow2 snapshot.qcow2

# Sparse-aware copy
cp --sparse=always database.img /backup/

# Debug mode — shows which copy method was used
cp --debug file.dat /dst/
```

<br>

<details>
<summary><b>All CLI flags</b></summary>
<br>

| Flag | Description |
|:---|:---|
| `-a, --archive` | Same as `-dR --preserve=all` |
| `-R, -r, --recursive` | Copy directories recursively |
| `-p` | Preserve mode, ownership, timestamps |
| `-f, --force` | Remove destination before copy if needed |
| `-n, --no-clobber` | Do not overwrite existing files |
| `-u, --update` | Copy only when source is newer |
| `-v, --verbose` | Explain what is being done |
| `-l, --link` | Hard link files instead of copying |
| `-s, --symbolic-link` | Create symlinks instead of copying |
| `-L, --dereference` | Always follow symlinks in source |
| `-P, --no-dereference` | Never follow symlinks in source |
| `--preserve=ATTR` | Preserve: mode, ownership, timestamps, links, xattr, all |
| `--no-preserve=ATTR` | Don't preserve specified attributes |
| `--sparse=WHEN` | Sparse file creation: `auto`, `always`, `never` |
| `--reflink=WHEN` | CoW cloning: `auto`, `always`, `never` |
| `--backup[=CONTROL]` | Backup: `numbered`, `existing`, `simple`, `none` |
| `-S, --suffix` | Override backup suffix (default: `~`) |
| `-x, --one-file-system` | Stay on the same filesystem |
| `-t, --target-directory` | Copy all sources into directory |
| `-T, --no-target-directory` | Treat destination as normal file |
| `--parents` | Replicate source path structure under dest |
| `--attributes-only` | Copy metadata only, no file data |
| `--remove-destination` | Remove each destination before copy |
| `--debug` | Show copy method used (implies `-v`) |
| `--progress` | Show progress bar during copy |

</details>

<br>

---

<br>

## Test Suite

```
137 tests + 11 benchmarks — all passing
```

<br>

| Suite | Tests | &nbsp; | Suite | Tests |
|:---|---:|:---:|:---|---:|
| `security` | **28** | | `unit_metadata` | **8** |
| `unit_copy` | **15** | | `unit_parallel` | **8** |
| `unit_util` | **15** | | `unit_backup` | **7** |
| `integration` | **12** | | `unit_engine` | **7** |
| `unit_options` | **12** | | `unit_sparse` | **4** |
| `unit_dir` | **10** | | `benchmarks` | **11** |

```bash
# Run all tests
cargo test --release

# Run benchmarks with output
cargo test --release bench_ -- --nocapture
```

<br>

---

<br>

## Project Structure

```
src/
├── main.rs ··········· Entry point, CLI dispatch                125 lines
├── cli.rs ············ Clap-derived CLI definitions              201 lines
├── options.rs ········ CopyOptions resolution from flags         236 lines
├── dir.rs ············ Recursive directory copy (fast + slow)   1030 lines
├── copy.rs ··········· Single file copy logic                    335 lines
├── engine.rs ········· Copy engine (reflink/cfr/sendfile/rw)     199 lines
├── metadata.rs ······· Permission, xattr, ACL, timestamps       225 lines
├── sparse.rs ········· Sparse file hole detection + copy         192 lines
├── error.rs ·········· Error types (thiserror)                   145 lines
├── util.rs ··········· Path utilities, target resolution         120 lines
├── backup.rs ········· Backup file creation                       77 lines
└── progress.rs ······· Progress bar (indicatif)                   24 lines

tests/
├── common/mod.rs ····· Shared test harness (Env fixture)         194 lines
└── 12 test files ····· 137 tests + 11 benchmarks               2439 lines

html/
└── index.html ········ Project showcase page                    1475 lines
```

<br>

---

<br>

<div align="center">

Built with Rust &mdash; MIT License

</div>
