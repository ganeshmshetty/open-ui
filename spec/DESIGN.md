# Design Language

Status: **draft 0.2**. Companion to `SPEC.md`. It defines how the phase-1 standard library (SPEC §8.8) looks and behaves on screen. The GPUI renderer MUST implement it, and later renderers map it to their own medium (§6.1 covers terminals).

Sources: three observation reports in `research/design/`:

- `chatgpt-design.md`: ChatGPT's generative-UI widgets, observed in dark mode.
- `openui-design.md`: openui's public site and demos, rendered pages only.
- `baselines.md`: gpui-kit 0.8 theme tokens, Radix, Apple HIG, Geist and Okabe-Ito.
- `constrained-vocab.md`: Slack Block Kit, Microsoft Adaptive Cards and Raycast, which render third-party UI from a fixed vocabulary.
- `data-dense.md`: Tremor, IBM Carbon, Stripe Apps and Apple chart guidance, for tables, stat cards and charts.

Every value below is our own choice. Most are taken from gpui-kit's defaults or from public design-system scales, and §11 gives the reasoning. No product's visual identity is copied: we use neither ChatGPT's black canvas nor openui's teal accent.

---

## 1. Principles

1. **The model owns the content; the host owns the look.** (This is also Adaptive Cards' core rule.) The model can say `tone: "danger"` or `gap: 3`. It can never pick a color, a pixel value or a font. Every program therefore looks consistent and well made, whatever the model writes.
2. **Calm and dense, like a good dashboard.** The content is the interface. There is no decoration, no gradient and no illustration. Default spacing is compact but never cramped.
3. **Hierarchy through size and weight, not color.** We use three weights (400, 500, 600) and a short type scale. Muted text handles secondary information.
4. **Color carries meaning, or it isn't there.** Neutral surfaces and text, one accent for interaction (focus, links, selection), and five status tones. Charts are the only place with many colors.
5. **Hairlines, not shadows.** Containers have a 1 px border on a flat surface. Shadows appear only on overlays (select menus, tooltips).
6. **Good by default.** Layout rules (§6) give the right spacing even when the model passes no `gap` or `padding`. The model should rarely need spacing arguments.
7. **Streaming feels steady.** New content appears smoothly, nothing jumps or flickers, and partial content is clearly not yet interactive (§8).
8. **Native and themeable.** The theme sits on gpui-kit's semantic tokens and follows the host's light or dark appearance. Hosts can re-skin colors, fonts and radius without code changes (§10).

---

## 2. Spacing

A 4 px base. The model's `space` token is a step from 0 to 8.

| Step | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| px | 0 | 4 | 8 | 12 | 16 | 20 | 24 | 28 | 32 |

- Defaults favour steps 2–4. Steps 6–8 are for separating major sections.
- Internal component padding (inside a button or input) is fixed per component (§6). It is not a model-facing token.
- The renderer MAY add a 2 px half-step internally, for example between a label and its control, but never expose it to the model.

---

## 3. Typography

**Fonts.**

- UI text uses the platform system font: SF Pro on macOS, the system UI font on Linux and Windows. This is the font gpui-kit's theme defaults to.
- Monospace uses the theme's mono font for `Code`, `mono: true` and numbers in tables.
- Hosts may override either font (§10).

**Weights.** Only 400 (regular), 500 (medium) and 600 (semibold).

| Role | Size / line height | Weight | Used by |
| --- | --- | --- | --- |
| `caption` | 12 / 16 | 400 | helper and error text, table headers, chart axes, badge text |
| `small` | 13 / 18 | 400 | `Text(size: "sm")`, field labels (500), table cells in compact density |
| `body` | 14 / 20 | 400 | default `Text`, controls, table cells, list items |
| `body_lg` | 16 / 24 | 400 | `Text(size: "lg")`, prose markdown paragraphs |
| `title` | 20 / 28 | 600 | `Heading(level: 2)` |
| `heading` | 24 / 32 | 600 | `Heading(level: 1)` |
| `mono` | 13 / 20 | 400 | `Code`, `mono: true` |

| Heading level | Role |
| --- | --- |
| 1 | `heading` (24/32, 600) |
| 2 | `title` (20/28, 600) |
| 3 | 16/24, 600 |
| 4 | 14/20, 600 |

Rules:

- `bold: true` on `Text` means weight 600.
- Numbers in tables and chart labels use tabular figures. Numeric table columns are right-aligned.
- Prose markdown segments around the program use `body_lg` for paragraphs, so chat text stays readable. Program UI uses `body` (14) and is denser.
- Text never goes below 12 px.

---

## 4. Color

### 4.1 Roles

The theme is built on gpui-base `SemanticThemeTokens` (gpui-kit 0.8), not on the legacy `ThemeColor` struct. Our role names are those tokens, plus the tones and the chart palette.

Default values (neutral, from gpui-base defaults, hex rounded):

| Role | Light | Dark | Use |
| --- | --- | --- | --- |
| `background` | `#ffffff` | `#0a0a0a` | message canvas |
| `surface` | `#ffffff` | `#0a0a0a` | Card fill (same as background; the border does the work) |
| `surface_subtle` | `#fafafa` | `#141414` | table header, code block, chart plot area |
| `foreground` | `#0a0a0a` | `#fafafa` | primary text |
| `muted_foreground` | `#737373` | `#a3a3a3` | secondary text, labels, captions, axes |
| `border` | `#e5e5e5` | `#262626` | card, table, divider, input borders |
| `input` | `#e5e5e5` | `#2f2f2f` | input border at rest |
| `ring` | accent at 50% opacity | accent at 50% opacity | focus ring (2 px) |
| `primary` | `#171717` | `#fafafa` | primary button fill |
| `primary_foreground` | `#fafafa` | `#171717` | primary button text |
| `secondary` | `#f5f5f5` | `#262626` | secondary button fill, hover fill |
| `accent` | `#2563eb` | `#60a5fa` | focus, links, selected tab underline, checked controls, slider fill |
| `selection` | accent at 25% | accent at 30% | text selection |

`surface_subtle` and `accent` are our additions. gpui-base's own `accent` is a neutral hover color, which we map to `secondary`. The renderer's theme module owns this mapping (§10).

### 4.2 Tones

Each tone has a `fg` for text and icons, a `bg` for tinted fills (badge and alert backgrounds), and a `border`.

| Tone | Light fg / bg / border | Dark fg / bg / border |
| --- | --- | --- |
| `neutral` | `#404040` / `#f5f5f5` / `#e5e5e5` | `#d4d4d4` / `#1f1f1f` / `#2e2e2e` |
| `info` | `#1d4ed8` / `#eff6ff` / `#bfdbfe` | `#93c5fd` / `#0f1d33` / `#1e3a5f` |
| `success` | `#15803d` / `#f0fdf4` / `#bbf7d0` | `#86efac` / `#0d2416` / `#1a4d2e` |
| `warning` | `#b45309` / `#fffbeb` / `#fde68a` | `#fcd34d` / `#2b1f08` / `#5c4210` |
| `danger` | `#dc2626` / `#fef2f2` / `#fecaca` | `#fca5a5` / `#2d0f0f` / `#5c1d1d` |

All `fg`-on-`bg` and `fg`-on-`background` pairs MUST meet WCAG AA, 4.5:1 for text. The theme module's unit tests check contrast for every pair, in both modes.

### 4.3 Chart palette

The palette is categorical and colorblind-safe: Okabe-Ito order without black or yellow, re-tuned for dark backgrounds.

| Series | Light | Dark |
| --- | --- | --- |
| 1 | `#0072b2` | `#56b4e9` |
| 2 | `#e69f00` | `#f0b84a` |
| 3 | `#009e73` | `#3cc79a` |
| 4 | `#cc79a7` | `#e09cc4` |
| 5 | `#d55e00` | `#f08a4b` |
| 6 | `#56b4e9` | `#9ad0f0` |

Phase 1 charts are single-series and use series 1. The full palette is defined now so multi-series charts (Appendix A) need no redesign. gpui-kit's built-in `chart.1–5` is a single blue ramp and MUST NOT be used for categories.

---

## 5. Radius, borders, elevation, sizes

| Radius token | px | Use |
| --- | --- | --- |
| `none` | 0 | tables (outer radius comes from the container), dividers |
| `sm` | 4 | badges, checkboxes, chart bar tops, skeleton bars |
| `md` | 6 | buttons, inputs, selects, code blocks, alerts |
| `lg` | 8 | select menus, tooltips, images |
| `xl` | 12 | cards |
| `full` | 9999 | switch track, radio dot |

- **Borders** are 1 px in `border`, or `input` for controls at rest. There are no double borders: a Table inside a Card drops its own outer border.
- **Elevation.** No shadows on in-flow content. Overlays (select menu, tooltip) use one soft shadow: `0 4px 16px rgba(0,0,0,0.12)` in light, `rgba(0,0,0,0.5)` in dark, plus a 1 px `border`.
- **Control heights.** Small 24, **medium 32 (default)**, large 40. Checkboxes and radios are 16, and the switch is 32×18.
- **Table rows** are 32 by default, with 8 px horizontal cell padding and 1 px `border` dividers between rows. There is no zebra striping.

---

## 6. Layout rules (automatic spacing)

These rules apply whenever the model does not pass the argument. They are what make output look designed.

1. **Message frame.** The program renders inside the host's message column. Program content has a max width of 760 px and is left-aligned, with no outer padding (the host provides it). A program never scrolls horizontally, except inside a `Table` or `Code`.
2. **Column** defaults to `gap: 2` (8 px). Section rhythm comes from rule 3, not from a bigger gap.
3. **Headings make their own space.** A `Heading` that is not the first child of its container gets an extra 8 px above it, on top of the gap. So `Column([Heading, Text, Heading, Text])` reads as sections without any `gap` argument.
4. **Row** defaults to `gap: 2`, with children vertically centered. A Row of only `Button`s and `Badge`s is a toolbar: it wraps when the width runs out, even if `wrap: false`.
5. **Grid** defaults to `cols: 2` with `gap: 3` (12 px), and every cell is the same width.
   - Cells are at least 220 px wide. If the measured width cannot fit `cols` cells, the column count drops until it can, down to 1.
   - This needs the container width. The renderer measures it with a layout pass, and gpui-kit's `measure` helpers are the expected route; ROADMAP M8.
6. **Card**: 16 px padding (`padding: 4`), `xl` radius, 1 px border, `surface` fill.
   - The title row is `body` 14 with weight 600. The gap between the title and the content is 12 px.
   - Children stack with an 8 px gap.
   - A Card's first child that is a `Heading` replaces the title style. Don't do both.
7. **Form**
   - Fields stack with a 12 px gap.
   - Each field's label sits above its control (`small` 13/18, weight 500, `foreground`), with 6 px between them.
   - Helper or error text goes below the control (`caption`, with `danger.fg` for errors), 4 px down.
   - Submit buttons sit in a toolbar row after the last field, 16 px below it, left-aligned. A single primary button stays at its natural width, never full width.
8. **Text measure.** Plain `Text` wraps at its container width. Card and column widths already keep lines readable, so no extra measure rule is needed.
9. **Images** keep their aspect ratio by `fit`, with `lg` radius. Their height comes from `height`, default 160. In a `Row`, an image takes its natural width and the text beside it takes the rest.
10. **Tables**
    - The header row uses `caption` 12/16, weight 500, `muted_foreground`, on `surface_subtle`.
    - Cells use `body` 14.
    - `format: "number"` and `"percent"` columns are right-aligned with tabular figures.
    - An empty table shows its `empty` text centered in `muted_foreground`, in a single 64 px row.
    - Cell text stays on one line and is truncated with an ellipsis; the full value shows in a tooltip. Header text may wrap to 2 lines, then truncates.
    - Columns align by data type: text and dates start-aligned, every numeric format end-aligned.
    - More columns than fit scroll horizontally inside the table. The rest of the program never scrolls.
    - Only a sorted column shows a sort arrow (sorting itself is phase 2).
11. **Charts**
    - Default plot height is 240.
    - Axis labels use `caption` in `muted_foreground`.
    - Only horizontal gridlines, in `border`. No vertical gridlines and no chart border; the chart sits in its container.
    - Bars use `sm` top radius and series 1. Lines are 2 px, without point markers until hover.
    - The tooltip is the overlay style from §5.
    - `title` renders above the chart in `body` 14, weight 600.
    - **Bar charts always start the value axis at zero.** Line charts may start at a rounded value near the minimum.
    - The value axis has about 4 gridlines on round intervals (1, 2 or 5 × 10ⁿ). Tick labels use the column's number formatting (§6.12) and are never rotated; category labels that don't fit are truncated, and every second label is hidden before truncating.
    - Bars are sorted in the order the data gives them. The renderer never reorders.
    - `null` values leave a gap: no bar, and a break in the line. They are never drawn as zero or interpolated.
    - No legend for a single series. The `summary` text, if given, renders below the plot in `caption` `muted_foreground`.
    - More than 24 categories: the chart shows the first 24 and a `caption` line "Showing 24 of N".

12. **Number formatting** (Table `format`, chart ticks, tooltips):
    - `number`: thousands separators by locale, at most 2 decimal places, trailing zeros trimmed (`1,234.5`). Integers have no decimals.
    - `compact`: below 1,000 as `number`; above, scaled with K, M, B or T and at most 1 decimal, trimmed (`1.2K`, `3.4M`, `12B`).
    - `currency`: the locale's currency format for the `currency` code (ISO 4217, default `USD`), with that currency's minor units (`$1,234.50`, `¥1,235`). Values of 1,000,000 or more use compact form (`$1.2M`).
    - `percent`: the value is already a percentage (`12.3` → `12.3%`), at most 1 decimal place.
    - `delta`: like `number` but always signed (`+4.2`, `−1.3`, `0`). In a Table, positive is `success.fg` and negative `danger.fg`. In a Stat, the color follows `good`.
    - `date`: the value is an ISO 8601 string, shown in the locale's medium date format.
    - Negative numbers use a true minus sign (U+2212), never parentheses.
    - Formatting uses host locale. Until host formatters exist (SPEC 14.2 question 5), the renderer uses `en-US` rules.
13. **Same size in every state.** A component keeps its height across loading, empty and loaded states, so data arriving never shifts the layout. A chart's skeleton and its empty state are both its full plot height; a table's loading state is header plus 3 rows.
14. **Empty states.** Empty data is always explained, never a blank box: Table shows `empty`, Chart shows "No data" centered in its plot area, and an empty `List` renders nothing (the model is told to avoid empty lists).

### 6.1 Terminal mapping (for future renderers)

| px range | Vertical (lines) | Horizontal (columns) |
| --- | --- | --- |
| 0 | 0 | 0 |
| 4–8 | 0 | 1 |
| 12–20 | 1 | 2 |
| 24–32 | 1 | 3 |

- Cards and tables use box-drawing borders.
- Tones map to the nearest ANSI 16 color, or to truecolor when the terminal supports it.
- Weight 600 maps to bold, and `muted_foreground` maps to dim.

---

## 7. Component anatomy and states

Shared state styles:

| State | Treatment |
| --- | --- |
| hover | Fill shifts to `secondary` (interactive controls only). Cursor is a pointer. |
| focus | A 2 px `ring` outside the border, keyboard focus only. Never removed. |
| disabled | 50% opacity, no hover, `not-allowed` cursor. |
| partial (streaming) | See §8. Controls render but are disabled; text shows normally. |
| loading (query or ref pending) | See §8. |
| error (field) | Border in `danger.fg`, error text below in `caption` with `danger.fg`. |

| Component | Anatomy and sizes | Notes |
| --- | --- | --- |
| Heading | Text only, per §3 | No underline or divider. |
| Text | `body` (or `small`/`body_lg`). Tone changes color only: neutral is `foreground`, the others use `tone.fg`. | `mono` uses the mono font. |
| Markdown | `body_lg` paragraphs, 8 px between blocks. Headings mapped down: md `#` is level 3, `##` is level 4. Links in `accent`, underlined on hover. Inline code is `mono` on `surface_subtle` with `sm` radius. | No images (alt text and a link, per SPEC §10.12). |
| Code | `mono` on `surface_subtle`, `md` radius, 12 px padding, horizontal scroll. The language label and a copy button sit top-right in `caption`. | |
| Badge | `caption`, weight 500, `tone.fg` on `tone.bg`, 1 px `tone.border`, `sm` radius, 2×8 px padding, height 20. | |
| Alert | A 1 px `tone.border` box on `tone.bg` with `md` radius and 12 px padding. Optional title in `body` weight 600, text in `body`. An optional leading status icon in `tone.fg`. | No dismiss button in phase 1. |
| Divider | 1 px `border`, full width, 8 px above and below in addition to the gap. | |
| Form fields (all) | Errors appear after the field loses focus (touched) or after a failed submit, never while the user is still typing the first time. | |
| Image | `lg` radius. While pending, a `surface_subtle` box at the target size. On failure, the same box with the alt text centered in `caption` `muted_foreground`. | |
| Button | Medium 32 high, 12 px horizontal padding, `md` radius, `body` weight 500. Tones: **primary** is `primary` fill with `primary_foreground` text; **secondary** is a `secondary` fill; **ghost** is transparent and shows `secondary` on hover; **danger** is `danger.fg` fill with white text. While its action is gated (streaming or submitting), it shows as disabled. | One primary button per group is the norm; the prompt says so (S10). |
| Input / TextArea | 32 high (TextArea: `rows` × 20 + 12), 1 px `input` border, `md` radius, `body` text, `muted_foreground` placeholder. Label and errors follow the Form rules (§6.7) even outside a Form. | |
| Select | Looks like Input, with a chevron at the right. The menu is an overlay with `lg` radius and 32 px items, and the selected item shows an `accent` check. | |
| Checkbox | A 16 px box with `sm` radius and an `input` border. When checked: `accent` fill with a white check. The label is to the right in `body`, 8 px away. | |
| CheckboxGroup / RadioGroup | The group label uses the Form label style. Options stack vertically with an 8 px gap. They switch to a wrapping row when there are 4 or fewer options and every label is under 16 characters. | |
| Switch | A 32×18 `full`-radius track: `input` color when off, `accent` when on, with a white knob. Label on the right. | |
| Card | §6.6 | |
| List | Items are 8 px apart. Unordered items have a 4 px `muted_foreground` dot, and ordered items have a `muted_foreground` number, both in a 16 px gutter. Lists inside Cards get no extra border. | |
| Table | §5 and §6.10. Outer `xl` radius and border when it is not inside a Card. Rows virtualize above 100. | |
| Chart | §6.11 | |
| Stat | Label in `caption` `muted_foreground`; 4 px below it the value in 24/32 weight 600 `foreground`, tabular figures; the delta as a Badge to the right of the value (arrow ↑ or ↓, the formatted number, tone `success` when the direction matches `good`, `danger` when it doesn't, `neutral` at zero); optional `caption` line 4 px below in `muted_foreground`. No border of its own: put it in a Card or Grid. | One Stat per Card cell; a Grid of 2–4 Stats is the standard KPI row. |
| Facts | Rows of label and value: label in `small` `muted_foreground` (fixed 40% width), value in `body` `foreground`, 8 px between rows, no dividers. With `columns: 2`, two fact columns side by side, 24 px apart. | Values are truncated with a tooltip like table cells. |
| Tabs | A tab bar of `body` labels, weight 500, `muted_foreground`. The selected tab is `foreground` with a 2 px `accent` underline. There is a 1 px `border` under the bar, then 12 px before the panel. | |

---

### 7.1 Interaction rules

- **One primary action per group.** At most one `tone: "primary"` button in a toolbar row or form. Others are `secondary` or `ghost`.
- **Keyboard:** Enter activates the focused button; Enter in a single-line field submits when the form has exactly one submit button (SPEC §10.4). Phase 1 has no action menus or shortcuts.
- **Button labels are verbs** ("Find laptops", "Refresh"), titles and headings are nouns ("Sales", "Nearby ramen").

## 8. Streaming, loading, motion

**Partial nodes** (status `partial`):

- Text and layout render normally. The user reads along as content arrives.
- Inputs and buttons render in their disabled style until sealed (inputs) or until the stream finishes (buttons). This matches SPEC §7.4 and §10.4.

**Appearing content.**

- A newly inserted sealed node fades in from 0 to 1 opacity over **150 ms, ease-out**.
- Text appended to an existing node does not animate, so long texts stay readable while they grow.
- Nothing slides or moves, and nodes already on screen never animate again.

**Reduced motion.** When the OS asks for reduced motion, or the host disables animation, all fades are off.

**Loading data** (a query or ref with status `loading`):

- **Table:** the header shows, plus 3 skeleton rows. Each skeleton row is a `surface_subtle` bar 12 px high with `sm` radius, at 60% of the column width.
- **Chart:** the plot area shows a `surface_subtle` box at full height.
- **List of refs:** 3 skeleton rows.
- **Image:** the pending box from §7.
- Skeletons pulse their opacity between 0.5 and 1 on a 1.2 s cycle, which reduced motion disables.

**Refreshing** (status `refreshing`): the existing content stays and dims to 70% opacity. No spinner.

**Failed:** the old data or default stays. A single line of `caption` text in `danger.fg` reads "Couldn't load data", below the component. There is no retry button, because retry is the program's own `refresh` action.

**Fatal program** (`extract.version` or `name.no_root`): the fallback markdown renders as prose, preceded by one `caption` line in `muted_foreground`: "This response's interactive view couldn't be shown."

---

### 8.1 Content guidance (feeds prompt section S10)

These are rules for the model, not the renderer. They go into the generated prompt (SPEC §9.1, S10), worded briefly:

1. Lead with the answer: the most important number or fact first, as a Heading or a short Text.
2. Prefer one Card per topic. Don't wrap everything in Cards; a Card around a single Text is noise.
3. At most 3 charts per Row or Grid row. Use a chart only when the data has 4 or more points; otherwise use Text or a Table.
4. Bar charts compare categories; line charts show change over time.
5. Table: at most 8 columns; put the identifying column first.
6. Use Badge for status, not colored Text. Use Alert only for something the user must notice.
7. One primary button per group; button labels are verbs.
8. Don't set `gap` or `padding` unless the defaults are clearly wrong.
9. Show headline numbers as a Grid of `Stat`s, and details as `Facts`, not as hand-built Rows of Text.

## 9. Accessibility

- Contrast: AA everywhere, checked in tests (§4.2).
- Focus is always visible (§7). The keyboard order follows the tree.
- Minimum click target is 24×24. Default controls are 32 high.
- Never use color alone: errors also have text, status badges have text, and chart series have tooltips and the table fallback.
- Roles and labels come from the IR (SPEC §5.4, §10.10).

---

## 10. Implementation contract (GPUI)

- **`UiTheme`** lives in `<name>-gpui::theme`. It holds every token in this document and is the only module allowed to contain literal colors or sizes (AGENTS.md quality bar).
- **Built on gpui-base.** It is built from gpui-base `SemanticThemeTokens` (colors, radius, spacing, typography), plus the additions here: `surface_subtle`, our `accent`, the tones, the chart palette and control heights.
- **Token adapter.** gpui-base's spacing, type and radius tokens are named (7 spacing entries, for example), not this document's numeric scales. The theme module maps them explicitly, and **this document's scales are authoritative**: gpui-base supplies default colors, and every size comes from §2, §3 and §5 here. `Theme::global` returns resolved `Hsla` colors; semantic tokens live under `theme_tokens`.
- **Light and dark** follow the active gpui-kit theme mode.
- **Host overrides.** A host MAY override:
  - any color role or tone;
  - the UI and mono font families;
  - the radius scale, as one multiplier from 0 to 2.

  It MAY NOT change the spacing or type scales in phase 1. They are tuned with the layout rules.
- **Verification.** Before use, check each gpui-base token name and its accessor against docs.rs for `gpui-base 0.8.0`, and record any differences in `PINS.md`.
- **Visual check.** A style-guide window (a GPUI demo binary, ROADMAP M6) renders every component in every state, in light and dark, and is compared against this document.

---

## 11. Where the values come from

| Decision | Basis |
| --- | --- |
| 4 px spacing, steps 0–8 | Radix spacing steps and gpui-kit control sizes (baselines) |
| Body 14 / controls 32 | gpui-kit medium controls use 14 px text and 32 px height. macOS body is 13 pt. ChatGPT widgets use 14–16 (chatgpt-design). |
| Weights 400/500/600 only | ChatGPT widgets use exactly these weights (chatgpt-design) |
| Hairline borders, no shadows in flow | Both ChatGPT (1 px light borders, almost no shadows) and openui (alpha borders, minimal shadow). It suits dense data UIs. |
| One emphasis per card, muted secondary text | ChatGPT's single prominent value per card with muted captions (chatgpt-design) |
| Neutral palette values | gpui-base defaults (shadcn neutral), so out-of-the-box kit widgets already match |
| Blue accent (not neutral, not teal) | Focus and selection need a non-neutral color for accessibility. Blue is the conventional, low-risk choice, and it is distinct from both references' identities. |
| Okabe-Ito charts | Colorblind-safe categorical palette (baselines). gpui-kit's chart colors are a single-hue ramp. |
| Card radius 12 | gpui-base `xl`. ChatGPT uses 16–20 and openui up to 16; 12 is more native-feeling on desktop. |
| Table rules (truncation, alignment by type, header wrap) | IBM Carbon data table guidance (data-dense) |
| Zero-based bars, null gaps, ~4 round gridlines, no single-series legend | Carbon data visualization and Apple chart guidance (data-dense) |
| Same height across loading, empty and loaded | Stripe Apps chart guidance (data-dense) |
| Errors after blur, one primary action, nouns for titles | Raycast extension guidelines (constrained-vocab) |
| Model owns content, host owns look | Adaptive Cards design principles (constrained-vocab) |
| 150 ms fade, no per-word animation | ChatGPT fades each word over 0.7 s, which reads as slow in a dense UI. We fade whole nodes briefly, and leave text static. |

---

## Appendix A. Proposed spec additions

These came from the research and change the component vocabulary. **Decision (2026-10-21): P1, P2 and P3 are adopted into SPEC §8.8. P4, P5 and P6 are deferred (SPEC Appendix A)**, because each adds a choice the model can get wrong and the defaults already work for chat.

| # | Proposal | Signature sketch | Why | Source |
| --- | --- | --- | --- | --- |
| P1 | **Stat** (KPI card content) | `Stat(label: text, value: scalar; delta: number, good: "up" \| "down" = "up", format: col_format = "number", caption: text)` | The most common dashboard element; today the model fakes it with Heading + Text, inconsistently. Anatomy: caption-size label, 24/32 600 value, delta Badge colored by `good`. | Tremor, Stripe Apps |
| P2 | **Facts** (key-value list) | `Facts(items: list<{label, value}>; columns: int 1..2 = 1)` | Details panels (order info, specs, weather details) are everywhere; a Table is too heavy for them. | Adaptive Cards FactSet, Raycast Detail metadata |
| P3 | More number formats | add `"compact"` (1.2K, 3.4M), `"currency"` (with a `currency: "USD"` prop on TableColumn/Stat), `"delta"` (signed, colored) to `col_format` | Big numbers and money are the most common cells in dashboards. | Carbon, MDN Intl.NumberFormat |
| P4 | Table density | `Table(...; density: "compact" \| "comfortable" = "compact")`, rows 32 or 40 | Long lists want compact; small summary tables look better comfortable. | Carbon |
| P5 | Chart height tiers | `Chart(...; height: "sm" \| "md" \| "lg" = "md")`, 180 / 240 / 320 | Charts inside Grid cells need to be smaller than a featured chart. | Stripe Apps, Tremor |
| P6 | List item accessories | `ListItem(title: text; subtitle: text, badge: text, value: text)` as an optional structured child of List | Makes ref-backed lists (places, tickets) consistent instead of ad-hoc Rows. | Raycast List accessories |

Suggested order if approved: P1, P2 and P3 first (they cover most dashboards), then P4–P6.
