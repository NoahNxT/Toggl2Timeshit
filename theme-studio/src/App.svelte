<script lang="ts">
import { onMount } from "svelte";

type ThemePreference =
  | "terminal"
  | "dark"
  | "light"
  | "tokyonight"
  | "dracula"
  | "catppuccin"
  | "cyberpunk";

type ThemePalette = {
  panel: string;
  border: string;
  text: string;
  muted: string;
  accent: string;
  highlight: string;
  success: string;
  error: string;
};

type ThemeSelection = { kind: "builtin"; theme: ThemePreference } | { kind: "custom"; id: string };

type BuiltinTheme = {
  theme: ThemePreference;
  label: string;
  palette: ThemePalette;
};

type CustomTheme = {
  id: string;
  name: string;
  palette: ThemePalette;
  created_at: string;
  updated_at: string;
};

type StateResponse = {
  builtins: BuiltinTheme[];
  customThemes: CustomTheme[];
  activeTheme: ThemeSelection;
};

type SaveResponse = {
  state: StateResponse;
  savedThemeId: string;
};

type ActivateResponse = {
  state: StateResponse;
};

type EditableTheme = {
  mode: "builtin" | "custom";
  persistedId?: string;
  name: string;
  palette: ThemePalette;
  sourceSelection?: ThemeSelection;
  isNew: boolean;
};

const paletteFields: Array<{ key: keyof ThemePalette; label: string }> = [
  { key: "panel", label: "Panel" },
  { key: "border", label: "Border" },
  { key: "text", label: "Text" },
  { key: "muted", label: "Muted" },
  { key: "accent", label: "Accent" },
  { key: "highlight", label: "Highlight" },
  { key: "success", label: "Success" },
  { key: "error", label: "Error" },
];

let builtins: BuiltinTheme[] = [];
let customThemes: CustomTheme[] = [];
let activeTheme: ThemeSelection | null = null;
let selectedSelection: ThemeSelection | null = null;
let editor: EditableTheme | null = null;
let loading = true;
let busy = false;
let errorMessage = "";
let noticeMessage = "";
let finished = false;

onMount(async () => {
  await loadState();
});

async function loadState(): Promise<void> {
  loading = true;
  errorMessage = "";
  try {
    const state = await requestJson<StateResponse>("/api/state");
    applyState(state, state.activeTheme);
  } catch (error) {
    errorMessage = messageFrom(error);
  } finally {
    loading = false;
  }
}

function applyState(state: StateResponse, nextSelection: ThemeSelection | null = null): void {
  builtins = state.builtins;
  customThemes = state.customThemes;
  activeTheme = state.activeTheme;

  const resolvedSelection =
    nextSelection && selectionExists(nextSelection) ? nextSelection : state.activeTheme;
  selectedSelection = resolvedSelection;
  editor = editorFromSelection(resolvedSelection);
}

function selectionExists(selection: ThemeSelection): boolean {
  if (selection.kind === "builtin") {
    return builtins.some((theme) => theme.theme === selection.theme);
  }
  return customThemes.some((theme) => theme.id === selection.id);
}

function editorFromSelection(selection: ThemeSelection): EditableTheme {
  if (selection.kind === "builtin") {
    const builtin = builtins.find((theme) => theme.theme === selection.theme);
    if (!builtin) {
      throw new Error("Built-in theme not found.");
    }
    return {
      mode: "builtin",
      name: builtin.label,
      palette: clonePalette(builtin.palette),
      sourceSelection: selection,
      isNew: false,
    };
  }

  const custom = customThemes.find((theme) => theme.id === selection.id);
  if (!custom) {
    throw new Error("Custom theme not found.");
  }
  return {
    mode: "custom",
    persistedId: custom.id,
    name: custom.name,
    palette: clonePalette(custom.palette),
    sourceSelection: selection,
    isNew: false,
  };
}

function clonePalette(palette: ThemePalette): ThemePalette {
  return { ...palette };
}

function selectionsEqual(left: ThemeSelection | null, right: ThemeSelection | null): boolean {
  if (!left || !right || left.kind !== right.kind) {
    return false;
  }
  if (left.kind === "builtin" && right.kind === "builtin") {
    return left.theme === right.theme;
  }
  return left.id === (right as Extract<ThemeSelection, { kind: "custom" }>).id;
}

function selectionLabel(selection: ThemeSelection | null): string {
  if (!selection) {
    return "Unsaved custom theme";
  }
  if (selection.kind === "builtin") {
    return builtins.find((theme) => theme.theme === selection.theme)?.label ?? "Built-in theme";
  }
  return customThemes.find((theme) => theme.id === selection.id)?.name ?? "Custom theme";
}

function isActive(selection: ThemeSelection): boolean {
  return selectionsEqual(selection, activeTheme);
}

function isSelected(selection: ThemeSelection): boolean {
  return selectionsEqual(selection, selectedSelection);
}

function isDirty(): boolean {
  if (!editor) {
    return false;
  }
  if (editor.mode === "builtin") {
    return false;
  }
  if (!editor.persistedId) {
    return true;
  }
  const current = customThemes.find((theme) => theme.id === editor.persistedId);
  if (!current) {
    return true;
  }
  return (
    current.name !== editor.name ||
    JSON.stringify(current.palette) !== JSON.stringify(editor.palette)
  );
}

function confirmDiscard(): boolean {
  if (!isDirty()) {
    return true;
  }
  return window.confirm("Discard unsaved theme changes?");
}

function chooseSelection(selection: ThemeSelection): void {
  if (!confirmDiscard()) {
    return;
  }
  selectedSelection = selection;
  editor = editorFromSelection(selection);
  errorMessage = "";
  noticeMessage = "";
}

function createCustomFromCurrent(): void {
  if (!confirmDiscard() || !editor) {
    return;
  }
  const baseLabel = editor.mode === "builtin" ? editor.name : editor.name;
  editor = {
    mode: "custom",
    name: `${baseLabel} Copy`,
    palette: clonePalette(editor.palette),
    isNew: true,
  };
  selectedSelection = null;
  errorMessage = "";
  noticeMessage = "New custom theme draft created.";
}

function updateThemeName(value: string): void {
  if (!editor || editor.mode !== "custom") {
    return;
  }
  editor = { ...editor, name: value };
}

function updatePaletteField(key: keyof ThemePalette, value: string): void {
  if (!editor) {
    return;
  }
  editor = {
    ...editor,
    palette: {
      ...editor.palette,
      [key]: value,
    },
  };
}

async function saveTheme(showNotice = true): Promise<string | null> {
  if (!editor || editor.mode !== "custom") {
    return null;
  }
  busy = true;
  errorMessage = "";
  try {
    const response = await requestJson<SaveResponse>("/api/themes", {
      method: "POST",
      body: JSON.stringify({
        id: editor.persistedId,
        name: editor.name,
        palette: editor.palette,
      }),
    });
    applyState(response.state, { kind: "custom", id: response.savedThemeId });
    if (showNotice) {
      noticeMessage = "Theme saved.";
    }
    return response.savedThemeId;
  } catch (error) {
    errorMessage = messageFrom(error);
    return null;
  } finally {
    busy = false;
  }
}

async function deleteTheme(): Promise<void> {
  if (!editor?.persistedId) {
    return;
  }
  if (!window.confirm(`Delete "${editor.name}"?`)) {
    return;
  }
  busy = true;
  errorMessage = "";
  try {
    const response = await requestJson<ActivateResponse>(`/api/themes/${editor.persistedId}`, {
      method: "DELETE",
    });
    applyState(response.state, response.state.activeTheme);
    noticeMessage = "Theme deleted.";
  } catch (error) {
    errorMessage = messageFrom(error);
  } finally {
    busy = false;
  }
}

async function activateSelection(selection: ThemeSelection): Promise<void> {
  busy = true;
  errorMessage = "";
  try {
    const response = await requestJson<ActivateResponse>("/api/activate", {
      method: "POST",
      body: JSON.stringify(selection),
    });
    applyState(response.state, selection);
    noticeMessage = `Applied ${selectionLabel(selection)}.`;
  } catch (error) {
    errorMessage = messageFrom(error);
    throw error;
  } finally {
    busy = false;
  }
}

async function saveAndApply(): Promise<void> {
  if (!editor) {
    return;
  }

  let selection = selectedSelection;
  if (editor.mode === "custom") {
    const savedThemeId = await saveTheme(false);
    if (!savedThemeId) {
      return;
    }
    selection = { kind: "custom", id: savedThemeId };
  }

  if (!selection && editor.mode === "builtin" && editor.sourceSelection) {
    selection = editor.sourceSelection;
  }

  if (!selection) {
    errorMessage = "Select a theme to apply.";
    return;
  }

  try {
    await activateSelection(selection);
    await finishSession();
  } catch {
    return;
  }
}

async function cancelAndClose(): Promise<void> {
  if (!confirmDiscard()) {
    return;
  }
  await finishSession();
}

async function finishSession(): Promise<void> {
  busy = true;
  try {
    await requestJson("/api/finish", { method: "POST", body: "{}" });
    finished = true;
    noticeMessage = "Theme studio closed. You can return to Timeshit.";
  } catch (error) {
    errorMessage = messageFrom(error);
  } finally {
    busy = false;
  }
}

async function requestJson<T>(url: string, init: RequestInit = {}): Promise<T> {
  const response = await fetch(url, {
    headers: {
      "Content-Type": "application/json",
      ...(init.headers ?? {}),
    },
    ...init,
  });

  if (!response.ok) {
    const message = await readError(response);
    throw new Error(message);
  }

  if (response.status === 204) {
    return undefined as T;
  }

  return (await response.json()) as T;
}

async function readError(response: Response): Promise<string> {
  try {
    const payload = (await response.json()) as { error?: string };
    return payload.error ?? `Request failed with ${response.status}.`;
  } catch {
    return `Request failed with ${response.status}.`;
  }
}

function messageFrom(error: unknown): string {
  return error instanceof Error ? error.message : "Something went wrong.";
}

$: previewPalette = editor?.palette;
</script>

<svelte:head>
  <title>Timeshit Theme Studio</title>
</svelte:head>

{#if loading}
  <main class="shell">
    <section class="loading-card">
      <p>Loading your theme library...</p>
    </section>
  </main>
{:else}
  <main class="shell">
    <section class="hero">
      <div>
        <p class="eyebrow">Timeshit Theme Studio</p>
        <h1>Craft, preview, and apply terminal themes without touching JSON.</h1>
      </div>
      <div class="hero-actions">
        <button class="secondary" on:click={createCustomFromCurrent} disabled={busy || finished}>
          Duplicate Current
        </button>
        <button class="primary" on:click={saveAndApply} disabled={busy || finished}>
          Save &amp; Apply
        </button>
      </div>
    </section>

    {#if errorMessage}
      <div class="message error">{errorMessage}</div>
    {/if}
    {#if noticeMessage}
      <div class="message notice">{noticeMessage}</div>
    {/if}

    <section class="workspace">
      <aside class="library">
        <div class="panel-header">
          <div>
            <p class="panel-kicker">Library</p>
            <h2>Built-ins</h2>
          </div>
        </div>

        <div class="theme-list">
          {#each builtins as builtin}
            <button
              class:selected={isSelected({ kind: "builtin", theme: builtin.theme })}
              class:active={isActive({ kind: "builtin", theme: builtin.theme })}
              class="theme-tile"
              on:click={() => chooseSelection({ kind: "builtin", theme: builtin.theme })}
              disabled={busy || finished}
            >
              <span>{builtin.label}</span>
              {#if isActive({ kind: "builtin", theme: builtin.theme })}
                <strong>Active</strong>
              {/if}
            </button>
          {/each}
        </div>

        <div class="panel-header custom-header">
          <div>
            <p class="panel-kicker">Library</p>
            <h2>Custom Themes</h2>
          </div>
          <button class="ghost" on:click={createCustomFromCurrent} disabled={busy || finished}>
            New
          </button>
        </div>

        <div class="theme-list custom-list">
          {#if customThemes.length === 0}
            <p class="empty-state">No saved custom themes yet.</p>
          {:else}
            {#each customThemes as custom}
              <button
                class:selected={isSelected({ kind: "custom", id: custom.id })}
                class:active={isActive({ kind: "custom", id: custom.id })}
                class="theme-tile"
                on:click={() => chooseSelection({ kind: "custom", id: custom.id })}
                disabled={busy || finished}
              >
                <span>{custom.name}</span>
                {#if isActive({ kind: "custom", id: custom.id })}
                  <strong>Active</strong>
                {/if}
              </button>
            {/each}
          {/if}
        </div>
      </aside>

      <section class="editor">
        <div class="panel-header">
          <div>
            <p class="panel-kicker">Editor</p>
            <h2>{editor?.mode === "custom" ? editor.name || "New Custom Theme" : selectionLabel(selectedSelection)}</h2>
          </div>
          <div class="editor-actions">
            <button class="secondary" on:click={cancelAndClose} disabled={busy}>
              Cancel
            </button>
            <button
              class="secondary"
              on:click={() => saveTheme(true)}
              disabled={busy || finished || !editor || editor.mode !== "custom"}
            >
              Save
            </button>
            <button
              class="danger"
              on:click={deleteTheme}
              disabled={busy || finished || !editor?.persistedId}
            >
              Delete
            </button>
          </div>
        </div>

        {#if editor}
          <div class="editor-grid">
            <div class="form-card">
              <label class="field">
                <span>Name</span>
                <input
                  type="text"
                  value={editor.name}
                  on:input={(event) => updateThemeName((event.currentTarget as HTMLInputElement).value)}
                  disabled={busy || finished || editor.mode !== "custom"}
                  maxlength="48"
                  placeholder="Aurora"
                />
              </label>

              <div class="palette-grid">
                {#each paletteFields as field}
                  <label class="field palette-field">
                    <span>{field.label}</span>
                    <div class="color-row">
                      <input
                        type="color"
                        value={editor.palette[field.key]}
                        on:input={(event) =>
                          updatePaletteField(
                            field.key,
                            (event.currentTarget as HTMLInputElement).value
                          )}
                        disabled={busy || finished}
                      />
                      <input
                        type="text"
                        value={editor.palette[field.key]}
                        on:input={(event) =>
                          updatePaletteField(
                            field.key,
                            (event.currentTarget as HTMLInputElement).value
                          )}
                        disabled={busy || finished}
                        placeholder="#112233"
                      />
                    </div>
                  </label>
                {/each}
              </div>
            </div>

            <div class="preview-card">
              <div class="preview-shell" style={`--panel:${previewPalette?.panel};--border:${previewPalette?.border};--text:${previewPalette?.text};--muted:${previewPalette?.muted};--accent:${previewPalette?.accent};--highlight:${previewPalette?.highlight};--success:${previewPalette?.success};--error:${previewPalette?.error};`}>
                <div class="preview-topbar">
                  <span class="dot red"></span>
                  <span class="dot amber"></span>
                  <span class="dot green"></span>
                  <p>{editor.name || "Untitled Theme"}</p>
                </div>
                <div class="preview-header">
                  <strong>Timeshit v1.9.1</strong>
                  <span>Workspace: noah's workspace</span>
                  <span class="accent">Day: Sick 7.60h credit / 8.00h target</span>
                </div>
                <div class="preview-body">
                  <section>
                    <header>Projects</header>
                    <div class="preview-list selected">Intern - CAIOPS <span>2.00h</span></div>
                    <div class="preview-list">Odisee - Link <span>2.00h</span></div>
                  </section>
                  <section>
                    <header>Entries</header>
                    <div class="preview-entry">
                      Hosting setup agent + knowledge registry <span>2.00h</span>
                    </div>
                  </section>
                </div>
                <div class="preview-footer">
                  <span class="error-text">Total 4.00h</span>
                  <span>h help</span>
                  <span>s settings</span>
                  <span>g studio</span>
                </div>
              </div>
            </div>
          </div>
        {/if}

        {#if finished}
          <p class="finish-note">
            The local studio session has ended. This tab can stay open, but Timeshit has already resumed.
          </p>
        {/if}
      </section>
    </section>
  </main>
{/if}

<style>
  .shell {
    padding: 32px;
    color: #eff6ff;
  }

  .hero,
  .workspace,
  .loading-card,
  .message {
    width: min(1440px, calc(100vw - 64px));
    margin: 0 auto;
  }

  .hero {
    display: flex;
    justify-content: space-between;
    gap: 24px;
    align-items: end;
    margin-bottom: 20px;
  }

  .hero h1 {
    margin: 8px 0 0;
    max-width: 820px;
    font-size: clamp(2rem, 4vw, 3.8rem);
    line-height: 1;
    letter-spacing: -0.05em;
  }

  .eyebrow,
  .panel-kicker {
    margin: 0;
    text-transform: uppercase;
    letter-spacing: 0.18em;
    font-size: 0.72rem;
    color: #8aa3c7;
  }

  .hero-actions,
  .editor-actions {
    display: flex;
    gap: 12px;
    flex-wrap: wrap;
  }

  .message {
    margin-bottom: 16px;
    padding: 14px 16px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    border-radius: 16px;
    backdrop-filter: blur(18px);
  }

  .message.error {
    background: rgba(255, 96, 141, 0.12);
    border-color: rgba(255, 96, 141, 0.4);
  }

  .message.notice {
    background: rgba(0, 245, 255, 0.08);
    border-color: rgba(0, 245, 255, 0.32);
  }

  .workspace {
    display: grid;
    grid-template-columns: 320px minmax(0, 1fr);
    gap: 20px;
  }

  .library,
  .editor,
  .loading-card {
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(8, 16, 30, 0.68);
    border-radius: 24px;
    backdrop-filter: blur(24px);
    box-shadow: 0 20px 60px rgba(0, 0, 0, 0.28);
  }

  .loading-card {
    padding: 40px;
    text-align: center;
  }

  .library,
  .editor {
    padding: 20px;
  }

  .panel-header {
    display: flex;
    justify-content: space-between;
    gap: 16px;
    align-items: center;
    margin-bottom: 14px;
  }

  .panel-header h2 {
    margin: 4px 0 0;
    font-size: 1.2rem;
  }

  .custom-header {
    margin-top: 20px;
  }

  .theme-list {
    display: grid;
    gap: 10px;
  }

  .theme-tile {
    display: flex;
    justify-content: space-between;
    align-items: center;
    gap: 12px;
    padding: 14px 16px;
    border-radius: 16px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.02);
    color: inherit;
    cursor: pointer;
    text-align: left;
    transition:
      transform 120ms ease,
      border-color 120ms ease,
      background 120ms ease;
  }

  .theme-tile:hover:enabled,
  .theme-tile.selected {
    transform: translateY(-1px);
    border-color: rgba(0, 245, 255, 0.45);
    background: rgba(0, 245, 255, 0.08);
  }

  .theme-tile.active strong {
    color: #00f5ff;
  }

  .empty-state,
  .finish-note {
    color: #8aa3c7;
  }

  .editor-grid {
    display: grid;
    grid-template-columns: minmax(0, 1.1fr) minmax(320px, 0.9fr);
    gap: 18px;
  }

  .form-card,
  .preview-card {
    border-radius: 20px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(255, 255, 255, 0.02);
    padding: 18px;
  }

  .field {
    display: grid;
    gap: 8px;
    margin-bottom: 14px;
  }

  .field span {
    font-size: 0.86rem;
    color: #8aa3c7;
  }

  .field input[type="text"] {
    border: 1px solid rgba(255, 255, 255, 0.1);
    background: rgba(6, 10, 18, 0.78);
    color: inherit;
    padding: 12px 14px;
    border-radius: 14px;
  }

  .palette-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 12px;
  }

  .palette-field {
    margin-bottom: 0;
  }

  .color-row {
    display: grid;
    grid-template-columns: 52px minmax(0, 1fr);
    gap: 10px;
  }

  .color-row input[type="color"] {
    width: 52px;
    height: 48px;
    border-radius: 12px;
    border: 1px solid rgba(255, 255, 255, 0.08);
    background: rgba(6, 10, 18, 0.78);
    padding: 4px;
  }

  .preview-shell {
    --panel: #101924;
    --border: #5f6b7a;
    --text: #f3f4f6;
    --muted: #9aa4b2;
    --accent: #4f8cff;
    --highlight: #ffd166;
    --success: #52d273;
    --error: #ff6b6b;
    min-height: 100%;
    border-radius: 18px;
    border: 1px solid color-mix(in srgb, var(--border) 80%, transparent);
    background: var(--panel);
    color: var(--text);
    overflow: hidden;
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.04);
  }

  .preview-topbar,
  .preview-header,
  .preview-footer {
    padding: 12px 14px;
    border-bottom: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
  }

  .preview-topbar {
    display: flex;
    align-items: center;
    gap: 8px;
    background: color-mix(in srgb, var(--border) 20%, var(--panel));
  }

  .preview-topbar p {
    margin: 0 0 0 6px;
    color: var(--muted);
    font-size: 0.86rem;
  }

  .dot {
    width: 11px;
    height: 11px;
    border-radius: 999px;
    display: inline-block;
  }

  .dot.red {
    background: #ff5f57;
  }

  .dot.amber {
    background: #febc2e;
  }

  .dot.green {
    background: #28c840;
  }

  .preview-header {
    display: flex;
    flex-wrap: wrap;
    gap: 16px;
    font-size: 0.86rem;
  }

  .preview-header strong {
    color: var(--accent);
  }

  .preview-header span {
    color: var(--muted);
  }

  .preview-header .accent {
    color: var(--highlight);
    font-weight: 600;
  }

  .preview-body {
    display: grid;
    grid-template-columns: 0.8fr 1.2fr;
    gap: 12px;
    padding: 14px;
  }

  .preview-body section {
    border: 1px solid color-mix(in srgb, var(--border) 75%, transparent);
    border-radius: 16px;
    min-height: 240px;
    padding: 12px;
  }

  .preview-body header {
    font-weight: 700;
    color: var(--accent);
    margin-bottom: 10px;
  }

  .preview-list,
  .preview-entry {
    display: flex;
    justify-content: space-between;
    gap: 12px;
    padding: 8px 10px;
    border-radius: 10px;
  }

  .preview-list span,
  .preview-entry span {
    color: var(--muted);
  }

  .preview-list.selected {
    background: var(--accent);
    color: #09111d;
    font-weight: 700;
  }

  .preview-list.selected span {
    color: #09111d;
  }

  .preview-entry {
    color: var(--highlight);
    font-weight: 600;
  }

  .preview-footer {
    display: flex;
    gap: 14px;
    border-top: 1px solid color-mix(in srgb, var(--border) 70%, transparent);
    border-bottom: 0;
    color: var(--muted);
    font-size: 0.84rem;
  }

  .error-text {
    color: var(--error);
    font-weight: 700;
  }

  button {
    border: 0;
    border-radius: 14px;
    padding: 11px 16px;
    cursor: pointer;
    transition:
      transform 120ms ease,
      opacity 120ms ease,
      background 120ms ease;
  }

  button:hover:enabled {
    transform: translateY(-1px);
  }

  button:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  .primary {
    background: linear-gradient(135deg, #00f5ff, #66a3ff);
    color: #09111d;
    font-weight: 700;
  }

  .secondary {
    background: rgba(255, 255, 255, 0.08);
    color: inherit;
  }

  .ghost {
    background: transparent;
    border: 1px solid rgba(255, 255, 255, 0.12);
    color: inherit;
  }

  .danger {
    background: rgba(255, 96, 141, 0.12);
    color: #ff9bb8;
  }

  @media (max-width: 1120px) {
    .workspace,
    .editor-grid,
    .hero {
      grid-template-columns: 1fr;
      display: grid;
    }

    .hero-actions {
      justify-content: start;
    }
  }

  @media (max-width: 720px) {
    .shell {
      padding: 18px;
    }

    .hero,
    .workspace,
    .loading-card,
    .message {
      width: min(100vw - 36px, 100%);
    }

    .palette-grid,
    .preview-body {
      grid-template-columns: 1fr;
    }
  }
</style>
