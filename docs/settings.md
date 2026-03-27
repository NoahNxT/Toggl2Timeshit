# Settings

Open Settings with `s`.

Settings are organized into categories (left column) and items (right column).

## Navigation
**Categories (left)**
- `Up/Down`: Select category
- `Enter`: Open items
- `Esc`: Close settings

**Items (right)**
- `Up/Down`: Select item
- `Enter`: Edit item
- `Esc`: Back to categories

**Editing**
- Numeric/text fields: type, `Backspace` delete, `Enter` save, `Esc` cancel
- Toggle/preset fields: `Up/Down` change, `Enter` save, `Esc` cancel

## General
- **Theme**  
  Built-in presets include `Terminal`, `Midnight`, `Snow`, `Tokyo Night`, `Dracula`, `Catppuccin`, and `Cyberpunk`.
- **Target hours**  
  Used to color the footer total (green if met, red if below).
- **Time rounding** (Off/On)  
  Disabled by default. When disabled, rounding settings are removed from config.
- **Rounding increment**  
  `0.25h`, `0.50h`, `0.75h`, `1.00h` (requires time rounding enabled)
- **Rounding mode**  
  `closest`, `up`, `down` (requires time rounding enabled)

### How rounding is applied
Rounding is applied to each **grouped entry line** (per description). Project totals and overall totals are computed as the **sum of rounded entry lines**.

## Rollups
- **Include weekends**  
  Whether weekends are part of rollup targeting and navigation.
- **Week start**  
  Monday or Sunday for weekly grouping/calendar.
- **Credit vacation as worked**  
  If enabled, each vacation day contributes its configured vacation-day hours to worked totals.
- **Credit sick as worked**  
  If enabled, each sick day contributes its configured sick-day hours to worked totals.
- **Vacation target hours**  
  Target hours counted for days marked as vacation (`k` shortcut).
- **Vacation credit hours**  
  Worked hours credited when “Credit vacation as worked” is enabled.
- **Sick target hours**  
  Target hours counted for days marked as sick (`j` shortcut).
- **Sick credit hours**  
  Worked hours credited when “Credit sick as worked” is enabled.

## Integrations
- **Toggl token**  
  Update the API token from inside the app. The token is stored at:
  ```
  ~/.toggl2tsc
  ```

### Config File
Settings are stored in:
```
~/.toggl2tsc.json
```
