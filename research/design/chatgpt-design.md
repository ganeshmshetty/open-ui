# ChatGPT generative-UI widgets: measured design language

Source: live computed styles and screenshots of the user's own ChatGPT Plus account (dark theme only), captured 2026-10-21 via Chrome. Values are `getComputedStyle` readings unless marked "estimate". Chats used: Tokyo Weather Apple Stock, 2023 Population Bar Chart, Laptop Recommendation Form, Ramen And Premier League Scores (places map + scores), Markdown Demo, Five-minute timer (currency + timer). One new chat was created ("Write Contact Form") to get a widget with text inputs and a dropdown. No widget buttons were clicked (the timer's "Open" and the scores' "Send me results" were left alone).

Scope limits:
- Light mode was NOT captured. Switching theme needs account settings, which were not touched. Everything below is dark mode only.
- No empty or error state was provoked.
- Hover was tested on one control only (a score row): no visible change.
- Keyframe bodies were unreadable (cross-origin stylesheet). Streaming timing is partly estimated.

Note on colors: all values are rendered sRGB. Some values came back as oklab() and were converted by hand, so they are approximate.

---

## 1. Overall principles (inferred)

1. **Neutral surfaces, hairline structure.** The widget sits directly on the page background (#000). Cards are separated by a 0.9px border at 15% white and a 16–20px radius. Section breaks and table rows use 5% white hairlines. There are almost no drop shadows. The only shadow is a soft pin shadow on map markers.
2. **Color is semantic, not decorative.** Text is near-white, secondary text is light gray, and tertiary text is mid gray. Color appears only where it carries meaning: red for a negative change, green for timer state, a deep blue for the user's own message. Charts use a single blue hue for every bar.
3. **Numbers lead.** The stock price, temperature and currency amounts are the largest type on the card (24px semibold, or 18px semibold in lists). Labels are small and muted and sit above or beside the value. Units and qualifiers are a smaller, lighter span next to the number (e.g. "26°C" then a muted "(78°F)").
4. **Eyebrow, value, meta.** Almost every card follows the same pattern: a 12px muted eyebrow line ("Latest confirmed Nasdaq close · October 20"), a big value, then a small meta line or grid of label/value pairs, then a 14px muted source line with citation chips.
5. **Fixed measure and rhythm.** Widgets fill the 768px content column. Body text is 16px on a 26px line (1.625). Spacing is built from 4, 8, 12, 16 and 20px steps.
6. **Forms are native-feeling but restrained.** Radio and checkbox controls are 20px circles. Inputs are pill-shaped with a 1px inset ring and no fill. The primary action is an inverse white pill.
7. **Citations are quiet.** A source is a small dark-gray pill (#414141) with a favicon, a name and a "+N" count. It sits inline after the sentence it supports and never breaks the text flow.
8. **Density is moderate, not dense.** Grid cells are 8px apart and rows are 8px padded. Vertical rhythm is generous between blocks (16px) and tight inside a block (2–8px).

---

## 2. Token tables

### 2a. Typography (font family for all widgets: system UI, resolved as `-apple-system-body, ui-sans-serif, -apple-system, system-ui, …`; on macOS this is the system font)

Monospace: `ui-monospace` (code blocks and inline code).

| Role | Size | Weight | Line height | Letter spacing | Where seen |
|---|---|---|---|---|---|
| Hero number | 24px | 600 | 32px | normal | Stock price "$339.50 USD", weather temp "26°C" |
| Section title (widget) | 18px | 600 | 28px | normal | Currency "USD 250 converted", timer "Timer started", markdown H2 |
| Chart / section title | 16px | 600 | 24px | normal | Chart title "Population by Country in 2023" |
| Body | 16px | 400 | 26px | normal | Paragraphs, descriptions, option labels |
| Emphasis / value | 16px | 600 | 26px | normal | Grid values, condition "Mostly clear", row names, bolded team names |
| Interactive label | 16px | 500 | 26px | normal | Change row "−$3.44 (−1.00%)" |
| Table header | 14px | 600 | 20px | normal | Table `th` |
| Table cell / label | 14px | 400 | 20px | normal | Table `td`, team names, timer name |
| Secondary body / hint | 14px | 400 | 18.2px (1.3) | normal | Source lines, helper text ("Select all that apply.") |
| Field label | 14px | 500 | 20px | normal | Contact form labels |
| Button | 14px | 500 | 14px | normal | CTA "Find my laptop", "Send" (line height equals size) |
| Eyebrow / meta | 12px | 500 | 16px | normal | "Latest confirmed Nasdaq close · October 20", "Tokyo, Japan · 11:08 PM JST" |
| Meta line | 12px | 400 | 16px | normal | "About 1–2 minutes from Exit B4", rate "1 USD ≈ €0.8907" |
| Status | 12px | 600 | 16px | normal | "Full-time" |
| Small label | 10px | 400 | 12px | normal | Grid labels "Today's high", "52-week range" |
| Chip text | 12px | 400 | 15.6px | 0.3px | Citation chip label and favicon row |
| Chip count / micro | 9px | 400 | 9px | normal | Citation "+1" count; chip name inside the chip |
| Map label | 11px | 500 | 13px | 0.06px | Map pin neighborhood labels |
| Code block | 12px | 400 | 20px | normal | Code block (`ui-monospace`) |
| Inline code | 14.88px | 500 | 26px | normal | Inline `code` in prose |
| Map pin rating | 12px | 500 | 12px | normal | Pin "4.9" |

Type scale in use: 9, 10, 11, 12, 14, 14.88, 16, 18, 20, 24. Weights in use: 400, 500, 600. Nothing above 24px appears in any widget.

### 2b. Spacing values observed

- Widget outer padding: 12px (weather, stock, places-related card rows), 16px (form card, contact form card), 20px (chart section top padding).
- Gap between header and grid inside a card: 8px (weather, stock).
- Gap between header blocks (eyebrow / value / change): 1px to 4px (weather header gap 1px; stock header gap 4px).
- Gap between the two stacked lines of a change row: 4px.
- Divider: 8px margin top and bottom, 1px hairline.
- Grid cells: 8px column and row gap; each cell is a stacked label (10px) and value (16px/600).
- Form: 16px gap between questions; 12px gap between contact-form fields.
- Radio/checkbox label: 10px gap between control and text. Option rows are 26px tall.
- Table cell padding: 8px vertical; 8px horizontal on one side (left cell pads right 8px and vice versa).
- Paragraph spacing in prose: 8px top margin; H3 has a 24px top margin.
- List: ul left padding 32px.
- Citation chip padding: 4px 6px 4px 4px; internal gap 4px (favicon to name) and 3px (name to count).
- Pin padding: 0 8px. Place-card text stack: title, rating, meta, description.

### 2c. Radii

| Radius | Used for |
|---|---|
| 8px | Textarea wrapper, row buttons (score rows), inline code tags use ~4px |
| 9999px (pill) | Citation chips, radio/checkbox, text inputs, CTA buttons, map pins, badges, "Send me results" secondary button |
| 16px | Weather/stock card, timer resource card, place thumbnails (132px), resource card |
| 20px | Form card (laptop and contact form) |
| 22px | User message bubble |
| 24px | Code block container |
| 28px | Chat composer |

### 2d. Borders and dividers

- Card border: 0.909px solid rgba(255,255,255,0.15). This is the single most-used border (weather, stock, form, contact form, pins).
- Divider and table-row rule: 0.909px rgba(255,255,255,0.05).
- Input and textarea ring: no border; an inset box-shadow of 0 0 0 1px rgba(255,255,255,0.2).
- Checkbox unchecked border: 0.909px rgb(93,93,93). Checked: fill rgb(237,237,237) with the same border.
- Code block border: 0.909px (color not captured).

### 2e. Shadows

- Almost none. Inputs and the composer use an inset 1px ring (rgba(255,255,255,0.2)).
- Map pin: 0 8px 16px -4px rgba(0,0,0,0.12).
- The chat composer also uses an inset 1px ring at 20% white.

### 2f. Color palette (dark mode, observed)

| Role | Value | Notes |
|---|---|---|
| Page background / sidebar | rgb(0,0,0) | Widgets have no background of their own |
| Text primary | rgb(237,237,237) (#ededed) | Titles, values, body |
| Text secondary | rgb(205,205,205) (#cdcdcd) | Eyebrows, labels, meta, sources, chart axis labels |
| Text tertiary / muted | rgb(175,175,175) (#afafaf) | Input placeholder, disabled button text |
| Text on light button | rgb(13,13,13) (#0d0d0d) | CTA label on white pill |
| Surface raised | rgb(48,48,48) (#303030) | Code block, map pin, timer resource card (≈ at 50% alpha on black) |
| Surface chip | rgb(65,65,65) (#414141) | Citation chip background |
| Surface composer | rgb(27,27,27) (#1b1b1b) | Chat input bar |
| Surface user bubble | rgb(23,62,118) (#173e76) | User message background; text rgb(246,250,254) |
| Inverse primary (CTA) | rgb(237,237,237) | Pill button fill (via ::before) |
| Border | rgba(255,255,255,0.15) | Card border |
| Divider | rgba(255,255,255,0.05) | Rules and table rows |
| Input ring | rgba(255,255,255,0.20) | Inset 1px |
| Accent blue (chart) | #1f4e94 at 100% to 90% alpha | Bar fill gradient |
| Negative / down | rgb(255,103,100) (#ff6764) | Stock decline, change text and arrow |
| Positive / success | green (not measured; looks like ≈ #3ddc84-ish, estimate) | Timer icon and check mark |
| Warning | not observed | |
| Danger | rgb(255,103,100) (same as negative; estimate that errors reuse it) | |
| Code keyword | rgb(248,166,200) (#f8a6c8) | Syntax highlight |

Role mapping summary: text primary `#ededed`, secondary `#cdcdcd`, tertiary `#afafaf`, surface `#000`, raised surface `#303030`, border `rgba(255,255,255,.15)`, accent (interactive fill) `#ededed` with dark text, danger/down `#ff6764`, success green (estimate).

### 2g. Light mode

Not captured (see scope limits). Do not assume inversions of the values above.

---

## 3. Per-widget anatomy and measurements

Widths are measured in CSS px. The content column is 768px wide.

### 3.1 Weather card (Tokyo)

Root: 768×207. Border 0.909px rgba(255,255,255,0.15), radius 16px, padding 12px, gap 8px.

1. Header block (742×76, gap 1px):
   - Eyebrow: "Tokyo, Japan · 11:08 PM JST", 12px/500/16px, rgb(205,205,205).
   - Hero: "26°C", 24px/600/32px, rgb(237,237,237). Unit "(78°F)" is a span at 16px/600/26px, rgb(205,205,205).
   - Condition: "Mostly clear", 16px/600/26px, rgb(237,237,237).
   - Icon: 32×32 SVG, rgb(205,205,205), right side.
2. Divider: 8px top and bottom margin, 1px rgba(255,255,255,0.05).
3. Grid of three cells (742×38, gap 8px). Each cell is 242px wide:
   - Label: "Today's high" etc., 10px/400/12px, rgb(205,205,205).
   - Value: 16px/400/26px, with a span "29°C" at 16px/600/26px, rgb(237,237,237).
4. Alert row (742×26, gap 8px): 16px icon in rgb(205,205,205), then text 12px/400/16px, rgb(237,237,237): "Gale advisory in effect until midnight JST, issued by the Japan Meteorological Agency."
5. Prose line below card: 16px body.

Layout: left-aligned, one header, a three-up grid, a single warning line. Hierarchy is clear: temperature first, then condition, then grid. Icons are thin, monochrome, 32px for the hero glyph and 16px for inline glyphs.

### 3.2 Stock card (AAPL)

Root: 768×223. Border 0.909px rgba(255,255,255,0.15), radius 16px, padding 12px, gap 8px.

1. Header (742×82, gap 4px):
   - Eyebrow: "Latest confirmed Nasdaq close · October 20", 12px/500/16px, rgb(205,205,205).
   - Price: "$339.50 USD", 24px/600/32px, rgb(237,237,237).
   - Change row (gap 4px): trend SVG 16×16 in rgb(255,103,100), then "−$3.44 (−1.00%)" 16px/500/26px, rgb(255,103,100).
   - Logo: 32px Apple glyph at right, rgb(205,205,205).
2. Divider (8px margins).
3. Two-column grid (742×38, gap 8px). Each cell is 367px:
   - Label: "Oct. 21 premarket quote" / "52-week range", 10px/400/12px, rgb(205,205,205).
   - Value: "$338.12" / "$243.42–$348.15", 16px/400/26px with a 16px/600/26px span.
4. Source line: 14px/400/18.2px, rgb(205,205,205), then the inline citation chip.
   - Chip: 100×20, background rgb(65,65,65), radius 9999px, padding 4px 6px 4px 4px, gap 4px.
   - Favicon: 12×12 circular image.
   - Text stack (gap 3px): "Investing.com" 9px/400/9px rgb(205,205,205) and count "+1" 9px/400/9px rgb(175,175,175).

Notes: the down state uses the red accent for both the arrow and the text, but the price is not colored. The timestamp and premarket context are in the grid, not in the header. The source line carries an honest caveat ("I couldn't verify a newer live Nasdaq trading price") in the same muted style as the source. Prose after the card is 16px body with inline citation chips after sentences.

### 3.3 Bar chart (2023 population)

Root: a `section` of 768×432 containing a recharts SVG (752×340, viewBox 0 0 752 340).

1. Chart title: "Population by Country in 2023", 16px/600/24px, rgb(237,237,237). Container 672 wide, padding top 20px and 96px bottom on the parent wrapper.
2. Recharts bars: five paths 117px wide, each with fill `url(#fill-…)`, a linear gradient from #1f4e94 (100% alpha) at 0% to #1f4e94 (90% alpha) at 100%. Top corners are rounded at 8px (drawn via the path arc).
3. Gridlines: horizontal, stroke rgba(255,255,255,0.05), 1px, dash 3px 3px.
4. Axis labels: "0 million", "400 million", "800 million", "1.2K million", "1.6K million" and category labels "India", "China", etc. Font 12px/400, rgb(205,205,205), system font.
5. Data table below the chart (768×221): see 3.4.
6. Conclusion paragraph: 16px body, "Conclusion:" as 16px/600.
7. Source line: 14px/400/18.2px rgb(205,205,205), then citation chip "Nations Inq… +1" (same chip style as above).

Notes: the chart uses one hue for every bar, with no legend because there is only one series. Values are labelled in the table, not on the bars. Axis text is muted, grid is nearly invisible. The chart and table together make the numbers exact, with the chart giving shape.

### 3.4 Data table (inside chart answer)

Table 768×221, system font.

- Header cell (`th`): 14px/600/20px, rgb(237,237,237), padding 8px 8px 8px 0 (left column) and 8px 0 8px 8px (right column). Right column header is right-aligned.
- Body cell (`td`): 14px/400/20px, same padding. Numeric column is right-aligned.
- Row: height 37px, border-bottom 0.909px rgba(255,255,255,0.05). No zebra striping.
- Table has no outer border and no background.

### 3.5 Form: laptop recommendation (single widget with radio, checkbox and CTA)

Root card: 768×804, border 0.909px rgba(255,255,255,0.15), radius 20px, padding 16px, gap 16px (form child is flex column, gap 16px).

Each question block:
- Question: "1. What's your budget?", 16px/600/26px, rgb(237,237,237), with a 16px leading icon for question 2 and 3.
- Helper text: 14px/400/18.2px rgb(205,205,205) (e.g. "Select all that apply.", "Prices in INR. Choose your maximum comfortable range.").
- Options: label 16px/400/26px rgb(237,237,237), gap 10px between control and text. Each row is 26px high.
  - Radio: 20×20, pill-shaped, inset 1px ring rgba(255,255,255,0.2). Selected state shows a small inner dot.
  - Checkbox: 20×20 `button` with role checkbox. Unchecked: border 0.909px rgb(93,93,93), transparent. Checked: fill rgb(237,237,237) and border rgb(237,237,237), with a dark check glyph.
  - Option icons (for the "Main use" question): small 16px line icons, e.g. book, code, gamepad, film, briefcase.
- Section divider between questions: 8px margins, rgba(255,255,255,0.05).
- CTA: "Find my laptop →", a full-width pill (734×36). Pill background is rgb(237,237,237), drawn via `::before`. Label 14px/500/14px, color rgb(13,13,13), padding 0 16px, gap 6px, arrow icon 18px.

Observed density: five question blocks worth of content fit in ~800px. Scanning is easy because the question titles are the only semibold text.

### 3.6 Form: contact (generated in the new test chat)

Root card: 480×564, border 0.909px rgba(255,255,255,0.15), radius 20px, padding 16px. This card is narrower than the 768px column. Widgets are not forced to full width.

- Title "Contact Us": estimate ~18px/600 (not measured; helper failed).
- Subtitle "Fill out the form below to get in touch.": estimate 14px/400, rgb(205,205,205).
- Field label: 14px/500/20px, rgb(205,205,205) ("Name", "Email", "Country", "Message").
- Text input: 446×40. Pill (radius 9999px). Padding 0 18.62px. No border. Inset 1px ring rgba(255,255,255,0.2). Font 16px/400/26px, text rgb(237,237,237). Placeholder rgb(175,175,175).
- Dropdown (Country): visually the same pill as the input with a right chevron and the selected value "India". Computed style not captured (no `select` element; likely a custom `combobox`).
- Textarea: 446×120. Wrapper radius 8px (not pill; a different shape from the input). Inset 1px ring rgba(255,255,255,0.2). Textarea padding 10px 14px, 16px/400/20px font, placeholder rgb(175,175,175).
- Send button: 446×36, disabled (`disabled=true`, `aria-disabled=true`). Label 14px/500/14px rgb(175,175,175). Fill ~ rgba(237,237,237,0.05) via `::before`. Icon: paper plane. It becomes enabled after input (not tested).
- Vertical gap between fields: 12px.

Note: the input is pill-shaped and the textarea is a rounded rectangle (8px). Those two shapes sit in the same column.

### 3.7 Places with map (ramen near Shibuya)

Structure: intro prose (16px), then a map, then a numbered list of place cards.

Map:
- Container: 768×400, Mapbox dark style. Border radius 0 (square corners). Chrome: a fullscreen button at top right (round, dark). Hint text "Use two fingers to move the map" and "Use ⌘ + scroll to zoom the map" appear on first interaction.
- Pins (custom markers): a `button` 49×28, background rgb(48,48,48), radius 9999px, border 0.909px rgba(255,255,255,0.15), padding 0 8px, shadow 0 8px 16px -4px rgba(0,0,0,0.12). Content: 12px star icon and "4.9" 12px/500/12px rgb(237,237,237). Ratings appear as pins on the map (not names).
- Neighborhood labels (on the basemap, not from the widget): 11px/500/13px, letter-spacing 0.06px, rgb(237,237,237), padding 2px 4px.
- Transit / other map icons are Mapbox defaults, not restyled.

Place card list:
- Thumbnail: 132×132, radius 16px, object-fit cover. Loads lazily; before load it is an image of 0×0 with class "LoadingImage" (skeleton-like).
- Title: "Ramen Kamo to Negi Shibuya Miyamasuzaka Branch", 16px/600/26px, rgb(237,237,237). The title is a link (underlined on hover, not verified).
- Rating row: star 12px + "4.7" 12px/500/12px rgb(237,237,237), then "(2,149)" 16px/400/26px rgb(205,205,205) (inconsistent with the 12px rating; the review count is larger text).
- Meta: "About 1–2 minutes from Exit B4", 12px/400/16px rgb(205,205,205).
- Description: 16px/400/26px rgb(237,237,237), up to two lines.

Notes: the map is the largest visual element, so the list is secondary. Ratings appear both on the map and in the list, which connects the two. The number "1." prefix is plain text, not a badge.

### 3.8 Sports scores (Premier League)

Structure: score rows, then a "Weekend highlights" H3, a bullet list, a link, and a secondary pill.

Score row:
- Row is a `button` 768×92, radius 8px, padding 12px, background transparent. Cursor pointer, `transition: all`.
- Inside: home team (crest 36×36, `object-fit: contain`, team name 14px/400/20px below); score 20px/400/28px (regular weight, not bold); centre stack: "Full-time" 12px/600/16px and date "Oct 17" 12px/400/16px rgb(205,205,205); away team mirrored.
- Dividers between rows: rgba(255,255,255,0.05).
- Hover: no visible background change on hover (tested once; computed background stayed transparent). The hover state is not evident.

Highlights:
- H3 "Weekend highlights": 16px/600/26px, margin top 24px.
- Bullets: 16px/400/26px. Team names are bold (16px/600). The bullet list has ul padding-left 32px and a `disc` marker.
- Source citation chip "RTE Soccer +2" after the last bullet.
- Secondary action: outlined pill "Send me Premier League results after matchdays" (not clicked). Pill with clock icon, rgba border, 16px text.

### 3.9 Currency conversion

Root: rows and a footer disclaimer, plus a timer block below. Chat-level layout.

Heading: "USD 250 converted" 18px/600/28px.
Sub: "Using indicative exchange rates for October 21, 2026:" 16px/400/26px.

Rows (one per currency):
- Flag: emoji 20px/600/28px in a 20–24px box (e.g. 🇪🇺, 🇯🇵, 🇮🇳).
- Name: "Euro (EUR)" 16px/600/26px rgb(237,237,237).
- Rate: "1 USD ≈ €0.8907" 12px/400/16px rgb(205,205,205).
- Amount: "€222.68" 18px/600/28px, right-aligned, rgb(237,237,237). Amounts use the currency symbol and grouping ("¥39,523", "₹23,679.18").
- Row height ~42px; gap between rows ~12px; no per-row background.

Disclaimer: "Indicative market rates; bank or exchange fees may change the final amounts." 14px/400/18.2px rgb(205,205,205), then citation chip "Currency Pr… +1" (9px chip text).

### 3.10 Timer

Heading: "Your 5-minute timer" 18px/600/28px (section title).
Status block:
- Icon: alarm clock glyph in green (estimate).
- "Timer started" 18px/600/28px with a green check-circle icon (estimate).
- Body: "You'll receive an alert in 5 minutes, at approximately 7:46 PM IST." 16px/400/26px rgb(205,205,205) (secondary).
- Completed line: "⏰ Time's up! Your 5-minute timer has finished." 16px/600/26px rgb(237,237,237). The clock emoji is inline.

Resource card (an entity card linking to the timer):
- Container: 768×70, radius 16px, background rgba(48,48,48,0.5) (estimate from oklab L 0.309 at alpha 0.5), no border.
- Icon: clock glyph in a rounded square (~ 40px), rgb(237,237,237).
- Name: "Five-minute timer" 14px/400/20px, rgba(237,237,237,0.85).
- Status: "Completed" (secondary line, 12px/400 rgb(205,205,205), estimate).
- Trailing "Open" pill button (not clicked). Outlined, pill.

### 3.11 Markdown and prose (Markdown Demo)

Prose:
- Body: 16px/400/26px rgb(237,237,237). Paragraph margin-top 8px.
- H2 ("Programming Languages Demo"): 18px/600/28px. Markdown H2 is smaller than body-large; hierarchy comes from weight and spacing, not size.
- H3 ("Language Comparison"): 16px/600/26px, margin top 24px.
- Lists: ul padding-left 32px, `disc` marker, item text 16px/400/26px, bold lead-in words (e.g. "Rust –" bold).
- Inline code: 14.88px/500/26px, rgb(237,237,237), background color(srgb 0.129 / 0.82) ≈ #212121 at 82% alpha, radius ~3.7px, padding ~0.74px 4.2px.

Table:
- 768×147. Header th 14px/600/20px, padding 8px 8px 8px 0. Body td 14px/400/20px. Row border-bottom 0.909px rgba(255,255,255,0.05). Columns left aligned.

Code block:
- Container: 768×122, background rgb(48,48,48), radius 24px, border 0.909px. Header row with the language name (e.g. "Rust", 14px-ish, estimate) and icons on the right: a "wrap" toggle and a copy button (both 16px line icons).
- Code: `ui-monospace`, 12px/400/20px, rgb(237,237,237). Padding 0 20px 12px. Line text is 12px inside a 20px line height.
- Syntax colors: keyword `fn` rgb(248,166,200) (pink); function name `main` blue-ish (estimate); string green-ish (estimate).

Blockquote: not present in this chat; not measured.

### 3.12 Shared chat chrome (for context)

- User bubble: background rgb(23,62,118), text rgb(246,250,254), 16px/400/24px, radius 22px, padding 10px 16px, right-aligned, max width ~ 536px (estimate).
- Assistant text: 16px/400/26px rgb(237,237,237), no bubble.
- Action row under each answer: icons 16px (copy, thumbs up, share, read aloud, regenerate, more), in rgb(205,205,205).
- Composer: 768×52, background rgb(27,27,27), radius 28px, inset ring rgba(255,255,255,0.2). Placeholder "Ask ChatGPT". Right: "Medium" model picker (text + chevron) and a mic and send button.
- Sidebar: background #000, 16px item text. Chat titles are truncated with ellipsis.

---

## 4. Interaction and streaming states

- **Streaming.** While a response is in progress, the status line reads "ChatGPT is responding" and the composer shows a waveform/voice button in place of send. Text appears with a per-word fade-in: an animation named `d-word-fade-in`, duration 0.7s, timing `ease`, fill mode `backwards`. Words fade in as they arrive (inferred from the animation name and timing; the keyframes themselves were not readable).
- **Loading media.** Place thumbnails render as `LoadingImage` elements at 0×0 until loaded (a placeholder with no visible skeleton captured). The map loads as a Mapbox canvas with no skeleton.
- **Skeletons and shimmer.** No shimmer or skeleton blocks were observed on finished widgets. Not captured during streaming (the chat finished too fast or the animation was in the word-fade only).
- **Hover.** Score rows: no visible background change (computed background stayed transparent). The map pin and the citation chip hover state were not captured. The Send button hover state was not captured.
- **Focus.** Not captured. The input and textarea rings are inset shadows; focus likely changes the ring (estimate, not verified).
- **Disabled.** Contact-form Send: `disabled` attribute set. Text becomes rgb(175,175,175) (tertiary), and the pill fill drops to rgba(237,237,237,0.05). This is the only explicit disabled state captured.
- **Selected.** Radio: selected shows an inner dot (on the 14-inch option). Checkbox: checked fills the circle with rgb(237,237,237) and a dark check glyph.
- **Error / empty.** Not provoked. The weather card contains a warning row with an icon, which is the closest equivalent (advisory, not an error).
- **Timer state.** Timer shows "Timer started" with a green check, then "Time's up!" with a clock emoji and a "Completed" status on the resource card. Green signals success, not danger.
- **Citation hover / expansion.** Citation chips are static pills. Hovering them was not tested.

---

## 5. What looks best and why; what to avoid

### Best (keep)

1. **Hairline cards on black.** A 15% white border with a 16–20px radius gives structure without weight or shadow. It works on dark and reads as "a card" without looking heavy.
2. **One hero number per card.** The price, temperature or converted amount is the largest element. Everything else is smaller and muted. This makes the card scannable in under a second.
3. **Eyebrow, value, meta pattern.** The 12px eyebrow sets context ("Latest confirmed Nasdaq close · October 20"), the 24px value carries the answer, and the meta line adds caveats. The same skeleton works for weather, stocks and currency.
4. **Semantic color only.** Red appears only on a decline. Green appears only on a timer check. The blue bubble is the user's, and the chart is one hue. Nothing else is colored. This keeps color meaningful.
5. **Tabular grids for paired facts.** The weather and stock grids use a 10px label over a 16px value, two or three columns, with 8px gaps. They are easy to scan and do not need a table.
6. **Forms that look like the product.** Radio, checkbox and text controls use the same 1px ring and muted label system as the chat composer. The CTA is an inverse white pill, the only high-contrast control. Disabled Send is clearly lower contrast.
7. **Citations as quiet pills.** A favicon, a name and a "+N" count in a 20px dark pill sit inline without competing with the text. The source line under each widget uses the same 14px muted style.
8. **Maps with ratings on the pins.** Pins show the rating, so the list and the map connect without extra labels. The list pairs a 132px thumbnail with title, rating, meta and description.
9. **Table hygiene.** Light hairline rules, 8px cell padding, 14px type, right-aligned numbers. No zebra, no outer border.

### Avoid (or be careful with)

1. **Multiple radii in one widget.** The contact form uses a pill for inputs and an 8px rounded rectangle for the textarea in the same column. Pick one radius family per form.
2. **Very small type.** 9px and 10px text (citation chip count, grid labels) is at the edge of legibility. Keep anything below 11px for decorative use only.
3. **Mixed weights for one value.** The review count "(2,149)" is 16px/400 next to a 12px/500 rating. The size jump is noticeable and reads as inconsistent. Keep rating and count on one size.
4. **Score weight.** Scores are 20px/400, not bold. This is deliberate (the team name carries emphasis) but makes the numbers less prominent than the team labels. Decide which of the two should lead.
5. **Hover that does nothing.** Score rows are clickable buttons with `cursor: pointer` and no hover change. Users get no affordance. Add a subtle fill (e.g. rgba(255,255,255,0.04)).
6. **Relying on color alone for state.** The stock decline uses red for both the arrow and the text, with a minus sign. That is good, but make sure the sign also conveys direction for colorblind users.
7. **Inline code size.** Inline code (14.88px) is larger than the 12px code block text. The two monospace styles should share a size scale.
8. **Map chrome noise.** Mapbox labels and transit icons compete with the pins. Consider a quieter basemap style when the map is secondary to the list.

---

## Appendix: measurements to reuse as design tokens (draft)

```
font.display.lg  24 / 32  600   (hero number)
font.title       18 / 28  600   (widget title, list amounts)
font.title.sm   16 / 24  600   (chart title)
font.body       16 / 26  400   (prose, descriptions)
font.body.strong 16 / 26 600
font.label      14 / 20  500   (field labels, table head 600)
font.caption    14 / 18.2 400  (source lines, helper text)
font.meta       12 / 16  400-500 (eyebrow, meta, status 600)
font.micro      10 / 12  400   (grid labels)
font.chip        9 / 9   400   (citation count)
font.code       12 / 20  mono  (code block)
font.code.inline 14.88 / 26 mono 500

color.text        #ededed
color.text.2     #cdcdcd
color.text.3     #afafaf
color.surface    #000000
color.surface.2  #1b1b1b  (composer)
color.surface.3  #303030  (code block, map pin)
color.surface.4  #414141  (citation chip)
color.user       #173e76  (user bubble), text #f6fafe
color.border     rgba(255,255,255,0.15)
color.divider    rgba(255,255,255,0.05)
color.ring       rgba(255,255,255,0.20)  (inset 1px)
color.down       #ff6764
color.accent.bar #1f4e94  (chart)
color.cta.fill   #ededed, text #0d0d0d
color.disabled   text #afafaf, fill rgba(237,237,237,0.05)

radius.chip/pill 9999
radius.row/textarea 8
radius.card      16
radius.form      20
radius.user      22
radius.code      24
radius.composer  28

space: 4, 8, 12, 16, 20, 24 (widget padding 12 or 16; grid gap 8; block gap 16)
max-width widget column 768
```

Estimates are marked in the text. Everything else was read from computed styles.
