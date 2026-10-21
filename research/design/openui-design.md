# OpenUI public design study

Scope: visual design of the rendered OpenUI website, docs, demos and playground, observed in a browser as a user would see them. Computed styles were read with `getComputedStyle` on live pages. Nothing under `research/openui/` was opened, and no GitHub source was read.

Measured values are exact unless marked "(guess)" or "(approx)". "Approx" means a hex was converted by eye from an oklch/oklab/lab value.

## 1. URLs visited

| URL | What it shows | Theme |
|---|---|---|
| https://www.openui.com/ | Marketing home: hero, "Works with the tools you already use" grid, LangShowcase (dashboard answer panels), benchmark block, Gateway pitch, FAQ | Light |
| https://www.openui.com/playground | Playground shell only. Redirected to `/openui-vs-json`, which renders the same playground UI. Not generated (see section 7). | Light |
| https://www.openui.com/docs | Docs intro (Fumadocs layout) | Light (docs also has dark, toggled by a header button) |
| https://www.openui.com/docs/openui-lang | OpenUI Lang docs page; contains the JSON vs OpenUI Lang token comparison | Light (dark tokens also read) |
| https://www.openui.com/chat?form_factor=desktop | "OpenUI Cloud Chat" empty state (not sent) | Light |
| https://www.openui.com/demo/intelligent-ui | "Open Intelligent UI" chat empty state (not sent). Dark by default; theme toggled to light. | Dark, then light |
| https://www.openui.com/demo/f1-dashboard | F1 themed dashboard demo (own brand theme, not the default system) | Light |
| https://www.openui.com/demo/trip-planner/planner | Trip planner demo: serif editorial headline, illustrated cards | Light |
| https://www.openui.com/demo/github | GitHub dashboard demo, username input state only (not submitted) | Light |

Not done: the playground and Gateway flows were not run, because they require sending a prompt. Demo dashboards were not populated for the same reason.

## 2. Overall principles (inferred)

1. **Token-first system.** The live CSS exposes about 1,500 custom properties on `:root`, in layers: raw swatches (`--swatch-*`), semantic tokens (`--iui-*`, `--border-*`, `--accent*`), typography and spacing scales (`--font-*`, `--space-*`, `--radius-*`), and per-product aliases (`--color-fd-*` for docs, `--ps-*`, `--sc-*`, `--faq-*`).
2. **Neutral-first palette.** Chroma is 0 for almost every neutral (`oklch(0.97 0 …)`, `oklch(0.097 0 0)`). Color is reserved for brand and status (teal accent, blue focus, green/amber/red status).
3. **Alpha-based surfaces and borders.** Borders and hover/tint fills are black or white at 2–20% alpha, not fixed greys. This is why the same component works on light and dark.
4. **Soft, layered elevation.** Shadows are two-layer, low-opacity, often with a 0.5px ring (`0 0 0 0.5px …`) and a large soft blur (`0 4px 16px` at ~6%).
5. **Pill and large-radius shapes for input.** The composer is a 28px-radius pill; suggestion tiles are 16px; model/chip controls are fully rounded. Buttons and docs links use 8px.
6. **Inter for product UI.** The UI and docs use Inter with fallback. Display, illustration and demo pages use their own faces (serif in the trip planner; Titillium Web and Saira in the F1 demo; displaySerif, uiSans).
7. **Demos are themed.** The F1 and trip-planner demos carry strong brand identities. The default library sits underneath and is restyled per demo. Treat demo styling as a showcase of theming, not the default look.
8. **Illustration and mascot as brand texture.** The dog mascot appears as a loading indicator, avatar and tab icon. Dither/halftone shader backgrounds sit behind showcase panels.

## 3. Token tables

### 3.1 Typography (measured tokens)

Families:
- `--font-body`, `--font-heading`, `--font-label`: `"Inter", "Segoe UI", Arial, sans-serif`
- `--font-code`: `var(--font-geist-mono), "SFMono-Regular", Menlo, monospace`
- `--font-numbers`: `var(--font-geist-mono), "Inter", …`

Scale:

| Token | Value |
|---|---|
| `--font-size-2xs` | 10px |
| `--font-size-xs` | 12px |
| `--font-size-sm` | 14px |
| `--font-size-md` | 16px |
| `--font-size-lg` | 18px |
| `--font-size-xl` | 20px |
| `--font-size-2xl` | 24px |
| `--font-size-3xl` | 28px |
| `--font-size-4xl` | 32px |
| `--font-size-5xl` | 36px |

Weights: regular 400, medium 500, bold 600, heavy 700.

Observed roles:

| Role | Where | Size / line-height / weight / tracking | Color |
|---|---|---|---|
| Body | Chat, home, body default | 16px / 24px / 400 | oklch(0.097 0 0) (approx #0a0a0a) |
| Chat headline (dark welcome) | Intelligent UI demo "What can I help with?" | 24px / 30px / 400 | oklch(0.985 0 …) (approx #f7f7f7) |
| Composer input | Chat composer textarea | 15px / 20px / 400 | ink |
| Playground page title | "What do you want to build?" | 28px / 42px / 600 | oklch(0.097 0 0) |
| Playground prompt textarea | Playground | 15px / 22.5px / 400 | ink |
| Playground chip | "Weather dashboard" etc. | 13px / 19.5px / 400 | ink at 60% |
| Playground Generate button | | 13px / 19.5px / 500 | cream text on cool blue-cyan (see color) |
| Model selector trigger | Chat/playground | 12px / 12px / 400 | ink |
| Suggestion tile label | Chat welcome | 14px / 20px / 400 | ink at 60% |
| Docs H1 | `/docs/openui-lang` "Introduction" | 34px / 39.1px / 400, tracking -0.68px | rgb(24,25,30) |
| Docs H2 | "Architecture at a Glance" | 24px / 32px / 400, tracking -0.24px | rgb(24,25,30) |
| Docs lead paragraph | Under H1 | 18px / 28px / 400 | rgb(64,64,70) |
| Docs body/sidebar | Sidebar links | 15px / 21.4px / 500 | ink |
| Docs inline code | `root = Stack([...])` | 14px / 28px / 500, mono | rgb(24,25,30) on rgba(0,0,0,0.05) |
| Docs CTA link | "Explore the Gateway" | 14px / 20px / 600 | rgb(24,25,30) |
| Trip-planner H1 | "Where shall we go?" | 64px / 65.28px / 380, tracking -2.24px, serif | rgb(0,0,0) |
| Trip-planner subtitle | | 19px / 28.5px / 380, serif | rgb(95,99,105) |
| Trip-planner chip label | "Four days in Kyoto" | 15px / 18px / 380, serif | rgb(0,0,0) |
| Trip-planner tab | Chat / Explore / Itinerary | 14px / normal / 500 active, 420 inactive, tracking -0.07px, uiSans | rgb(0,0,0) active, rgb(95,99,105) inactive |
| Trip-planner input | | 17px / 23.8px / 420 | rgb(0,0,0) |
| F1 H2 (section titles) | "Season overview" | 26.6px / 26.6px / 600, Saira | rgb(21,21,30) |
| F1 nav label active | "Home" | 18px / 21.6px / 700, Titillium Web | rgb(247,244,241) |
| F1 nav label inactive | | 18px / 21.6px / 600 | rgb(170,170,170) |
| F1 micro label | "FP1 · FRI", "DAYS" | 12px / 18px / 700, Saira, tracking 1.44–1.68px, uppercase | rgb(96,96,102) |
| F1 meta text | "Circuit of the Americas · 23 Oct–25 Oct" | 16px / 24px / 400, Titillium | rgb(96,96,102) |

### 3.2 Spacing (measured tokens)

| Token | Value | Token | Value |
|---|---|---|---|
| `--space-000` | 0 | `--space-l` | 20px |
| `--space-3xs` | 2px | `--space-xl` | 24px |
| `--space-2xs` | 4px | `--space-2xl` | 36px |
| `--space-xs` | 6px | `--space-3xl` | 48px |
| `--space-s` | 8px | | |
| `--space-s-m` | 10px | | |
| `--space-m` | 12px | | |
| `--space-m-l` | 16px | | |

Numbered aliases: `--space-1`=2px, `--space-2`=4px, `--space-3`=6px, `--space-4`=8px, `--space-5`=10px, `--space-6`=12px, `--space-8`=16px, `--space-10`=20px, `--space-12`=24px, `--space-16`=36px. The step is 2px at the bottom, then 4/6/8/10/12/16/20/24/36. It is not a clean 4px grid.

Observed component spacing:
- Chat composer padding `11px 12px 11px 24px`, gap 12px.
- Suggestion tile padding 16px, gap 12px.
- Playground prompt textarea padding `16px 16px 0`.
- Playground chip padding `6px 14px`.
- Model selector padding `0 12px`, gap 6px.
- Trip planner tab padding `6px 14px`.
- LangShowcase card padding about `36px 46px` (measured `35.84px 46.08px`).
- Docs link padding `6px 12px`.
- Docs inline code padding `2px 4px`.

### 3.3 Radii

| Token | Value |
|---|---|
| `--radius-none` | 0 |
| `--radius-3xs` | 1px |
| `--radius-2xs` | 2px |
| `--radius-xs` | 4px |
| `--radius-s` | 6px |
| `--radius-m` | 8px |
| `--radius-l` | 10px |

Observed component radii: chat composer 28px (pill); suggestion tiles 16px; playground chip 20px (pill-ish); playground prompt container 14px; playground Generate button 8px; docs CTA 8px; docs inline code 4px; model selector and history/toggle buttons 9999px; homepage showcase card 18px; showcase outer panel 24px; trip-planner tabs and chips 999px pill; F1 demo cards use cut corners rather than radius.

### 3.4 Borders

Light:
- Composer and chip border: `1px` at 6–12% of near-black (`oklab(0.994 …/0.06)` on chips, `oklab(…/0.12)` on the playground prompt).
- Showcase card border `1px rgba(10,10,10,0.08)`.
- Docs border `rgb(239,240,245)`, about #eff0f5.

Dark (`--border-*` and `--iui-*`):

| Token | Value |
|---|---|
| `--iui-border-subtle` | #2e2e2e |
| `--iui-border` | #383838 |
| `--iui-border-strong` | #4d4d4d |
| `--sc-border` | #ffffff14 (white at 8%) |
| `--faq-border` | #ffffff17 |
| `--border-default` | white at 12% (`--swatch-white-a12`) |
| `--border-interactive` | white at 20% |
| `--border-interactive-emphasis` | white at 40% |
| `--border-interactive-selected` | white |

### 3.5 Shadows

| Token / where | Value |
|---|---|
| `--iui-shadow-sm` (dark) | `0 1px 3px #0000004d` |
| `--iui-shadow` (dark) | `0 4px 20px #00000059` |
| `--ps-shadow` (playground light) | `0 1px 2px #0a0a0a0a, 0 2px 8px #0a0a0a0a` |
| Composer light | `0 0 0 0.5px oklch(0.097 0 0 / 0.08), 0 4px 16px lab(0 0 0 / 0.06)` |
| Playground prompt light | `0 4px 6px -2px oklch(0 0 0 / 0.025), 0 2px 2px oklch(0 0 0 / 0.05)` |
| Showcase card light | `0 1px 3px rgba(0,0,0,0.06), second layer rgba(0,0,0,0.28) (truncated in capture)` |

### 3.6 Colors by role

#### Light (default site)

| Role | Value |
|---|---|
| Page canvas (chat main, body) | oklch(0.97 0 89.9) (approx #f7f7f7) |
| Header / surface | oklch(0.994 0 89.9) (approx #fcfcfc) |
| Composer surface | oklch(0.994) (approx #fcfcfc), on the page canvas |
| Primary text | oklch(0.097 0 0) (approx #0a0a0a; swatch neutral-950) |
| Muted text (docs body) | rgb(64,64,70) (approx #404046) |
| Muted text (docs meta) | rgb(96,96,102) (approx #606066) |
| Trip-planner subtitle | rgb(95,99,105) (approx #5f6369) |
| Docs heading | rgb(24,25,30) (approx #18191e) |
| Docs border | rgb(239,240,245) (approx #eff0f5) |
| Docs background (`--color-fd-background`) | #f5f5f5 |
| Docs primary (`--color-fd-primary`) | #171717 |
| Button primary (playground Generate) | oklab(0.546 -0.027 -0.213) (approx #2b7d96, cool blue-cyan; approx) |
| Button primary foreground | oklch(0.994) |
| Illustration canvas (IllustrationUi light) | #ffffff, ink #0d0d0d |
| Swatches (neutral) | 25 #fdfdfd, 50 #fafafa, 100 #f5f5f5, 200 #e5e5e5, 300 #d4d4d4, 400 #a3a3a3, 500 #737373, 600 #525252, 700 #404040, 800 #262626, 900 #171717, 925 #101010, 950 #0a0a0a, 1000 #030303 |
| Swatches (gray, blue-tinted) | 25 #fcfcfd, 50 #f9fafb, 100 #f3f4f6, 200 #e5e7eb, 300 #d1d5db, 400 #9ca3af, 500 #6b7280, 600 #4b5563, 700 #374151, 800 #1f2937, 900 #111827, 950 #030712 |
| Status swatches | blue-500 #3b82f6, blue-600 #2563eb, green-600 #16a34a, yellow-600 #ca8a06, red-600 #dc2626 |
| Showcase tints | F1 panel: pale green (#e8f3ef-ish, guess); Travel panel: pale pink (guess); IKEA panel: pale grey-beige (guess). Dither shader overlays |

#### Dark (intelligent-ui demo, dark docs, IllustrationUi dark)

| Role | Value |
|---|---|
| Canvas (`--iui-canvas`) | #212121 |
| Surface (`--iui-surface`) | #2a2a2a |
| Surface hover | #333333 |
| Surface strong | #3a3a3a |
| Composer surface (measured) | oklch(0.229 0 …) (approx #2c2c2c) |
| Ink (`--iui-ink`) | #ececec |
| Ink hover | #d4d4d4 |
| On ink (text on ink) | #171717 |
| Muted (`--iui-muted`) | #ababab |
| Faint (`--iui-faint`) | #8c8c8c |
| Range track / thumb | #3a3a3a / #ececec |
| Focus (`--iui-focus`) | #4ea2ff |
| Accent | #3ccfa6 (teal). Accent soft #3ccfa61f, accent ring #3ccfa666 |
| Info | #7cb8ff, soft #16273b |
| Success | #5bcf9c, soft #13291f |
| Warning | #f0b84f, soft #30250f |
| Danger | #ff7d74, soft #3a1b19 |
| Purple | #b49cff, soft #261f3a |
| Pink | #f28cc0, soft #371b2a |
| Selected | #1f2a33 |
| Chart 1 to 6 | #3d9dff, #22bf91, #a07cff, #f6ae55, #ec84a3, #5fb3bd |
| Map water | #1d3540 |
| Photo placeholder | #26302f, icon #4a5a60 |
| Docs dark (`--color-fd-*`) | background #121212, foreground #ebebeb, card #191919, card foreground #fafafa, popover #1e1e1e, muted #292929, muted foreground #b8b8b8, secondary #2e2e2e, accent #686868 at 30%, border #666 at 20%, ring #8c8c8c, primary #fafafa, primary foreground #171717 |
| Sidebar (dark demo) | approx #151515 (guess, read by eye) |

Dark and light are switched with `[data-theme="dark"]` and `.dark` selectors. The IllustrationUi component defines both: light canvas #fff and ink #0d0d0d, dark canvas #212121 and ink #ececec.

## 4. Per-component anatomy and measurements

### 4.1 Chat composer (light, cloud chat empty state)
- Shape: full-width pill, 728 x 56px, radius 28px, padding `11px 12px 11px 24px`, gap 12px.
- Surface: oklch(0.994), with a 0.5px ring at 8% and a soft shadow `lab(0 0 0 / .06) 0 4px 16px`.
- Input: textarea 646 x 32px, Inter 15/20, padding `6px 0`, no border, transparent.
- Send: circular black icon button at the right (approx 36px, guess).
- Hierarchy: the composer is the single dominant element on the empty screen. The heading ("Good to see you / What's on your mind today?") sits above it.

### 4.2 Suggestion tiles (conversation starters)
- Light: 136 x 72px, radius 16px, padding 16px, gap 12px, bg `oklch(0.097 0 0 / 0.02)`, no border, text 14/20 Inter at 60% ink.
- Dark: 171 x 80px, radius 16px, padding 16px, gap 12px, bg `oklch(0.994 0 / 0.02)`, text at 60% ink, icon 16 x 16 at 60%.
- Layout: a grid of 4 (light, one row of five) or 4 x 2 (dark), with about 8px gaps. Labels are clipped to two lines in light mode, for example "Relive the FIFA World Cup...".

### 4.3 Model selector
- Pill, 12px text, padding `0 12px`, gap 6px, bg `oklch(0.097 0 0 / 0.04)`, radius 9999px. Includes a provider logo (Google "G" in the chat, "anthropic/…" text in the playground).

### 4.4 Playground prompt box
- Container 760 x 119px, bg oklch(0.994), border `1px` at 12% near-black, radius 14px, padding 14px, shadow as above.
- Textarea 758 x 64px, padding `16px 16px 0`, 15/22.5.
- Model select at bottom-left, Generate button bottom-right: 105 x 33px, radius 8px, padding `7px 14px`, 13/19.5 weight 500, bg cool blue-cyan (approx #2b7d96) with light text. The button is disabled (lower opacity) when empty.
- Chips under the box: 150 x 33px, radius 20px, padding `6px 14px`, 13/19.5, text 60%, border 1px at 6%, bg 2% tint.

### 4.5 Playground output panels
- Raw output pane: black (near #000) rounded panel (about 288 x 252px in the 800px capture), tabs "RAW OUTPUT" and "PARSED JSON" as small segmented control at top, with mono code.
- Preview pane: light bordered card with header "PREVIEW" (small caps label) and a fullscreen icon at right. Empty state: centred monitor icon and "Rendered UI will appear here" in muted grey.
- Hierarchy: two equal columns, code on the left, rendered result on the right. The raw pane is intentionally dark, so code reads as code.

### 4.6 Homepage showcase (LangShowcase)
- Outer panel radius 24px, background uses a faded dither shader (WebGL canvas, `FadedDither`) with a static `.webp` fallback.
- Tabs: a vertical column of narrow pill tabs (72px wide) on the right with mascot avatars (shiro-travel.svg, IKEA, F1 helmet) and rotated uppercase labels ("TRAVEL", "IKEA"). The active tab expands to 800px wide.
- Content card: 704 x 449px, white (`rgb(255,255,255)`), radius 18px, border `1px rgba(10,10,10,0.08)`, padding about 36 x 46px.
- The content is a pre-rendered GIF (for example `travel-kyoto-november.gif`), not live DOM. Styles inside the card could not be read.
- Hierarchy: a headline "Make your AI apps and agents respond with Dashboards" (grid icon inline), a "View docs" pill button with black circle arrow, then the showcase.

### 4.7 Button styles
- "View docs" (homepage CTA): white pill with 1px border, black circle arrow at right.
- "Build for free" (chat header): black pill, white text, arrow glyph.
- "Ask" (F1 demo): outlined grey pill in italic, uppercase, with a stacked-chevron icon.
- "Bevel" button (`BevelButton` component on the showcase tab): a chip with badge, used in the homepage showcase.
- Docs CTA "Explore the Gateway": 34px high, radius 8px, padding `6px 12px`, 14/20 weight 600, 1px border.

### 4.8 Chat header and sidebar
- Chat header: 53px tall, padding 8px, bg oklch(0.994); centred segmented control (Mobile / Tablet / Desktop) with active segment on a light grey pill; "Build for free" pill at right.
- Left rail (chat): 40px-wide, icon-only buttons 32 x 32, radius 7px, active state on a black circle (edit icon).
- Dark rail (intelligent UI): 32px icon buttons. History button uses bg `oklch(0.994 / 0.08)` and radius 9999px. Theme toggle is a sun/moon icon at the bottom.

### 4.9 Docs layout (Fumadocs-based, Tailwind)
- Top bar: logo left, centred search "Search... ⌘K" pill, theme toggle (sun/moon), discord, GitHub star count pill at right.
- Section tabs under header: Docs, Demos, Integrations, API Reference.
- Sidebar: 240px wide, grouped headings ("Core Concepts", "Language Spec") at 12px uppercase tone, links 15/21.4 weight 500. Bottom: three square-ish buttons (AI setup, Discord, Talk to us) at about 50px square.
- Content column: about 736px wide, H1 34px, lead paragraph 18/28, "Copy Markdown" and "Open" secondary buttons (small outlined, radius 6px).
- Callout: bordered box with radius 12px-ish (guess), 1px border `rgb(239,240,245)`, padding about 16px, containing a text line and an outlined CTA.
- Architecture diagram: boxes with 1px borders and rounded corners, one highlighted in pale blue.

### 4.10 F1 dashboard demo (themed, not default)
- Dark icon rail on the left (about 40px), red accent (#e10600-ish, guess) for active indicator and "+" button.
- Hero: eyebrow "NEXT RACE · ROUND 18" in red, 12px tracked uppercase. Big italic display type "UNITED STATES" (Titillium heavy italic). Countdown "04 DAYS / 03 HRS / 20 MIN" with Saira numerals.
- Tiles: flat off-white cards (bg about #f7f4f1, guess) with the top-right corner clipped at 45 degrees (cut-corner "tab" shape). Label 12px/700 tracked, time 22px-ish/700.
- Section headers: H2 26.6px/600 Saira, action buttons as black cut-corner rectangles with uppercase text.
- Chips under the hero: cut-corner rectangles, uppercase 12px/700.
- Sticky bottom search bar: off-white trapezoid with a mascot avatar and italic placeholder "RADIO IN YOUR QUESTION...".

### 4.11 Trip planner demo (themed, not default)
- Top centred segmented control: pills "Explore / Chat / Itinerary", active pill white with shadow, 999px radius, 14px label.
- Headline 64px serif, tight tracking (-2.24px), with a small eyebrow line above in 16px (about "It's 16° ☀ and clear in ...").
- Suggestion cards: white cards with a 3D-looking illustration at top (torii gate, Lisbon tram, cheap-week coin) and a serif caption. Cards have 20px radius, about 127 x 128px, slightly different heights to create a "tilted" rhythm; one card is faded (opacity reduced).
- Floating vertical toolbar at the left (plus and history icons) in a white pill with shadow.
- Input: full-width pill at the bottom, 754 x 56px, placeholder "Ask your travel agent anything...", send button in soft pink.

### 4.12 GitHub dashboard demo (input state)
- Header: GitHub-mark icon + "GitPulse" wordmark. Headline "Create an interactive GitHub dashboard for" with inline "@username" underline input.
- Below: "or select a developer" label and a stack of five grey pill rows with tiny avatars, each about 224px wide, 46px tall, radius 16px. These fade in at load (ghosted at first).

## 5. Streaming and loading visuals

Observed:
- **Chat cloud load**: "Loading OpenUI Cloud..." centred with a small spinner on a near-white page, about 3-4 seconds before the chat shell appears.
- **Intelligent UI demo load**: dark canvas with a small pulsing grey mascot in the centre for about 5 seconds, then the welcome screen fades in (heading, composer, tiles).
- **GitHub demo load**: suggestion pills start at reduced opacity and resolve to full opacity. This reads as a staggered fade-in (observed; the animation curve was not measured).
- **Playground empty state**: icon plus caption ("Generated code will appear here", "Rendered UI will appear here"). No generation was run, so the streaming state of the preview pane was not observed.
- **Docs streaming comparison**: the OpenUI Lang page describes a side-by-side at 60 tokens/s: JSON finishes in 14.2s (849 tokens), OpenUI Lang in 4.9s (294 tokens). It is stated in text; no animation was recorded.
- **Homepage showcase**: answers are pre-recorded GIFs (for example `travel-kyoto-november.gif`), which is the closest thing to a streaming capture on the site. The GIF frames were not extracted.

Not verified: the streaming render itself (lines appearing one at a time, skeleton placeholders for unfinished components). This needs a real generation run.

## 6. Strengths and weaknesses

### Strengths
- **Clear token layers.** Swatch to semantic to component values make theming mechanical. The dark and light semantic sets mirror each other.
- **Alpha-driven surfaces.** Tints and borders are alpha overlays on the canvas, so dark and light stay consistent and the palette stays small.
- **Restrained neutral palette.** Almost everything is chroma-0. Brand accent (teal #3ccfa6 in dark, blue focus #4ea2ff) appears sparingly.
- **Soft, layered elevation** (two-layer shadows and 0.5px rings) reads as modern and crisp without heavy borders.
- **Clear input hierarchy.** The composer pill is the strongest element on empty chat states; suggestion tiles sit below and recede.
- **Themeable demos.** F1 and trip-planner show the system adapting to very different visual brands, which is a useful proof point.
- **Chart palette.** Six dark-mode chart colours are distinct and hue-spaced, and they sit beside the accent without clashing.

### Weaknesses
- **Token naming is fragmented.** Layers use four prefixes for near-identical roles (`--color-fd-*`, `--iui-*`, `--sc-*`, `--faq-*`, `--ps-*`, plus `--swatch-*`). Some values are duplicated (for example `--iui-border` and `--border-default`).
- **Mixed colour spaces.** The site mixes oklch, oklab, lab and hex in the same layer. Fine for rendering, but harder to reason about and to port.
- **Non-grid spacing.** The space scale steps by 2, 4, 6, 8, 10, 12, 16, 20, 24, 36, 48. Some values (10px, 6px) sit off the 4px grid that most components use.
- **Component showcase is mostly images.** The homepage answers are GIFs, so the default component anatomy is hard to inspect on the public site. The playground needs a model run to show components.
- **Demos hide the default system.** The F1 and trip-planner demos override almost everything, so a visitor cannot see the default look without running the playground.
- **Light-mode demo state was unstable.** After toggling the Intelligent UI demo to light, the welcome content did not re-render in the capture. This may be a transient state or a bug; it was not confirmed.
- **Inter fallback inconsistency.** The site uses Inter, but the demos use Titillium, Saira and serifs, and the trip planner falls back to Arial on body. Heavy display faces may be hard to scan at small sizes (F1 micro labels at 12px with 1.4px tracking).

## 7. Limitations of this study
- The playground and Gateway were not run, because generation needs a prompt to be sent. No form was submitted.
- Demo dashboards with data were not reached (the GitHub demo requires a username).
- Homepage showcase cards are GIFs. Their internal styles were not measured.
- Some hex values for surfaces were converted by eye from oklch/lab (marked approx).
- Light-mode token values for the chat shell beyond the ones listed were not fully captured.
- Thesys C1 and docs.thesys.dev were not studied. They are a separate commercial product.
