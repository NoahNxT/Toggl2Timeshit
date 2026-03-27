# Development

## Requirements
- Rust (stable)
- Bun

## Build & Run
```bash
cd theme-studio
bun install --frozen-lockfile
bun run build
cd ..
cargo run
```

## Tests
```bash
bunx --cwd theme-studio @biomejs/biome check .
cargo test
```

`theme-studio/dist` is generated locally and in CI. It is not committed.

## Project Structure
- `src/app.rs`: state, cache, settings, key handling
- `src/ui.rs`: TUI rendering and modals
- `src/toggl.rs`: API client
- `src/storage.rs`: token, cache, quota, config
- `src/grouping.rs`: grouping and summaries

## Local Config
```
~/.toggl2tsc.json
```
Contains theme and target hours.
