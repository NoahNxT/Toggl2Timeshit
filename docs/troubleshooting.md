# Troubleshooting

## “Toggl API error: 402 Payment Required”
- This indicates a free-tier quota limit or billing issue.
- Use cached data and refresh only when needed.
- Check [Caching & Quotas](caching.md).

## “Invalid token. Please login.”
- Update your token via Settings → Integrations.

## No data shown
- Ensure you selected the correct workspace.
- Confirm your date range has entries.
- Try a manual refresh (`r`) if quota allows.

## Clipboard not working
- Some terminals restrict clipboard access.
- Ensure you run the app in a desktop environment with clipboard support.

## CI: CodeQL failing
- If CodeQL Default Setup is enabled, disable it when using advanced workflows.
- Keep Rust-only analysis to avoid JS/TS errors in a Rust repo.
