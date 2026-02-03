# Development

## Requirements
- Rust (stable)

## Build & Run
```bash
cargo run
```

## Tests
```bash
cargo test
```

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
