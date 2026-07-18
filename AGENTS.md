# AGENTS.md — bili-sync

## Quick Commands

| Action | Command |
|--------|---------|
| Build frontend | `just build-frontend`  (or `cd web && bun run build`) |
| Build release binary | `just build`  (or `./scripts/build_local.sh binary release`) |
| Build debug binary | `just build-debug` |
| Build Docker image | `just build-docker` |
| Build debug Docker | `just build-docker-debug` |
| Run dev (builds frontend first) | `just debug` |
| Run binary directly | `cargo run` (requires frontend built first) |
| Run tests | `cargo test` |
| Lint (backend) | `cargo clippy -- -D warnings` |
| Format check (backend) | `cargo +nightly fmt --check` |
| Format (backend) | `cargo +nightly fmt` |
| Lint (frontend) | `cd web && bun run lint` |
| Typecheck (frontend) | `cd web && bun run check` |
| Format (frontend) | `cd web && bun run format` |

## Monorepo Structure

```
├── crates/
│   ├── bili_sync/           # Main binary (bili-sync-rs)
│   ├── bili_sync_entity/    # SeaORM entities (tables)
│   └── bili_sync_migration/ # SeaORM migrations (m2024..._*.rs)
├── web/                     # SvelteKit frontend (bun, Svelte 5, TS, Tailwind)
├── scripts/
│   └── build_local.sh       # Cross-platform binary/Docker builder
├── bili-sync/               # Legacy? contains crates/bili_sync/
└── docs/                    # VitePress docs
```

- **Entry point**: `crates/bili_sync/src/main.rs` → `bili-sync-rs` binary
- **Frontend**: Built to `web/build/`, embedded at compile time via `build.rs` + `rust-embed-for-web`
- **Database**: SQLite via SeaORM, migrations in `bili_sync_migration/src/`
- **Config**: Stored in DB since v2.6.0, managed via WebUI

## CI Pipeline (`.github/workflows/`)

| Workflow | Trigger | Steps |
|----------|---------|-------|
| `pr-check.yaml` | PR/push to `rework-ci` | `cargo +nightly fmt --check` → `cargo clippy -D warnings` → `cargo test` → `cd web && bun run lint` |
| `build-binary.yaml` | Called by release | Builds 6 targets: Linux x64/arm64/armv7, macOS x64/arm64, Windows x64 |
| `release-build.yaml` | Tag push `v*` | Calls build-binary → GitHub Release + Docker Hub push (linux/amd64,arm64,arm/v7) |

**Order matters**: `fmt check → clippy → test` (backend) | `lint` (frontend)

## Key Quirks

- **Rust toolchain pinned**: `1.97.0` via `rust-toolchain.toml` (CI uses `+nightly` only for `fmt`)
- **Frontend uses `bun`**, not npm/pnpm — `bun install --frozen-lockfile`, `bun run build`
- **Frontend must be built before backend** — `build.rs` embeds `web/build/`
- **Docker**: Multi-stage, final stage is `scratch`; installs ffmpeg, python3, yt-dlp, yt-dlp-ejs
- **Cross-compile targets**: `x86_64-unknown-linux-musl`, `aarch64-unknown-linux-musl`, `armv7-unknown-linux-musleabihf`
- **Release profile**: `strip = true`, `lto = "thin"`, `codegen-units = 1`; `debug = false` for all deps in dev
- **DB migrations**: SeaORM, auto-applied on startup; new migrations in `bili_sync_migration/src/m202*.rs`
- **Config in DB since v2.6.0** — no config file, everything via WebUI
- **Built crate** captures git/version info at compile time (`build.rs`)

## Testing

- `cargo test` runs all crate tests
- No separate integration test suite visible; backend tests run in CI
- Frontend: `bun run check` (svelte-check), `bun run lint` (prettier + eslint)

## Environment / Runtime

- **Env vars** (Docker): `LANG=zh_CN.UTF-8`, `TZ=Asia/Shanghai`, `BILI_SYNC_IN_CONTAINER=1`, `RUST_LOG=none,bili_sync=info`
- **Volumes**: `/app/.config/bili-sync`, `/app/youtube_helper`, `/download`
- **Entry**: `/app/bili-sync-rs`
- **Ports**: WebUI on configured bind address (default `0.0.0.0:19797`)

## Common Tasks

- **Add migration**: Create `crates/bili_sync_migration/src/m2024MMDD_XXXXXX_name.rs`, register in `lib.rs`
- **Update version**: Edit `Cargo.toml` `[workspace.package].version`, update `web/package.json` and `docs/introduction.md` (release workflow does this)
- **New frontend dep**: `cd web && bun add <pkg>`
- **New backend dep**: Add to `[workspace.dependencies]` in root `Cargo.toml`, then to crate's `Cargo.toml`