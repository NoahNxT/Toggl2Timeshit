# Caching & Quotas

Timeshit is built to work well with Toggl Track free-tier limits.

## Key Behavior
- **No automatic API calls** on startup if cached data is available.
- **Manual refresh only** (`r`) attempts API calls for time entries.
- Workspace/project/client metadata is **cache-first** and only fetched if missing.
- If quota is exhausted, the app uses cached data and shows a warning.

## Cache Files
```
~/.toggl2tsc-cache.json
~/.toggl2tsc-quota.json
```

### Cache scope
- Token hash (per user)
- Workspace ID
- Date range (start/end)

### What is cached
- Workspaces
- Projects
- Clients
- Time entries

## Quota Behavior
The app tracks a **local daily call budget** and resets it at local midnight.  
This budget is applied to **time entry fetches** (the endpoint that typically hits Toggl’s free-tier quota).  
If the Toggl API returns 402/429/5xx, cached data is used instead.
