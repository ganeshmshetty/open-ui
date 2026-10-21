# Data-dense UI: tables, KPI cards and charts

Research notes for the Phase 1 component set (Card, Table, TableColumn with align/format, Chart bar/line, Badge, Text, Heading, Grid, List). Sources are public docs and pages only. Each figure is tagged with where it came from. Items marked **UNVERIFIED** are either third-party claims, values I could not confirm on a primary page, or my own recommendations.

Retrieval date: 2026-10-21. Several Tremor, Carbon and Stripe pages returned 404 at the URLs I tried, so coverage is partial. Gaps are listed at the end.

---

## 1. Per source

### 1.1 Tremor (tremor.so)

Tremor is a React and Tailwind library. The values below come from its source snippets and docs pages. Its docs have changed across major versions, so check the version before copying any prop names.

**Card** (https://www.tremor.so/docs/ui/card, from the installation source of the Tremor Raw card):
- Classes: `relative w-full rounded-lg border p-6 text-left shadow-xs`.
- Padding 24px (`p-6`), radius 8px (`rounded-lg`), 1px border, shadow `xs`.
- Background `bg-white` / dark `#090E1A`. Border `gray-200` / dark `gray-900`.
- Confirmed from source. Our card (radius 12, padding 16) is slightly larger in radius and tighter in padding than Tremor.

**BarChart defaults** (https://www.tremor.so/docs/visualizations/bar-chart, read from the component's destructuring defaults):

| Prop | Default |
|---|---|
| `showXAxis` / `showYAxis` | `true` / `true` |
| `showGridLines` | `true` (horizontal and vertical unless the chart says otherwise; not confirmed which) |
| `yAxisWidth` | `56` px |
| `showLegend` | `true`, `legendPosition: "right"` |
| `showTooltip` | `true` |
| `layout` | `"horizontal"` (vertical bars) |
| `type` | `"default"` (also `"stacked"`, `"percent"`) |
| `intervalType` | `"equidistantPreserveStart"` |
| `tickGap` | `5` |
| `autoMinValue` | `false` (so the y-axis starts at 0 by default) |
| `allowDecimals` | `true` |
| Root `className` | `"h-80 w-full"` (320px height by default) |
| `valueFormatter` | `(v) => v.toString()` (no formatting by default) |

Tremor's own examples pass `valueFormatter` built on `Intl.NumberFormat('us')` with `yAxisWidth={48}`. Its tooltip animation is 100ms. Bars have no entrance animation (`isAnimationActive={false}`).

**Badge** (https://npm.tremor.so/docs/ui/badges, read from the docs page): **UNVERIFIED as Tremor's own styling.** The fetched page showed base classes `inline-flex items-center gap-x-1 whitespace-nowrap rounded-md px-2 py-1 text-xs font-medium ring-1 ring-inset`, and color variants with light `bg-{c}-50 text-{c}-900 ring-{c}-500/30` and dark `dark:bg-{c}-400/10 dark:text-{c}-400`. The tokens and structure look like Tailwind-style badges generally, so treat them as an example, not as a verified Tremor spec.
- Size: 12px text (`text-xs`), 8px horizontal and 4px vertical padding (`px-2 py-1`), 6px radius (`rounded-md`), weight 500.

**BadgeDelta** (https://www.tremor.so/docs/ui/badges, and the v3.18.0 API reference via tessl):
- `deltaType`: `increase | moderateIncrease | unchanged | moderateDecrease | decrease`.
- `isIncreasePositive` (default `true`). Set it to `false` for metrics where a rise is bad, such as cost or churn.
- Sizes `xs | sm | md | lg | xl`.
- Colors per delta type were **not** found. Do not assume green/red mapping from Tremor without checking source.

**KPI card pattern** (Tremor docs and tessl reference, v3.18.0): a `Card` containing a `Flex` row with the value and a `BadgeDelta`, plus a `Text` line below for the comparison period. The exact KPI markup page returned 404, so Tremor's KPI type scale is **not verified**.

**Numbers, from Tremor:** no compact formatting is built in. Formatting is done with `Intl.NumberFormat`.

---

### 1.2 IBM Carbon Design System

Carbon's data-table style page has concrete numbers. Its chart pages are mostly qualitative and are marked "work in progress". Where a Carbon page disagreed with an older version, I note it.

**Data table row heights** (https://carbondesignsystem.com/components/data-table/style, current docs):

| Size | Row height | Note |
|---|---|---|
| xs | 24px | Pairs with 24px toolbar (8px top and bottom margin) |
| sm | 32px | Pairs with 32px toolbar (4px margin) |
| md | 40px | **Default** |
| lg | 48px | Pairs with 48px toolbar (16px margin) |
| xl | 64px | Only when rows hold two lines of content |

- Carbon v10 docs list four sizes, compact 24, short 32, default 48, tall 64. Current docs use five sizes. Do not mix them.
- The header row must match the body row size.

**Table typography and spacing** (same page):
- Column header text: 14px, weight 600 (SemiBold).
- Row text: 14px, weight 400.
- Table title (above the table, not the header): 20px, weight 400.
- Header padding: 16px left and right.
- Minimum gap between columns: 16px.
- Sort icon: 8px padding.
- Text in rows and headers is vertically centered, except xl rows, which get 16px top padding.
- Zebra rows use the `$layer-accent` background token. This is an opt-in striping style.
- Column header states: hover `$layer-accent-hover`, focus a `$focus` border, active `$layer-accent-active`.

**Table usage rules** (https://carbondesignsystem.com/components/data-table/usage/):
- Column titles should be one or two words. If a title runs long, wrap it to two lines, then truncate. Show the full title in a tooltip.
- Only the sorted column shows an arrow. Unsorted columns show an icon only on hover.
- Columns sort ascending or descending.
- For extended load times use skeleton rows, not spinners.
- Toolbar holds at most five actions. Extra actions go into an overflow menu.
- If an overflow menu has fewer than three options, show the actions as inline icon buttons instead.
- Tables need three or more columns (style page).
- Toolbar, batch action bar and pagination heights should match the row size. Exception: use the large toolbar for xl tables.
- Not covered by Carbon: text versus number alignment, empty states, a column-count limit, and horizontal scroll.

**Chart anatomy** (https://carbondesignsystem.com/data-visualization/chart-anatomy/):
- Nine elements: title, axes, ticks, axis title, legend, toolbar, zoom bar, graph frame, tooltip.
- Title should be descriptive and state the main insight.
- Label charts directly where possible to keep legends short.
- Tooltips appear on hover over data points, icon buttons, or truncated text. They should repeat the values on both axes and add relevant detail.
- Keep the graph frame uncluttered.
- No numeric values for ticks, gridline spacing, or sizing on this page.

**Axes and labels** (https://carbondesignsystem.com/data-visualization/axes-and-labels/):
- Bar and area charts: the numeric axis always starts at zero. Truncating a bar chart's y-axis "can distort perception and exaggerate percentage differences".
- Line charts and scatter plots may start at non-zero values, because they show trends.
- Never interpolate across missing data. Label the start and end of a no-data period, and show it with texture or an empty area.
- Axis break: 16px fixed on the y-axis, minimum 16px on the x-axis, drawn as a sinusoidal line.
- "Never change axis tick increments to accommodate data availability."
- Time labels: use the localized date format. Use semibold "landmark" labels where the day, month or year changes.
- Not covered: tick counts, gridline style, number formatting beyond dates, label rotation.

**Legends** (https://carbondesignsystem.com/data-visualization/legends/):
- Default placement is bottom. Top (including under the title) is for space-limited dashboards. Left is for better type alignment. Right is for layouts with plenty of room.
- Overlay is only allowed on geospatial charts, with a background at 80% opacity.
- By default, legend shows up to two lines of items. "View more" expands it, to no more than 30% of chart height, then it scrolls.
- Hovering one category sets all others to 30% opacity.
- Clicking a category isolates it.
- "Your chart doesn't need a legend if it only presents one data category."
- Hiding legends on mobile is discouraged unless only one category shows.

**Categorical color palette** (https://carbondesignsystem.com/data-visualization/color-palettes/, 14 colors, in a fixed order):
01 `#6929c4` (Purple 70), 02 `#1192e8` (Cyan 50), 03 `#005d5d` (Teal 70), 04 `#9f1853` (Magenta 70), 05 `#fa4d56` (Red 50), 06 `#570408` (Red 90), 07 `#198038` (Green 60), 08 `#002d9c` (Blue 80), 09 `#ee538b` (Magenta 50), 10 `#b28600` (Yellow 50), 11 `#009d9a` (Teal 50), 12 `#012749` (Cyan 90), 13 `#8a3800` (Orange 70), 14 `#a56eff` (Purple 50).
- A 4-color single-hue group is available for predictable counts: Purple 70, Blue 80, Cyan 50, Teal 70.
- No gridline or axis color guidance. No contrast ratios.

---

### 1.3 Linear (linear.app)

Linear's public material is about the theme system and UI direction. It gives no table row heights or font sizes.

**Redesign write-up** (https://linear.app/now/how-we-redesigned-the-linear-ui, also at https://linear.app/blog/how-we-redesigned-the-linear-ui):
- Headings use **Inter Display**. Body text uses **Inter**. No sizes, weights or line heights are published.
- Theme generation moved from HSL to **LCH** ("perpetually uniform" lightness across hues).
- A theme used to need 98 variables. The new system needs **three**: base color, accent color, and contrast.
- A contrast variable controls how contrasty a theme is. High-contrast themes can be generated for accessibility.
- Surface levels: background, foreground, panels, dialogs, modals.
- Stated goal: "reduce visual noise", and increase the hierarchy and density of navigation.
- Light mode uses darker text and neutral icons. Dark mode uses lighter text and icons.
- Not covered: list and table styling, border treatment, spacing, status icons, badges.

**Changelog, March 2024** (https://linear.app/changelog/2024-03-20-new-linear-ui, from search snippet only): default dark and light themes have increased contrast.

**UNVERIFIED third-party values** (from search snippets of community analyses, not Linear): sidebar background `#0f1011`, faint text `#62666d`. One analysis says the faintest text is about 3.6:1 on the near-black background, below the 4.5:1 WCAG AA minimum for body text. Use it as a warning, not as a palette.

---

### 1.4 Stripe (Stripe Dashboard and Stripe Apps docs)

Stripe's dashboard is not documented publicly at the component level. Its Stripe Apps docs (for apps that run inside the dashboard) are the closest public source. They have concrete tokens and chart rules.

**Spacing scale** (https://docs.stripe.com/stripe-apps/style):

| Token | px |
|---|---|
| xxsmall | 2 |
| xsmall | 4 |
| small | 8 |
| medium | 16 |
| large | 24 |
| xlarge | 32 |
| xxlarge | 48 |

- Width fractions: 1/2, 1/3, 2/3, 1/4 ... 11/12. `fill` = 100%.
- Text overflow uses `textOverflow: ellipsis` with `overflow: hidden`.
- Custom font faces are not allowed. Only the preset `font` tokens. (The font-size table did not come through in my fetch. **Gap**.)

**Chart layout pattern** (https://docs.stripe.com/stripe-apps/patterns/chart-layout):
- Wrap every chart in a fixed-height container. Stripe's table of heights:
  - **180px**: chart inside a dashboard metric card (compact).
  - **320px**: featured chart on a detail or full-width page.
  - Sparklines size to their container. Example used 24px high, 80px wide.
- Charts in the same row should have the same height.
- Maximum **three charts per row**. Below about **200px** wide, charts are hard to read.
- Row layouts: 1/2 + 1/2, 1/3 x3, or 2/3 + 1/3.
- Pair every chart with its current headline value above it.
- KPI block: label in `caption` style, secondary color. Value in `subtitle` style, bold.
- Chart states (loading, error, empty, populated) must all use the same height, so content below does not move.
- Empty state text example: "No data for this time period."
- Line chart: good for direction and rate of change. Its y-axis may zoom. Not for absolute totals.
- Bar chart: "Displays discrete totals as bars starting from zero." Warns that small differences in large values disappear.
- Meter chart: a single horizontal bar showing part-to-whole. Snapshot only. For change over time, use stacked bars.
- Sparkline: no axes, no labels. Shows shape, not values.

**Next-gen BarChart data model** (https://docs.stripe.com/stripe-apps/components/barchart.md?app-sdk-version=9Next):
- Base data: `x` (date, number or string), `y` (number or null), `name` (series name, groups bars by color).
- Stacked: same `x` with different `name` stacks automatically.
- Forecast: optional `forecast` value shown on top of the actual bar.
- Incomplete: `status: "complete" | "incomplete"`. Complete points must come before incomplete ones. The incomplete region is styled differently.

**Lists** (https://docs.stripe.com/stripe-apps/patterns/lists): use the clickable `List` component for rows that act as links. Custom list rows are not clickable, so put a link or button inside them.

**Stripe dashboard descriptions** (third-party analyses, not Stripe docs, all **UNVERIFIED**):
- Transaction tables are compact and sortable, and money amounts are right-aligned.
- Monospaced or tabular numerals are used for amounts so digits line up.
- Status colors: green for succeeded, red for failed, yellow for pending. Stripe's own docs say color should be reserved for status signals.
- Home screen: five headline numbers, each with a sparkline and a smaller prior-period comparison below.
- The claim of "6 distinct type sizes" is contradicted by the same article's description of three tiers. Ignore it.

---

### 1.5 Apple HIG and Swift Charts (optional)

Source: WWDC22 session "Design an effective chart" (https://developer.apple.com/videos/play/wwdc2022/110340/). The HIG charts page (https://developer.apple.com/design/human-interface-guidelines/charts) returned only its title, so its specifics are not verified.

- Focus on "a few key pieces of information".
- Bars for totals, because "zeros are visible without creating a distraction". Bar charts keep the lower bound fixed at 0: "a bar that's twice as tall as another has twice as many sales."
- Axes "frame marks to provide references for their values". Label the time range with start and end dates.
- Fixed range when a range is known (battery 0 to 100%). Dynamic range when there is no maximum.
- Gridlines: "around four horizontal grid lines". Use round intervals, such as multiples of 20, or steps of 7 days for a 30-day view (five vertical lines). Remove gridlines in previews.
- Too many gridlines are "overwhelming". Too few do not help the reader estimate mid-range values.
- Color is "an addition", not the only signal. Add symbols, and a legend. Balance saturation and luminosity across series so none looks more important. Check with color-blindness filters. Adapt for dark mode and increased contrast.
- Descriptions: put a summary label above the chart, such as "Total Sales: 1,234 Pancakes". Spell out month names in accessibility labels.
- Touch targets for bars should stretch to the full chart height.

---

### 1.6 Number formatting (platform reference)

From MDN `Intl.NumberFormat` (https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Intl/NumberFormat/NumberFormat):
- `notation: "compact"` with `compactDisplay: "short"` (default) gives `988M` (en-GB form). `"long"` gives `988 millions`.
- `signDisplay: "exceptZero"` gives `+55%` for 0.55 with `style: "percent"`. Zero shows no sign. Use this for deltas.
- `signDisplay` options: `auto` (default, negative only), `always`, `exceptZero`, `negative` (negative only, not negative zero), `never`.
- Default max fraction digits: decimal = max(minFD, 3). Currency = the ISO minor unit (2 for USD). Percent = max(minFD, 0), so `0%` by default.
- `currencyDisplay`: `symbol` (default), `narrowSymbol` (`$100` not `US$100`), `code`, `name`.

From MDN `font-variant-numeric` (https://developer.mozilla.org/en-US/docs/Web/CSS/font-variant-numeric):
- `tabular-nums` (OpenType `tnum`): all digits share one width, so columns align. Use in tables and KPI values.
- `lining-nums` (`lnum`): figures sit on the baseline. Use with tabular-nums.
- Caveat: only works if the font has the `tnum` feature.
- Tabular figures can look loose in running prose. Suit them to tables and counters.

---

## 2. Consolidated rules

### 2.1 Number formatting

| Rule | Recommendation | Basis |
|---|---|---|
| Thousands separator | Always on for values >= 1,000 (e.g. `12,480`) | Carbon example "1,234 Pancakes"; Tremor `Intl` examples |
| Compact notation | KPI values and axis ticks >= 10,000 use `1.2K`, `3.4M`, `1.2B`. Table cells show full values unless the column is marked compact. | MDN `notation: "compact"`. Choice of threshold: **UNVERIFIED**, my recommendation |
| Decimal places | Integers stay integers. Decimals: 1 for KPIs under 100, 2 for money in tables. Max 3 otherwise. | MDN defaults (3 for decimal). Rest **UNVERIFIED** |
| Currency | Symbol before amount. Two decimals in tables. Compact allowed in KPIs (`$1.2M`). Use `narrowSymbol` for dense UI. Currency code only when several currencies appear in one view. | MDN `currencyDisplay`. Rules **UNVERIFIED** |
| Percent | Round to 0 decimals in tables and charts, 1 decimal in KPIs. The value is shown as `%`, not as a fraction. | MDN percent default 0. Rest **UNVERIFIED** |
| Negative numbers | Use a true minus sign (U+2212) or the locale minus. Do not use parentheses in dashboards. Show red only when the metric semantics say bad. | Recommendation, **UNVERIFIED** |
| Deltas | Always show sign (`+12.4%`, `-3.1%`). Use `signDisplay: "exceptZero"` so zero shows no sign. Arrow next to the number. Color: good = green, bad = red, neutral = gray. | MDN `exceptZero`. Tremor `isIncreasePositive`. Colors **UNVERIFIED** |
| Units | Unit placement: currency and `%` attached to the number. Other units (ms, kg) as a short suffix or in the column header, never both. | Recommendation, **UNVERIFIED** |
| Missing values | `—` (em dash) in cells, not `0` and not blank. Never interpolate in charts. | Carbon "never interpolate". Em dash **UNVERIFIED** |

### 2.2 KPI / stat card anatomy

Stripe gives the labels and sizes. Tremor gives the card and delta structure.

```
+-----------------------------------+
| Label (caption, secondary)        |   12/16, secondary text
| 12,480          [ +4.2% ^ ]       |   value 28/32 semibold, tabular-nums
| vs previous 30 days               |   12/16, muted
| [~~~~~~~~~ sparkline 24px ~~~~~~] |   optional, 80 x 24
+-----------------------------------+
```

| Part | Size | Notes |
|---|---|---|
| Card | padding 16 or 24, radius 8 (Tremor) to 12 (ours), 1px border | Tremor p-6 is 24. Ours is 16. Pick one per density tier. |
| Label | 12px/16px, weight 500, muted | Stripe uses `caption` with secondary color for labels |
| Value | 28/32px, weight 600, tabular-nums | Stripe uses `subtitle` bold. Exact px **UNVERIFIED** |
| Delta badge | 12px text, 2px 8px padding, radius 6, ring 1px inset | Tremor badge classes |
| Comparison text | 12/16px, muted | Tremor delta has a comparison line below |
| Sparkline | 24px high, ~80px wide, no axes | Stripe example |
| Height | fixed container (Stripe: 180 for chart inside metric card) | Keep same height across loading, empty and loaded |

### 2.3 Table rules

**Density.** Carbon's default is 40px rows with 14px text. Our current 32px rows match Carbon `sm`. Offer two tiers: compact 32, default 40. Header row matches the body row size.

**Header style.**
- Carbon: 14px, semibold 600, 16px horizontal padding, 16px minimum column gap.
- Ours: 12px, weight 500, muted on a subtle fill. This is smaller and lighter than Carbon's. It fits a dense table, but test it at the muted color for contrast. **Check against WCAG 4.5:1.**

**Alignment.**
- Numbers right-aligned with `tabular-nums` (MDN, plus Stripe's third-party description).
- Text left-aligned.
- Right-align the header of numeric columns too, so the header sits over the numbers.
- Dates: left or right, but be consistent within a table.
- Status badges: left.

**Truncation vs wrapping.**
- Column titles: one or two words. If longer, wrap to two lines then truncate, with a tooltip for the full title (Carbon).
- Cell text: truncate with an ellipsis and show the full value in a tooltip. Do not truncate numbers. Our recommendation, **UNVERIFIED**.
- Multi-line cell content needs the xl (64px) row size in Carbon. Our equivalent is a `tall` density, used only when a column is marked `wrap`.

**Sorting.** Show the arrow only on the sorted column. Show icons on hover for unsorted columns (Carbon).

**Zebra.** Optional, off by default. Carbon uses a layer-accent background token on alternate rows.

**Empty and loading states.**
- Loading: skeleton rows, not a spinner (Carbon). Keep the table height stable.
- Empty: keep the header, show one message row in the body. Stripe example: "No data for this time period." Our wording **UNVERIFIED**.

**Column count.** Carbon requires three or more columns for a table, so a two-column data set probably wants a List. No Carbon limit on max columns. Horizontal scroll for more than about 6 to 8 columns in a card. **UNVERIFIED**, my estimate.

**Toolbar and actions.** At most five actions in the toolbar. Overflow menu after that. If the overflow menu has fewer than three items, show them as icon buttons instead (Carbon).

**Lists.** A row that is a link is clickable. Non-clickable rows get an inner link or button, not row-level click (Stripe).

### 2.4 Chart rules

**Bar vs line.**
- Bar: totals and comparisons across categories or periods. Always zero baseline.
- Line: trends over time. Y-axis may start above zero (Carbon, Stripe).
- Stripe's warning: a bar chart hides small changes on large totals. If the change matters, use a line.

**Sorting bars.** Sort bars by value when the category order has no meaning. Keep time order for time series. (Recommendation, **UNVERIFIED** against a primary source.)

**Axis starts at zero.** Bar charts only. Enforce in the validator, not only in the renderer.

**Max categories.** Stripe warns against more than three charts in a row, and says charts below about 200px wide are hard to read. For bars, no category limit given. Our recommendation: above about 12 bars, switch to a line or show top N with "other". **UNVERIFIED**.

**Label rotation.** No primary source. Recommendation, **UNVERIFIED**: prefer horizontal labels with truncation. Rotate only if truncation loses meaning.

**Y-axis tick count.**
- Apple: around four horizontal gridlines is comfortable.
- Tremor default: the y-axis is auto-generated, with `tickGap` 5. Exact tick count not documented.
- Recommendation: 4 to 5 ticks. Use round intervals (1, 2, 2.5, 5 times a power of ten).

**Rounding.** Round the tick values to a nice interval. Do not change tick increments to fit the data (Carbon).

**Gridlines.**
- Horizontal only (our current default) matches Apple's guidance of about four.
- Vertical lines only for time axes with regular spacing. Apple's example: one every 7 days.
- Carbon and Apple do not give a gridline color. Use a low-contrast token, roughly 10% to 15% of the foreground, **UNVERIFIED**.

**Chart height.**
- Stripe: 180px in a metric card, 320px for a featured chart. Same height across a row.
- Tremor default: 320px (`h-80`).
- Ours: 240px. Keep 240 for a standard card and add a `compact` of 180 for KPI rows and `featured` of 320.

**Legend.**
- Single series: no legend (Carbon).
- Multi-series: bottom by default, top on dashboards (Carbon). Show up to two lines, then "View more". Hover dims others to 30% opacity.
- Prefer direct labels where the chart has room.

**Tooltip anatomy.** Show the category or x value, then the formatted value. For two-axis charts, show both (Carbon). Apple's summary label pattern: "Total Sales: 1,234 Pancakes". Use the same number formatter in the tooltip as on the axis.

**Color.** Okabe-Ito is already colorblind-safe. Carbon's categorical palette of 14 is too many for us. For a single-series bar or line, use one accent color. Do not use color alone to encode status. Add an icon or text (Apple, Stripe).

**Empty, loading, error.** Same height in all states (Stripe). Loading skeleton, not spinner (Carbon).

**Missing data.** Never interpolate. Draw a gap and label the start and end of the gap (Carbon).

**Accessibility.** Give every chart a short text summary, and a data table fallback (Carbon, Apple).

---

## 3. Ideas for us

Each idea cites its source. Items marked **UNVERIFIED** are my recommendations, not taken from a primary source.

1. **Two density tiers for Table.** `density: "compact" | "default"` with rows 32px and 40px. Header row height matches the rows. Source: Carbon data table style (https://carbondesignsystem.com/components/data-table/style). Our current 32px becomes `compact`. Default moves to 40px to match Carbon's default.

2. **Header type and color.** Consider 13px or 14px semibold 600 for column headers, instead of 12px weight 500 muted. Carbon uses 14/600. Check contrast of the muted header color on the subtle fill against 4.5:1. Source: Carbon style page. Final choice **UNVERIFIED**.

3. **Column header rules.** Limit headers to one or two words. Wrap to two lines, then truncate, and show the full header in a tooltip. Source: Carbon usage page (https://carbondesignsystem.com/components/data-table/usage/).

4. **Sort affordance.** Show a sort arrow only on the sorted column. Show icons on hover for others. Source: Carbon usage page.

5. **Right-align numeric columns, header included.** Use `tabular-nums` and `lining-nums` for `format: "number" | "currency" | "percent"` columns. Source: MDN `font-variant-numeric`. The "money right-aligned" claim comes from a third-party Stripe analysis, so treat it as **UNVERIFIED**.

6. **`format` option on TableColumn and KPI value.** Add `format: "number" | "compact" | "currency:USD" | "percent" | "delta"`. Map each to `Intl.NumberFormat`. `compact` maps to `notation: "compact"`. `delta` uses `signDisplay: "exceptZero"`. Source: MDN `Intl.NumberFormat`. Keeps formatting in one place in Rust core and renderer.

7. **Stat/KPI component.** Anatomy: label (12/16, muted), value (28/32, semibold, tabular), delta badge, optional comparison line, optional sparkline (24px high). Stripe's KPI pattern is label above value, with sparkline next to value. Source: Stripe Apps chart layout (https://docs.stripe.com/stripe-apps/patterns/chart-layout). Sizes for our component **UNVERIFIED** beyond Stripe's caption and subtitle roles.

8. **Delta badge with `isIncreasePositive`.** Add `good_when: "up" | "down"` (Tremor's name is `isIncreasePositive`) so a cost decrease can render green. Five delta states from Tremor's `deltaType`: increase, moderateIncrease, unchanged, moderateDecrease, decrease. Source: Tremor BadgeDelta (https://www.tremor.so/docs/ui/badges). Colors **UNVERIFIED**.

9. **Chart height tiers.** `size: "compact" | "default" | "featured"` at 180, 240 and 320px. Same height for charts in one row. Source: Stripe chart layout (180 and 320) and Tremor default (320). Our 240 stays as default.

10. **Max three charts per row, minimum width about 200px.** Enforce in the Grid component, or in the validator as a warning. Source: Stripe chart layout.

11. **Validator rule: bar charts start at zero.** Reject, or force `min: 0`, for bar charts. Allow non-zero for line charts. Source: Carbon axes page and WWDC22.

12. **Never interpolate missing data.** Chart renders a gap for null `y` and labels the gap. Source: Carbon axes page. Matches Stripe's `y: number | null` in the BarChart data model.

13. **Tick and gridline rules.** 4 to 5 horizontal gridlines, round intervals, no tick-increment changes to fit data. Source: Apple WWDC22 (four gridlines, multiples of 20) and Carbon ("never change axis tick increments").

14. **Legend rules.** Hide legend for single-series charts (Carbon). For multi-series: bottom by default, at most two lines, "View more" beyond that. Source: Carbon legends page.

15. **Hover dimming for multi-series.** Hovering one series sets others to 30% opacity. Source: Carbon legends page.

16. **Tooltip format.** Show category and formatted value. Two-axis charts show both axes. Use the same formatter as the axis. Source: Carbon chart anatomy page, Apple summary label.

17. **Chart summary line.** Each chart takes an optional `summary` string rendered above it, such as "Total sales: 1,234". Source: Apple WWDC22 (summary label). Also serves as the screen reader description.

18. **Status badges with icon or text, not color alone.** Badge takes `status: "success" | "warning" | "critical" | "neutral"` and always renders a label or icon. Source: Apple WWDC22 (color as addition), Stripe third-party description (green, red, yellow).

19. **Skeleton and empty states at fixed height.** Table and chart loading show skeleton rows at the same height as the loaded content. Source: Carbon usage (skeleton instead of spinner), Stripe chart layout (same height across states).

20. **Three-variable theme model.** Our color tokens could be generated from `base`, `accent` and `contrast` (Linear's approach, down from 98 variables). Source: Linear redesign write-up (https://linear.app/now/how-we-redesigned-the-linear-ui). Generating from LCH is worth testing. **UNVERIFIED** for us.

21. **Contrast floor for secondary text.** Set a minimum contrast of 4.5:1 for muted label and header text. Linear's contrast setting shows the trade-off. Third-party analysis flagged about 3.6:1 faint text as a failure. Source: Linear changelog and third-party analysis (**UNVERIFIED** value).

22. **Em dash for missing cells.** Show `—` for null values in tables and KPIs, not `0`. Source: Carbon "never interpolate" applied to tables. **UNVERIFIED** as a table rule.

23. **Clickable lists vs static rows.** Two components: `List` (clickable rows) and `ListRow` (static content, links inside). Source: Stripe lists pattern (https://docs.stripe.com/stripe-apps/patterns/lists).

24. **Card padding tiers.** Default 16px, roomy 24px. Tremor's card uses 24px. Source: Tremor card (https://www.tremor.so/docs/ui/card). Our radius 12 is larger than Tremor's 8. Keep 12 or move to 8 after a visual check. **UNVERIFIED** preference.

---

## 4. Gaps and verification notes

- **Tremor KPI card and badge pages.** The `/docs/ui/kpi-card` page returned 404. The badge styling I captured looks generic and may not be Tremor's own. Exact KPI type sizes are not verified.
- **Tremor version.** The BarChart defaults come from a v3-era source. Check the version before using them.
- **Carbon chart pages.** `bar-charts` and `line-charts` returned 404 at the URLs I tried. Carbon has no published guidance on tick counts, gridline color, number formatting or label rotation. Carbon's chart docs say they are still a work in progress.
- **Carbon row sizes.** Current docs use five sizes (24/32/40/48/64). Carbon v10 uses four (24/32/48/64). The values above are from the current docs.
- **Linear.** No public table or list specs. The LCH and three-variable detail is from the official write-up. The hex values I saw came from third-party sites.
- **Stripe Dashboard.** No public component spec. The Stripe Apps docs are for apps inside the dashboard, not for its native tables. The font-size table in Stripe's style page did not come through in the fetch.
- **Apple HIG charts page.** Returned only its title. The WWDC22 session is the source for Apple points.
- **Number formatting for our rules.** Only the `Intl.NumberFormat` defaults are verified. The thresholds for compact notation, decimal rules, and the em dash are recommendations.

---

## 5. Source URLs

Primary, fetched:
- https://www.tremor.so/docs/ui/card
- https://www.tremor.so/docs/visualizations/bar-chart
- https://www.tremor.so/docs/ui/badges (badge classes not confirmed as Tremor's own)
- https://carbondesignsystem.com/components/data-table/style
- https://carbondesignsystem.com/components/data-table/usage/
- https://carbondesignsystem.com/data-visualization/chart-anatomy/
- https://carbondesignsystem.com/data-visualization/axes-and-labels/
- https://carbondesignsystem.com/data-visualization/legends/
- https://carbondesignsystem.com/data-visualization/color-palettes/
- https://linear.app/now/how-we-redesigned-the-linear-ui
- https://docs.stripe.com/stripe-apps/style
- https://docs.stripe.com/stripe-apps/patterns/chart-layout
- https://docs.stripe.com/stripe-apps/patterns/lists
- https://docs.stripe.com/stripe-apps/components/barchart.md?app-sdk-version=9Next
- https://developer.apple.com/videos/play/wwdc2022/110340/
- https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Intl/NumberFormat/NumberFormat
- https://developer.mozilla.org/en-US/docs/Web/CSS/font-variant-numeric

Search snippets only (not fetched):
- https://linear.app/changelog/2024-03-20-new-linear-ui
- https://www.925studios.co/blog/stripe-dashboard-design-breakdown (third-party, fetched, claims mostly unverified)
- https://www.tessl.io/registry/tessl/npm-tremor--react/3.18.0/files/docs/icon-elements.md (Tremor v3.18.0 API, used for BadgeDelta props)

Not reached (404 or title-only):
- https://developer.apple.com/design/human-interface-guidelines/charts
- https://carbondesignsystem.com/data-visualization/bar-charts/ and /line-charts/
- https://www.tremor.so/docs/ui/kpi-card
- https://linear.app/blog/how-we-redesigned-the-linear-ui-part-ii
