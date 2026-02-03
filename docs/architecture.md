# Architecture

## High-level Flow
1. Load token → cache → quota
2. Resolve workspace, projects, clients, entries (cache-first)
3. Group entries by project + client
4. Render dashboard

## Modules
- **app.rs**: App state, cache/quota logic, key handling, settings
- **ui.rs**: Layout, panels, modals, themes
- **toggl.rs**: API client + error mapping
- **storage.rs**: Token, cache, quota, config
- **grouping.rs**: Aggregation + sorting
- **dates.rs**: Date parsing/range helpers

## Cache Strategy
Cache records are keyed by:
```
token_hash + workspace_id + date_range
```

Manual refresh (`r`) attempts API calls; otherwise cache is used whenever available.
