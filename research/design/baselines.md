# Design-system baselines for the open-ui native renderer

Gathered 2026-10-21. Target stack: gpui-kit 0.8.0 (crates `gpui-kit`, `gpui-component`, `gpui-base`) over GPUI (`gpui-pre` =0.3.9).

Legend: **[V]** verified from source or fetched page. **[D]** derived by us from verified values (e.g. hex from HSLA). **[U]** unverified or from memory; check before relying on it.

Source files for gpui-kit were read from the crates.io 0.8.0 tarballs (`https://static.crates.io/crates/<crate>/<crate>-0.8.0.crate`), not from a git checkout.

---

## A. gpui-kit 0.8.0 (our rendering dependency)

### A1. Packages and versions

| Item | Value | Source |
|---|---|---|
| Latest crate version | `0.8.0` (max stable) | [crates.io API for gpui-kit](https://crates.io/api/v1/crates/gpui-kit) [V] |
| Docs install line | `gpui-kit = "0.8.0"`; `version = "0.8.0", features = ["gpui-fast"]` optional | [gpui-kit.com getting started](https://gpui-kit.com/docs/getting-started) [V] |
| GPUI pin in docs | `gpui = { package = "gpui-pre", version = "=0.3.9" }`, `gpui_platform = { package = "gpui-pre-platform", version = "=0.3.9", features = ["font-kit"] }` | same page [V] |
| Crate layout | `gpui-kit` (re-exports), `gpui-base` (unstyled behaviour and tokens), `gpui-component` (styled widgets), `gpui-kit-assets` (Lucide icons) | [GitHub README](https://raw.githubusercontent.com/longbridge/gpui-kit/main/README.md) [V] |
| Stale README | The GitHub README still says `gpui-kit = "0.7"`. The docs site and crates.io say 0.8.0. Treat the README as stale. | [V] |
| API docs | https://docs.rs/gpui-kit/ and https://docs.rs/gpui-component/0.8.0 | [V] |
| Gallery | No hosted gallery URL found. The gallery is the `story` crate, run with `cargo run` from a checkout. No screenshots captured (gpui-kit.com is JS-rendered). | [V] README and docs |

### A2. Theme architecture (two layers)

- **Legacy surface: `ThemeColor` in `gpui-component`.** 139 fields, read as `cx.theme().background` and similar. Field roles are listed in [docs.rs ThemeColor](https://docs.rs/gpui-component/0.8.0/gpui_component/theme/struct.ThemeColor.html) [V].
- **Semantic surface: `SemanticThemeTokens` in `gpui-base`.** Groups `colors`, `radius`, `spacing`, `typography`, `shadow`. Source: `gpui-base-0.8.0/src/theme_tokens.rs` [V].
- `gpui-component-0.8.0/src/theme/mod.rs` says: *"New application-owned presentation should use `semantic_tokens` rather than extending this legacy surface."* [V]. **We should build on the semantic tokens.**

### A3. Semantic color tokens (gpui-base defaults)

Light and dark defaults from `ColorTokens::light()` and `ColorTokens::dark()`. Hex is derived [D] by rounding each HSLA lightness to 0–255. The source says the light palette is "aligned with gpui-component's Default Light theme".

| Token | Light (hsla) | Light hex [D] | Dark (hsla) | Dark hex [D] |
|---|---|---|---|---|
| background | (0,0,1.000) | #ffffff | (0,0,0.039) | #0a0a0a |
| foreground | (0,0,0.039) | #0a0a0a | (0,0,0.980) | #fafafa |
| surface | = background | #ffffff | = background | #0a0a0a |
| primary | (0,0,0.090) | #171717 | (0,0,0.980) | #fafafa |
| primary_foreground | (0,0,0.980) | #fafafa | (0,0,0.090) | #171717 |
| secondary | (0,0,0.898) | #e5e5e5 | (0,0,0.149) | #262626 |
| secondary_foreground | (0,0,0.090) | #171717 | (0,0,0.980) | #fafafa |
| muted | (0,0,0.961) | #f5f5f5 | (0,0,0.149) | #262626 |
| muted_foreground | (0,0,0.451) | #737373 | (0,0,0.639) | #a3a3a3 |
| accent | (0,0,0.961) | #f5f5f5 | (0,0,0.149) | #262626 |
| destructive | (0,0.842,0.602) | ~#ef4444 | (0,0.906,0.708) | ~#f87171 |
| destructive_foreground | (0,0,0.980) | #fafafa | (0,0.722,0.506) | ~#dc2626 |
| border | (0,0,0.898) | #e5e5e5 | (0,0,0.149) | #262626 |
| input | (0,0,0.898) | #e5e5e5 | (0,0,47/255) | #2f2f2f |
| ring | (0,0,0.639) | #a3a3a3 | (0,0,0.451) | #737373 |
| selection | #55a0fc @ 30% alpha | | #1d4ed8 @ 30% alpha | |

Contrast [D], WCAG 2.x formula: `muted_foreground` on background is about 4.7:1 in light mode and about 7.8:1 in dark mode. Both pass AA for normal text.

### A4. Default bundled theme (shadcn "neutral" palette)

From `gpui-component-0.8.0/src/theme/default-theme.json`, with names resolved through `default-colors.json` [V, resolved by script]. The two entries are "Default Light" (`is_default: true`) and "Default Dark".

| Role key | Light | Dark |
|---|---|---|
| background | #ffffff | #0a0a0a |
| foreground | #0a0a0a | #fafafa |
| border | #e5e5e5 | #262626 |
| input.border | #e5e5e5 | #2f2f2f |
| ring | #a3a3a3 | #737373 |
| primary.background | #171717 | #fafafa |
| primary.foreground | #fafafa | #171717 |
| primary.hover.background | #262626 | #f5f5f5 |
| secondary.background | #e5e5e5 | #262626 |
| secondary.foreground | #171717 | #fafafa |
| muted.background | #f5f5f5 | #262626 |
| muted.foreground | #737373 | #a3a3a3 |
| accent.background | #f5f5f5 | #262626 |
| accent.foreground | #171717 | #fafafa |
| danger.background | #ef4444 (red-500) | #f87171 (red-400) |
| danger.foreground | #fafafa | #dc2626 (red-600) |
| success.background | #22c55e (green-500) | #4ade80 (green-400) |
| warning.background | #eab308 (yellow-500) | #facc15 (yellow-400) |
| info.background | #06b6d4 (cyan-500) | #22d3ee (cyan-400) |
| popover.background | #ffffff | #0a0a0a |
| list.hover.background | #f5f5f5 | (not set) |
| table.head.foreground | #737373 | #525252 |
| sidebar.background | #fafafa | #0a0a0a |
| title_bar.background | #f8f8f8 | #171717 |
| selection.background | #55a0fc | #1d4ed8 |
| link.foreground | #0a0a0a | #fafafa |
| base.blue / base.red / base.green | #2563eb / #dc2626 / #16a34a | #60a5fa / #f87171 / #4ade80 |
| base.yellow / base.magenta / base.cyan | #ca8a04 / #9333ea / #0891b2 | #facc15 / #c084fc / #22d3ee |
| chart.1 … chart.5 | #93c5fd, #3b82f6, #2563eb, #1d4ed8, #1e40af | same as light |

Note: `chart.1`–`chart.5` is a single-hue blue ramp, not a categorical palette. Do not use it for series.

### A5. Other bundled themes

`https://github.com/longbridge/gpui-kit/tree/main/themes` lists 21 JSON themes: adventure, alduin, asciinema, aurora, ayu, catppuccin, everforest, fahrenheit, flexoki, gruvbox, harper, hybrid, jellybeans, kibble, macos-classic, mellifluous, molokai, solarized, spaceduck, tokyonight, twilight [V]. Raw files: `https://raw.githubusercontent.com/longbridge/gpui-kit/main/themes/<name>.json`.

`macos-classic.json` [V] (name suggests macOS look; not compared against Apple's values):

| Key | Light | Dark |
|---|---|---|
| background | #F9F9F9 | #131313 |
| foreground | #000000 | #DEDEDE |
| border | #D2D2D2 | #303030 |
| primary.background | #0060de | #077CFD |
| ring | #0060de | #077CFD |
| muted.foreground | #707070 | #9D9D9D |
| danger.background | (not set) | #E44A4F |
| base.red / base.green / base.yellow / base.blue | #d21f07 / #319a00 / #B59A00 / #0060de | #FF5257 / #30D158 / #FFC600 / #419CFF |

These themes are JSON in the theme-schema format, which uses `colors` with dotted keys and a `highlight` block for syntax. They have no font, radius or size fields.

### A6. Radius, spacing, typography (gpui-base defaults)

| Token | Value | Source |
|---|---|---|
| `RadiusTokens` none / sm / md / lg / xl / full | 0 / 3 / 6 / 8 / 12 / 9999 px | `theme_tokens.rs` [V] |
| `Theme.radius` default | 6 px | `theme/mod.rs` line ~774 [V] |
| `Theme.radius_lg` default | 8 px (Dialog and Notification) | `theme/mod.rs` [V] |
| `radius_2xl` / `radius_3xl` / `radius_4xl` | 2.5x, 3x, 3.5x `radius` (15, 18, 21 px at default) | `theme/mod.rs` [V] |
| `SpacingTokens` xxs / xs / sm / md / lg / xl / xxl | 2 / 4 / 8 / 12 / 16 / 24 / 32 px | `theme_tokens.rs` [V] |
| Typography `xs` / `sm` / `md` / `lg` / `xl` | 12/16, 14/20, 16/24, 18/28, 20/28 (size/line-height px) | `theme_tokens.rs` [V] |
| Typography `mono_md` | 13/20 px | `theme_tokens.rs` [V] |
| `Theme.font_size` default | 16 px (base) | `theme/mod.rs` doc comment [V] |
| `Theme.mono_font_size` default | 13 px | `theme/mod.rs` doc comment [V] |
| Default sans family | `.SystemUIFont` | `theme_tokens.rs` [V] |
| Default mono family | macOS Menlo, Windows Consolas, Linux DejaVu Sans Mono | `theme/mod.rs` doc comment [V] |
| Font weight tokens | Not extracted | [U] |

The helper methods on `Styled` (for example `h_5()`, `px_2p5()`, `text_sm()`) are used in `sizing.rs`. The `h_N` and `px_N` values follow the Tailwind convention, so 1 unit = 4 px and `text_sm` = 14 px / 20 px line height. The gpui helper source was not downloaded, so this is inferred from usage [U].

### A7. Component size scale (`Size`)

`Size` is `XSmall | Small | Medium (default) | Large | Size(Pixels)` [V, `sizing.rs`].

| Control | XSmall | Small | Medium (default) | Large |
|---|---|---|---|---|
| Input height (`input_h`) | 20 (h_5) | 24 (h_6) | 32 (h_8) | 44 (h_11) |
| Input horizontal padding | 4 | 8 | 10 | 12 |
| Input vertical padding | 0 | 2 | 8 | 10 |
| Input text size | 12 (text_xs) | 14 (text_sm) | 14 | 16 (text_base) |
| Button height (normal) | 20 (h_5, px 4) | 24 (h_6, px 8) | 32 (h_8, px 10) | 32 (h_8, px 12) |
| Icon-only button | 20 | 24 | 32 | 32 |
| Button text size | 12 | 14 | 14 | 16 |
| Toggle height | 20 | 24 | 32 | 36 (h_9) |
| Table row height | 26 | 30 | 32 | 40 |
| Table cell padding (v / h) | 2 / 4 | 3 / 6 | 4 / 8 | 8 / 12 |
| Table cell text | 12 | 14 | 14 | 16 |
| Tab height, underline variant | 26 | 30 | (not read) | 44 |
| Tab height, other variants | 20 | 24 | (not read) | 36 |

Custom sizes (`Size::Size(px)`) scale: button horizontal padding is `px * 0.2`, table row height is `px`, and text size is `px * 0.875` (for inputs).

### A8. Layout and component token names (from docs.rs)

ThemeColor groups that matter for us, with their documented roles [V, docs.rs]:
- Core: `background`, `foreground`, `border`, `accent`, `accent_foreground`.
- Semantic: `primary`, `secondary`, `danger`, `info`, `success`, `warning`, each with `_active`, `_foreground`, `_hover`.
- Controls: `input`, `ring`, `caret`, `selection`, `overlay`, `drag_border`, `drop_target`.
- Surfaces: `popover`, `group_box`, `sidebar` (plus `sidebar_*`), `title_bar`, `status_bar`, `window_border` (Linux only).
- Data: `table*` (`table_head`, `table_even`, `table_hover`, `table_row_border`, `table_active`), `list*`, `tab*`, `scrollbar*`, `switch`, `slider_*`, `progress_bar`, `skeleton`.
- Links: `link`, `link_hover`, `link_active`.
- Charts: `chart_1` to `chart_5`, `chart_bullish`, `chart_bearish`, `chart_grid`.
- Base palette: `red`, `green`, `blue`, `yellow`, `magenta`, `cyan`, each with `_light`.

---

## B. Zed editor (built on GPUI)

### B1. Defaults

| Setting | Value | Source |
|---|---|---|
| `ui_font_family` | `".ZedSans"` (name only; the family identity was not verified) | [Zed default.json](https://raw.githubusercontent.com/zed-industries/zed/main/assets/settings/default.json) [V for the string] |
| `ui_font_size` | 16 | same [V] |
| `ui_font_weight` | 400 | same [V] |
| `buffer_font_family` | `".ZedMono"` (docs say it aliases Lilex) | same, and [docs](https://zed.dev/docs/configuring-zed) [V] |
| `buffer_font_size` | 15 | same [V] |
| `buffer_line_height` | `"comfortable"` (options `standard`, `comfortable`, `{custom}`; numeric value not extracted) | docs [V] |
| `agent_buffer_font_size`, `git_commit_buffer_font_size` | 12 | default.json [V] |
| Theme mode | `system`, light `One Light`, dark `One Dark` | default.json [V] |
| Density key | No `ui_density` setting exists. The closest is `entry_spacing: "comfortable"` (project panel). | [V] |

### B2. One Dark / One Light key colors

From `assets/themes/one/one.json` [V]. Alpha suffix `ff` stripped.

| Role | One Dark | One Light |
|---|---|---|
| background | #3b414d | #dcdcdd |
| surface.background (= elevated, tab bar) | #2f343e | #ebebec |
| border | #464b57 | #c9c9ca |
| border.variant | #363c46 | #dfdfe0 |
| text | #dce0e5 | #242529 |
| text.muted | #a9afbc | #58585a |
| text.accent | #74ade8 | #5c78e2 |
| element.background | #2e343e | #ebebec |
| element.hover | #363c46 | #dfdfe0 |
| element.selected | #454a56 | #cacaca |
| editor.background | #282c33 | #fafafa |
| terminal.background | #282c34 | #fafafa |
| players[0] cursor / selection | #74ade8 / #74ade8 @ 24% | #5c78e2 / #5c78e2 @ 24% |

### B3. ThemeColors (Zed)

144 `Hsla` fields in `crates/theme/src/styles/colors.rs` [V]. Groups: borders (`border`, `border_variant`, `border_focused`, `border_selected`, `border_disabled`), surfaces (`surface_background`, `background`, `elevated_surface_background`, `panel_background`), element states (`element_background/hover/active/selected/disabled`), ghost elements, text (`text`, `text_muted`, `text_placeholder`, `text_disabled`, `text_accent`), icons (`icon`, `icon_muted`, `icon_accent`), status bar and title bar, tab and toolbar, scrollbars, editor, terminal ANSI, version control. The file does not contain default hex values; they come from constructors not fetched [U].

Zed's density is expressed in its settings, not in a token. This matters for us because Zed uses 16 px UI text with 15 px buffer text, so Zed is not a dense desktop baseline.

---

## C. Public design systems

### C1. Radix Colors 3.0.0 (12-step scales)

Source: `https://www.npmjs.com/package/@radix-ui/colors` (tarball fetched) [V].

Step roles, from [Radix palette docs](https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale) [V]:

| Step | Role |
|---|---|
| 1 | App background |
| 2 | Subtle background (cards, sidebars, code blocks) |
| 3 | UI element background (normal state) |
| 4 | Hovered UI element background |
| 5 | Active or selected UI element background |
| 6 | Subtle borders and separators |
| 7 | UI element border and focus ring |
| 8 | Hovered UI element border |
| 9 | Solid background (highest chroma). Most pair with white text. Sky, Mint, Lime, Yellow, Amber pair with dark text. |
| 10 | Hovered solid background |
| 11 | Low-contrast text |
| 12 | High-contrast text |

Contrast: steps 11 and 12 are guaranteed to Lc 60 and Lc 90 APCA on a step 2 background of the same scale [V, docs].

Gray (light / dark), exact values [V, `gray.css` and `gray-dark.css`]:

| Step | Light | Dark |
|---|---|---|
| 1 | #fcfcfc | #111111 |
| 2 | #f9f9f9 | #191919 |
| 3 | #f0f0f0 | #222222 |
| 4 | #e8e8e8 | #2a2a2a |
| 5 | #e0e0e0 | #313131 |
| 6 | #d9d9d9 | #3a3a3a |
| 7 | #cecece | #484848 |
| 8 | #bbbbbb | #606060 |
| 9 | #8d8d8d | #6e6e6e |
| 10 | #838383 | #7b7b7b |
| 11 | #646464 | #b4b4b4 |
| 12 | #202020 | #eeeeee |

Indigo light (accent example) [V]: 9 = #3e63dd, 10 = #3358d4, 11 = #3a5bc7, 12 = #1f2d5c.
Blue light [V]: 9 = #0090ff, 11 = #0d74ce, 12 = #113264.

### C2. Radix Themes 3.3.0 (tokens)

Source: `@radix-ui/themes` 3.3.0 tarball, `tokens/base.css` and `tokens.css` [V]. Values are at `--scaling: 1`.

| Token | Values (px) |
|---|---|
| `--space-1` … `--space-9` | 4, 8, 12, 16, 24, 32, 40, 48, 64 |
| `--font-size-1` … `--font-size-9` | 12, 14, 16, 18, 20, 24, 28, 35, 60 |
| `--line-height-1` … `--line-height-9` | 16, 20, 24, 26, 28, 30, 36, 40, 60 |
| `--default-font-size` | `--font-size-3` = 16 |
| `--default-font-family` | system stack (`-apple-system, BlinkMacSystemFont, …, system-ui`) |
| Radius base (factor 1, "medium") | 3, 4, 6, 8, 12, 16 (`--radius-1` … `--radius-6`) |
| Radius factor | none 0; small 0.75; medium 1; large 1.5; full = 1.5 with `--radius-full: 9999px` |
| Scaling | 90%, 95%, 100%, 105%, 110% (multiplies everything) |
| Button height, size 1 / 2 / 3 / 4 | `--space-5` 24, `--space-6` 32, `--space-7` 40, `--space-8` 48 |
| Text field height, size 1 / 2 / 3 | `--space-5` 24, `--space-6` 32, `--space-7` 40 |

### C3. Vercel Geist

Sources: https://vercel.com/geist/colors, /typography, /materials, /grid (fetched as `.md`) [V for roles and radii]. **Exact hex and px values were not available** [U].

Color roles [V]:
- Background 1 (`--ds-background-100`): default element background.
- Background 2 (`--ds-background-200`): secondary, use sparingly.
- Gray 100 to 1000 (ten steps). 100 = default component background; 200 = hover; 300 = active; 400 = default border; 500 = hover border; 600 = active border; 700 = high-contrast background; 800 = hover high-contrast; 900 = secondary text and icons; 1000 = primary text and icons.
- Colored scales: blue, red, amber, green, teal, purple, pink, same step roles. `gray-alpha` for overlays.

Radius presets [V, materials.md]: `material-base` 6, `material-small` 6, `material-medium` 12, `material-large` 12, `material-tooltip` 6, `material-menu` 12, `material-modal` 12, `material-fullscreen` 16 (px).

Typography class names [V]: headings 72, 64, 56, 48, 40, 32, 24, 20, 16, 14; buttons 16, 14 (default), 12 (inside inputs); labels 20, 18, 16, 14, 13, 12 (plus mono); copy 24, 20, 18, 16, 14, 13 (plus mono). Pixel sizes are inferred from the class names [U]. Font families: sans and mono (not named on the page).

Spacing and grid: no numeric tokens on the Geist pages [U].

### C4. Apple HIG for macOS

Source: Apple HIG JSON (`https://developer.apple.com/tutorials/data/design/human-interface-guidelines/typography.json`) [V]. The HTML page renders empty without JS.

macOS text styles, point size / leading [V]:

| Style | Size pt | Leading pt | Notes |
|---|---|---|---|
| Large Title | 26 | 32 | Weight not extracted [U] |
| Title 1 | 22 | 26 | |
| Title 2 | 17 | 22 | |
| Title 3 | 15 | 20 | |
| Headline | 13 | 16 | Bold per HIG convention [U] |
| Body | 13 | 16 | |
| Callout | 12 | 15 | |
| Subheadline | 11 | 14 | |
| Footnote | 10 | 13 | |
| Caption 1 | 10 | 13 | |
| Caption 2 | 10 | 13 | |

The macOS block also says: "SF Pro is the system font in macOS … macOS doesn't support Dynamic Type." [V]. Dynamic system font variants are exposed through AppKit (`NSFont.controlContentFont`, `labelFont`, `menuFont`, `systemFont(ofSize:)` and so on) [V].

Not verified [U]:
- macOS control heights (push button, text field, mini/small/regular) and margins. The HIG buttons page JSON lists only visionOS sizes (Mini 28, Small 32, Regular 44, Large 52, Extra large 64 pt), so do not use those for macOS.
- The layout JSON has no macOS margin numbers (it only lists tvOS 60/80 pt safe areas and visionOS grid values).

---

## D. Data visualization and tables

### D1. Colorblind-safe categorical palette: Okabe-Ito

The eight colors below were confirmed by one secondary source [V-secondary], which quotes Okabe & Ito's set. [Concept Viz reference](https://dev.conceptviz.app/fr/blog/okabe-ito-palette-hex-codes-complete-reference) [V-secondary, search snippet].

| Name | Hex |
|---|---|
| Black | #000000 |
| Orange | #E69F00 |
| Sky blue | #56B4E9 |
| Bluish green | #009E73 |
| Yellow | #F0E442 |
| Blue | #0072B2 |
| Vermillion | #D55E00 |
| Reddish purple | #CC79A7 |

Guidance from [jfly.uni-koeln.de/color](https://jfly.uni-koeln.de/color/) [V]: alternate warm and cool colors; when two warm or two cool colors sit together, separate them by brightness or saturation; for thin lines and small text use darker blue and orange rather than sky blue and yellow; do not rely on color alone (pair with shape, position, line type, or direct labels). That page gives a different vermillion (#FF2000) from an older proposal, so do not mix the two sources.

### D2. Tableau 10

The Tableau blog returned HTTP 403 [U]. The Vega `tableau10` scheme is verified from `vega-scale` 8.1.3 source, `src/palettes.js` [V]:

| # | Hex |
|---|---|
| 1 | #4c78a8 |
| 2 | #f58518 |
| 3 | #e45756 |
| 4 | #72b7b2 |
| 5 | #54a24b |
| 6 | #eeca3b |
| 7 | #b279a2 |
| 8 | #ff9da6 |
| 9 | #9d755d |
| 10 | #bab0ac |

Colorblind safety of Tableau 10 was not verified [U]. Note that #e45756 (red) and #54a24b (green) are close in lightness, which is a common CVD problem. Do not rely on Tableau 10 alone for colorblind safety.

### D3. Table row heights and numbers

| System | Row heights (px) | Horizontal padding | Source |
|---|---|---|---|
| gpui-component `Size` | XSmall 26, Small 30, Medium 32, Large 40 | 4 / 6 / 8 / 12 | sizing.rs [V] |
| Carbon (IBM), data table | xs 24, sm 32, md 40, lg 48, xl 64 | 16 (`$spacing-05`) | [carbondesignsystem.com](https://carbondesignsystem.com/components/data-table/style/) [V] |
| Carbon row text | 14 px / 1 rem, Regular | | same [V] |

Number alignment, from [Adobe Spectrum table guidance](https://spectrum.adobe.com/page/table) [V-search snippet, not fetched]:
- Right-align numerical data for scanning and comparing. Left-align only arbitrary identifiers (ZIP codes, phone numbers).
- Column headers follow the column's alignment.
- Use tabular (monospaced) and lining figures when users compare values.

---

## Implications for us

Each line gives the proposal, then a short rationale with sources.

### Spacing scale (4 px base, steps 0 to 8)

| Step | 0 | 1 | 2 | 3 | 4 | 5 | 6 | 7 | 8 |
|---|---|---|---|---|---|---|---|---|---|
| px | 0 | 4 | 8 | 12 | 16 | 20 | 24 | 28 | 32 |

Rationale: Radix Themes uses 4, 8, 12, 16, 24, 32 as its base steps [V]. gpui-component control heights use 20, 24, 32 [V]. gpui-base `SpacingTokens` has a 2 px half step (xxs), which we keep only as an optional extra, not in the 0 to 8 scale. Steps 9 and up (40, 48, 64) are for page layout, not control spacing.

### Type scale

| Role | Size / line-height px | Rationale |
|---|---|---|
| caption | 12 / 16 | gpui-base `xs` [V]; Radix font-size-1 [V]; gpui `Size::XSmall` text [V] |
| small / mono | 13 / 18 | gpui-base `mono_md` 13 [V]; macOS Body and Callout use 12–13 pt [V] |
| control and body (default) | 14 / 20 | gpui-component Medium and Small controls and table cells use `text_sm` (14) [V]; Radix size 2 type |
| body large | 16 / 24 | gpui-base `md` and gpui default `font_size` 16 [V]; Radix default font size 16 [V] |
| title | 20 / 28 | gpui-base `xl` [V]; Radix font-size-5 with line-height 28 [V] |
| heading | 24 / 32 | Radix font-size-6 (24) [V]; Geist headings are 24 and 32 [V-inferred from names] |
| display | 28 / 36 | Radix font-size-7 (28) with line-height 36 [V] |

We should not take gpui's 16 px default as our body size. gpui-component's own Medium controls use 14 px, and macOS body is 13 pt [V]. Keep 16 for large body only.

### Radius scale

| Token | px | Use | Rationale |
|---|---|---|---|
| none | 0 | tables, dividers | gpui-base `none` [V] |
| sm | 4 | badges, chips, checkboxes | Radix radius-2 at factor 1 [V] |
| md | 6 | buttons, inputs, selects | gpui-component default `radius` 6 [V]; Geist base and tooltip 6 [V] |
| lg | 8 | popovers, menus, dialogs | gpui `radius_lg` 8 [V] |
| xl | 12 | sheets, large cards | gpui-base `xl` 12 [V]; Geist menu and modal 12 [V] |
| full | 9999 | avatars, pills, toggles | gpui-base `full` [V] |

### Color roles

Use the gpui-base semantic names (section A3) as the public role set: `background`, `foreground`, `surface`, `primary`, `secondary`, `muted`, `muted_foreground`, `accent`, `destructive`, `border`, `input`, `ring`, `selection`. Add `success`, `warning`, `info` from gpui-component defaults (section A4).

Light / dark starting values: use A3 for the core roles and A4 for status colors. Contrast of `muted_foreground` is about 4.7:1 light and 7.8:1 dark [D].

If we need a full ramp, use Radix's 12-step role mapping (section C1) with the neutral scale from A4. Assign steps 1–2 to backgrounds, 3–5 to element states, 6–8 to borders, 9 to solid fills, and 11–12 to text.

Do not use gpui-component's `chart.1`–`chart.5` as a series palette (section A4).

### Chart palette (categorical, colorblind-aware)

Okabe-Ito in this order: #0072B2 (blue), #E69F00 (orange), #009E73 (bluish green), #CC79A7 (reddish purple), #56B4E9 (sky blue), #D55E00 (vermillion). Use #000000 for neutral or "other" series. Reserve #F0E442 (yellow) for fills only, never for text or thin lines.

Rationale: Okabe-Ito is designed to be distinguishable for common color-vision deficiencies [V-secondary, D1]. Tableau 10 is not verified as colorblind-safe (D2). Always pair color with a label or line style [V, D1 guidance].

Dark mode: lighter variants of the same hues are likely needed [U].

### Control heights

| Size | Height | Use | Rationale |
|---|---|---|---|
| XS | 20 | dense toolbars, inline table controls | gpui-component XSmall Button and Input [V] |
| S | 24 | secondary actions | gpui-component Small [V]; Radix size 1 [V] |
| M (default) | 32 | default button, input, select | gpui-component Medium [V]; Radix size 2 [V] |
| L | 40 | prominent action, larger touch targets | Radix size 3 [V]. gpui-component's Large input is 44 [V], so 44 is the alternative if we want its exact value. |

### Table

- Row heights: compact 26 (gpui XSmall) / default 32 (gpui Medium; Carbon sm 32) / comfortable 40 (gpui Large; Carbon md 40) [V].
- Cell padding: default 4 px vertical, 8 px horizontal (gpui Medium) [V]. Carbon uses 16 px horizontal for a looser table [V].
- Cell text: 14 px default, 12 px compact, 16 px comfortable [V].
- Numbers: right-aligned, tabular figures, headers follow the column alignment [V-search snippet, D3].

### Theme plumbing

- Build our theme on gpui-base `SemanticThemeTokens` and not the legacy `ThemeColor` (section A2). The crate itself says so [V].
- Start from the default neutral theme (section A4) and keep the status and chart choices from this file.

---

## Unverified or open items

1. gpui-kit gallery screenshots: not captured. The gallery is the `story` crate, run locally.
2. gpui-kit `Theme` font weight tokens: not extracted.
3. Zed `ui_font_family` identity (`.ZedSans`): name only, not checked against the font.
4. Zed `buffer_line_height: "comfortable"`: numeric multiplier not extracted.
5. Geist exact hex values, px sizes (except class-name inference), spacing and grid tokens: not found.
6. Apple macOS control heights and window margins: not verified. Use AppKit's `NSControl.ControlSize` documentation before relying on any value.
7. Tableau 10 classic hex (Tableau's own palette): blog returned 403. Only the Vega version is verified.
8. Okabe-Ito hex: verified through one secondary source. Confirm against the original Okabe & Ito publication before shipping.
9. Adobe Spectrum table guidance: from a search snippet, not a fetched page.
10. gpui helper scale (`h_5` = 20 px, `text_sm` = 14 px): inferred from the Tailwind convention, not from gpui source.
11. The GitHub README says `gpui-kit = "0.7"`, which contradicts the 0.8.0 docs and crates.io. Treat the README as stale.

## Sources

- gpui-kit 0.8.0 crates (downloaded and read): https://static.crates.io/crates/gpui-kit/gpui-kit-0.8.0.crate, https://static.crates.io/crates/gpui-component/gpui-component-0.8.0.crate, https://static.crates.io/crates/gpui-base/gpui-base-0.8.0.crate
- gpui-kit docs: https://gpui-kit.com/docs/getting-started, https://docs.rs/gpui-kit/, https://docs.rs/gpui-component/0.8.0/gpui_component/theme/struct.ThemeColor.html
- gpui-kit README: https://raw.githubusercontent.com/longbridge/gpui-kit/main/README.md
- gpui-kit themes: https://github.com/longbridge/gpui-kit/tree/main/themes
- Zed settings: https://raw.githubusercontent.com/zed-industries/zed/main/assets/settings/default.json, https://zed.dev/docs/configuring-zed
- Zed One theme: https://raw.githubusercontent.com/zed-industries/zed/main/assets/themes/one/one.json
- Zed ThemeColors: https://raw.githubusercontent.com/zed-industries/zed/main/crates/theme/src/styles/colors.rs
- Radix Colors 3.0.0: https://registry.npmjs.org/@radix-ui/colors/-/colors-3.0.0.tgz
- Radix Themes 3.3.0: https://registry.npmjs.org/@radix-ui/themes/-/themes-3.3.0.tgz
- Radix palette docs: https://www.radix-ui.com/colors/docs/palette-composition/understanding-the-scale
- Geist: https://vercel.com/geist/colors.md, https://vercel.com/geist/typography.md, https://vercel.com/geist/materials.md, https://vercel.com/geist/grid.md
- Apple HIG: https://developer.apple.com/design/human-interface-guidelines/typography (JSON data: https://developer.apple.com/tutorials/data/design/human-interface-guidelines/typography.json)
- Okabe-Ito guidance: https://jfly.uni-koeln.de/color/
- Okabe-Ito hex (secondary): https://dev.conceptviz.app/fr/blog/okabe-ito-palette-hex-codes-complete-reference
- Tableau 10 (Vega source): https://registry.npmjs.org/vega-scale/-/vega-scale-8.1.3.tgz (src/palettes.js)
- Carbon data table: https://carbondesignsystem.com/components/data-table/style/
- Adobe Spectrum table: https://spectrum.adobe.com/page/table
