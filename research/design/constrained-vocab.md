# Constrained vocabularies: Slack Block Kit, Adaptive Cards, Raycast

Research date: 2026-10-21. Web sources only (official docs and design guidance).

How to read the claims:
- **Verified** = stated on a page I fetched.
- **Search summary** = seen only in a search-result snippet; I did not open the page.
- **Unverified** = my inference, or a claim I could not confirm. Listed again in section 6.

## 0. Method and caveats

- Fetched pages with WebFetch and read them through a small model, so exact wording may drift. Numbers were re-checked where they matter.
- Several official URLs returned 404 or moved: Slack `/block-kit/design-guidelines` and `/block-kit/surfaces`, Adaptive Cards `authoring-cards/fallback` and `authoring-cards/versioning`, Raycast `information/human-interface-guidelines`, and the old Raycast toast/alert paths. I used the nearest live pages instead (listed in section 7).
- Slack has reorganized Block Kit docs. The current index lists 21 block types and 46 block elements; the vocabulary below reflects that index, not older Block Kit docs.
- Raycast does not publish a numeric limit on accessories in the pages I could read. The "do not use more than N accessories" rule is not verified.
- None of this was tested by rendering. Limits are what the docs state.

## 1. Slack Block Kit

### 1.1 Vocabulary

**Blocks** (21 on the current blocks index):
- **Actions**: row of up to 25 interactive elements (verified).
- **Alert**: highlights a warning, error or critical message.
- **Card**: card container (listed on index; not read in detail).
- **Carousel**: lets users page through several results or options.
- **Container**: groups related content for hierarchy.
- **Context**: supporting info that is not primary content (named in design guidance).
- **Context actions**: actions attached to a context row (index only).
- **Data table**: tabular data (index only).
- **Data visualization**: charts, graphs, statistics.
- **Divider**: horizontal separator.
- **File**: file reference.
- **Header**: large heading; text max 150 characters (verified).
- **Image**: image with required alt text in design guidance.
- **Input**: wraps one input element with label and hint; label and hint max 2000 characters (verified).
- **Markdown**: markdown content block (index only).
- **Plan**: plan/steps display (index only).
- **Rich text**: formatted content. The design page calls it preferred over markdown for agent-generated output (verified in page summary).
- **Section**: text (max 3000 characters), up to 10 fields of max 2000 characters each, and at most one accessory element (verified).
- **Table**: table (index only).
- **Task card**: action items with call-to-action buttons.
- **Video**: video block.

**Elements** (46 on the block elements index). Grouped, one line per group:
- **Buttons and menus**: Button (text max 75 characters, value max 2000, action_id max 255, style `primary` / `danger` / default; verified), Workflow button, Icon button, Overflow menu (button that reveals options), Link.
- **Selection**: Checkboxes (max 10 options; verified), Radio button group, Select menu (single), Multi-select menu, Date picker, Datetime picker, Time picker, Date (display), Color (display).
- **Text and data entry**: Plain-text input (single or multi-line), Rich text input (composer), Email input, URL input, Number input, File input.
- **Display and mentions**: Text, Image, Emoji, Tag (colored pill), Channel, User, Team, Usergroup, Message, File, Canvas and Canvas mention variants, Citation (AI source reference), Feedback buttons, Attachment mention, Work Object mention, Workflow mention, Broadcast (here / channel / everyone), Link.
- **Rich text children**: Rich text section, list, quote, preformatted.
- **Platform-specific**: List record, Salesforce data field, URL source (task cards).

**Composition objects**: Text object (`plain_text` or `mrkdwn`, max 3000 characters, verified), Option object, Option group object, Confirmation dialog, Dispatch action configuration (controls when a plain-text input sends a payload), Trigger, Workflow, Slack file object, Slack icon object, Conversation filter.

### 1.2 Limits

Verified:
- Up to **50 blocks per message**; up to **100 blocks** in modals or Home tabs.
- Section: text max **3000** chars; **10** fields max; field text max **2000**; one accessory.
- Actions block: max **25** elements; `block_id` max **255**.
- Text object: **3000** chars (both types).
- Button: text **75**; value **2000**; action_id **255**.
- Header: text **150**.
- Input: label **2000**; hint **2000**; accepts 13 element types including checkboxes, radio buttons, selects, date and time pickers, number, email, URL and rich text inputs.
- Checkboxes: max **10** options.

Search summary (not opened, conflicting legacy wording):
- Static select menu: max **100** options (flat list), or option groups (max **100** groups, each up to **100** options, label max **75**). You cannot mix flat options and groups.

Not stated on the pages I read: nesting depth, total characters per message.

### 1.3 What they deliberately do not allow

The docs give little rationale. What is stated or clearly implied:
- **No styling surface.** There are no colors, font sizes, paddings or custom CSS. The only visual knobs are semantic: button `style`, text type (`plain_text` vs `mrkdwn`), and a few flags (`focus_on_load`, `expand`, `dispatch_action`).
- **Markdown is limited.** Slack pushes generated content toward rich text blocks and away from markdown (verified in page summary). Inference: markdown renders differently across clients, so structure is safer.
- **Bounded size.** Hard caps on blocks, fields and options (inference: keeps messages readable in notifications and small surfaces).
- **No code.** Interactivity is declared: an action ID is sent back to the app; the app runs the code.

### 1.4 How the host controls appearance

Slack owns the look entirely. The developer chooses:
- The surface: message, modal (100 blocks), or Home tab.
- Semantic button style: `primary`, `danger`, default.
- Text type: plain or mrkdwn.
- Input flags: `initial_options` must match options; `focus_on_load` (only one per view).

There is no theme, color or size API in what I read.

### 1.5 Fallback and versioning

- **Top-level `text` as fallback.** When blocks are present, `text` is the notification fallback and is recommended (not required). Screen readers read only the top-level text, not the content of blocks (search summary of Slack accessibility guidance).
- **Notification change (July 2026).** Desktop notifications now pull text from blocks first and fall back to `text`. Mobile still uses `text` only (search summary of Slack changelog, 2026-07-13).
- **Unknown block types.** Not documented in the pages I read.
- **Versioning.** No schema version field found. The API is the contract; the block set is extended by adding new types.

### 1.6 Design guidance (verified from the designing-with-Block-Kit page)

- Show only what the current step needs (progressive disclosure). Reveal more only when a choice calls for it.
- Move rarely used actions into an overflow menu.
- Use sensible default values to reduce decisions.
- Put non-primary information in context blocks.
- Give every image specific alt text; limit decorative images.
- Keep sentences short; avoid jargon, idioms, and directional or sensory language.
- Do not use an emoji as a control; always pair emoji with text; use them sparingly in headers; not both in header and subtext.
- Do not rely on color alone to convey meaning.
- Button labels should stand alone (example: "cancel subscription") and be brief to avoid truncation.
- Wrap inputs in input blocks, not section or action blocks.
- Use descriptive placeholders for selects.
- When an interactive flow or conversation ends or expires, update the message to remove buttons and menus and leave a short text record.
- No more than three large flashes per second in animations.

### 1.7 Interaction patterns worth borrowing

- **Overflow menu** for secondary actions, instead of a row of buttons.
- **Input block** with label, hint, and an opt-in dispatch flag (send on change vs on submit).
- **Confirmation dialog object** attached to an interactive element.
- **Stale-state cleanup**: when a flow ends, the interactive parts are removed and a text record stays.
- **Separate surfaces by size**: small message vs modal with a higher block cap.

## 2. Adaptive Cards

### 2.1 Vocabulary (schema 1.0 to 1.6)

**Body element types** (verified on AdaptiveCard `body` allowed values): `ActionSet`, `ColumnSet` (contains `Column`), `Container`, `FactSet`, `Image`, `ImageSet`, `Input.ChoiceSet`, `Input.Date`, `Input.Number`, `Input.Text`, `Input.Time`, `Input.Toggle`, `Media`, `RichTextBlock`, `Table` (1.5; contains `TableRow` and `TableCell`), `TextBlock`.

**Actions** (verified on AdaptiveCard `actions` allowed values): `Action.Execute` (1.4), `Action.OpenUrl`, `Action.ShowCard`, `Action.Submit`, `Action.ToggleVisibility`.

**Card-level properties**: `version` (required on top-level card), `fallbackText`, `minHeight` (1.2), `rtl` (1.5), `backgroundImage`, `selectAction` (1.1), `speak`, `lang`, `verticalContentAlignment` (1.1), `refresh` and `authentication` (1.4), `metadata` (1.6).

**Common element properties**: `spacing`, `separator`, `id`, `isVisible` (1.2), `requires` (1.2), `fallback` (1.2), `height` (1.1, `auto` or `stretch`).

**Notable element properties**:
- `TextBlock`: `text` (markdown subset), `color` (`default`, `dark`, `light`, `accent`, `good`, `warning`, `attention`), `fontType` (`default`, `monospace`), `horizontalAlignment`, `isSubtle`, `maxLines`, `size` (`default`, `small`, `medium`, `large`, `extraLarge`), `weight` (`default`, `lighter`, `bolder`), `wrap`, `style` (`default`, `heading`; 1.5 for accessibility).
- `Table`: `columns`, `rows`, `firstRowAsHeader` (default true), `showGridLines` (default true), `gridStyle`, default cell alignments.
- `Image`: `size` (`auto`, `stretch`, `small`, `medium`, `large`), explicit pixel width and height, `style` (for example person), `altText`.

**Mentioned on the Adaptive Cards docs hub but not verified in schema pages I read**: Carousel, Badge, Icon, CodeBlock, Rating, ProgressBar, Charts. Teams docs say CodeBlock is web and desktop only. Treat as unverified.

### 2.2 Limits

Stated in what I read:
- `HostConfig.actions.maxActions` default **5**.
- Teams design guidance: **1 to 3** primary actions per card, **up to 6**.
- Elements stretch to parent width (renderer spec).
- Text does not wrap unless `wrap` is true (element docs say it does not wrap by default; host config `textBlock.wrap` default is true for cards that do not specify). Treat the default as a host setting.
- Fixed pixel column widths should be small; Teams mobile does not scroll horizontally.

Not stated: text length limits, maximum nesting depth, maximum body elements. Unverified whether `maxActions` overflow is dropped or moved; not found in pages read.

### 2.3 What they deliberately do not allow

From the Motivations and Principles page (verified):
- **Declarative, no code.** "There is no code behind" in Adaptive Cards. Card authors "cannot embed custom/arbitrary code." Interactivity comes from declared actions only. Hosts never run third-party code.
- **Technology-neutral.** The format does not rely on HTML, XAML, JavaScript or other UI frameworks, so a renderer can exist on any native platform.
- **Card authors own content; host apps own look and feel.** Authors may specify styling "only via semantic expressions of colors, sizes, etc." This is principle 8, and it is the key rule for us.
- **Conservative feature growth.** "When in doubt, keep it out!" New features need real use cases and a functional spec.
- **Extensions outside the spec break interoperability.** Host-specific elements are allowed but make a card incompatible with mainstream hosts.

Detail limits from element docs:
- `RichTextBlock` does not support markdown. `TextBlock` supports a subset.
- No input or action rendering when host `supportsInteractivity` is false.

### 2.4 How the host controls appearance (HostConfig)

HostConfig is the shared config object. Top-level keys include `fontFamily` (default "Calibri"), `containerStyles`, `imageSizes`, `imageSet`, `factSet`, `fontSizes`, `fontWeights`, `spacing`, `separator`, `actions`, `media`, `allowCustomStyle`, `supportsInteractivity`, `imageBaseUrl`. Verified defaults:

- **spacing** (px): `small` 3, `default` 8, `medium` 20, `large` 30, `extraLarge` 40, `padding` 20. The schema enum also has `none`, which is not a HostConfig key in the table.
- **fontSizes** (px): `small` 10, `default` 12, `medium` 14, `large` 17, `extraLarge` 20.
- **fontWeights**: `lighter` 200, `default` 400, `bolder` 800.
- **foregroundColors**: roles `default`, `accent`, `dark`, `light`, `good`, `warning`, `attention`. Each has a `default` and a `subtle` value (ARGB hex defaults).
- **containerStyles**: `default` and `emphasis` objects. The container style enum also includes `good`, `attention`, `warning`, `accent`.
- **imageSizes** (px): `small` 80, `medium` 120, `large` 180.
- **imageSet**: `imageSize` default `auto`; `maxImageHeight` default 100.
- **separator**: `lineThickness` default 1; `lineColor` default a semi-transparent black.
- **actions**: `actionsOrientation` (default `horizontal`), `actionAlignment` (default `stretch`), `buttonSpacing` 10, `maxActions` 5, `iconPlacement`, `iconSize` 30, `showCard` (`actionMode` `inline` or popup, `inlineTopMargin` 16).
- **factSet**: title (`weight` bolder, `maxWidth` 150), value, `spacing` 10.
- **inputs**: `label` (required and optional label styling, suffix default `*`), `errorMessage` (1.3).
- **textBlock** (defaults for cards that do not specify): `size` default, `weight` normal, `color` default, `isSubtle` false, `wrap` true, `maxWidth` 0.
- **media**: `allowInlinePlayback` true, `defaultPoster`, `playButton`.

Spacing rule: a renderer looks up the amount from HostConfig for the enum value. The card never supplies pixels. Spacing applies only between an element and a preceding element. Separators are drawn in the default container foreground color and never before the first item.

### 2.5 Fallback and versioning

Verified from the renderer spec and schema pages:
- **Parse errors.** Invalid JSON or schema-invalid cards must be reported to the host.
- **Unknown element types MUST be dropped**, the renderer continues with the rest of the payload, and alterations SHOULD be reported to the host as warnings.
- **Unknown properties** are kept on the element.
- **Card `version` above the supported version**: the renderer MUST return `fallbackText` instead.
- **Card `version` below the card's required version**: `fallbackText` is shown (may contain markdown).
- **Element `fallback`** (1.2): either `drop` (drop immediately, do not bubble up) or an alternative element. It is triggered by an unknown element type or by unmet `requires`.
- **Element `requires`** (1.2): a map of feature to minimum version. Missing or too-old features trigger `fallback`.
- **Formatting failures** in text date functions MUST display the raw string, so the developer sees the problem. This is a deliberate choice: errors stay visible.

Version-driven behavior changes (verified):
- `Action.Submit` input collection changed between 1.x and 2.x renderers. 1.x gathers all inputs on the card; 2.x gathers only inputs in the parent container or as siblings.
- Input `label` (1.3) replaced TextBlock labels in samples for accessibility.
- `TextBlock style: heading` (1.5) marks headings for accessibility.

Search summary (unverified): Teams mobile supports up to 1.2 or 1.6. Outlook mobile supports up to 1.0. These come from community Q&A and an AI-written answer; check the official support matrix before using them.

### 2.6 Design guidance

From the Teams design page (legacy but still live) and the Adaptive Cards hub:
- **Design for narrow first.** Narrow cards scale up well; wide cards do not scale down.
- **Columns.** Use `ColumnSet` for grids. Set `width: auto` only on the column that must stay fully visible. Do not set `auto` on every column, or a button beside an input may be clipped. Use `stretch` for the flexible column.
- **Fewer columns on mobile.** Too many columns clutter the card.
- **Pixel widths** only for values whose size does not change (thumbnails, numbers). Never exceed the narrow width.
- **Wrap every TextBlock.** Text does not wrap by default on the element; set `wrap: true`.
- **Actions.** Include 1 to 3 primary actions; up to 6. "Less is more."
- **Be concise.** Once cards scroll out of view they are less useful.
- **Containers.** Use `style` to emphasize; `selectAction` for tap; `ToggleVisibility` for collapsible groups. "Don't use containers for any reason other than" these.
- **Images.** Design for high DPI; use transparent backgrounds; no padding in the image file; fixed `width`/`height` only when exact size is needed.
- **Backgrounds.** No fixed background color unless it must be visible. Keep text contrast against any background you add.
- **Accessibility.** Use input `label`, not placeholder text (placeholders often have weak contrast and may not be read). Use `style: heading` for headings.
- **Width precedence** (renderer spec): pixels, then weight, then auto, then stretch.

### 2.7 Interaction patterns worth borrowing

- **Action.Submit** gathers inputs and sends a data payload. "What submit means" is up to the host, not the card.
- **Action.ShowCard** toggles inline or opens a popup, set by HostConfig.
- **Action.ToggleVisibility** for collapsible groups without a round trip.
- **selectAction** on `Column`, `ColumnSet`, `Image`, `Container` gives a whole-element tap target. A card-level selectAction does not allow ShowCard.
- **Action.OpenUrl** lets the host override how a URL opens (for example in-app).
- **Action.Execute** (1.4) runs through the host's universal action pipeline; `refresh` lets a card re-fetch.
- **Tables** (1.5): `firstRowAsHeader` for screen readers; grid color by container style role.

## 3. Raycast extension API

### 3.1 Vocabulary

**List**: `List.Section`, `List.Item` (`title` required; `subtitle`, `id`, `accessories`, `icon`, `keywords`, `actions`, `detail`, `quickLook`), `List.Item.Detail` (`markdown`, `isLoading`, `metadata`), `List.Item.Accessory` (types `text`, `icon`, `date`, `tag`, `tooltip`), `List.EmptyView` (`title`, `description`, `icon`, `actions`), `List.Dropdown` (search bar accessory, with `Item` and `Section`).

**Grid**: `Grid.Section`, `Grid.Item` (`content` required; `title`, `subtitle`, `accessory`, `actions`, `keywords`, `id`, `quickLook`), `Grid.EmptyView`, `Grid.Dropdown` (search bar accessory).

**Detail**: `markdown` (CommonMark plus LaTeX), `isLoading`, `navigationTitle`, `metadata`, `actions`. Metadata children: `Detail.Metadata.Label` (title, icon, text), `Link` (title, text, target), `TagList` (title, children) with `TagList.Item` (text or icon, color, onAction), `Separator`.

**Form**: `TextField`, `PasswordField`, `TextArea` (with `enableMarkdown`), `Checkbox`, `DatePicker` (`type` DateTime default or Date; `min`/`max`), `Dropdown` (with `Item` and `Section`), `TagPicker` (with `Item`), `Separator`, `FilePicker`. A `Description` type exists in the API but was not captured in full.

**ActionPanel**: `ActionPanel` (`title`), `ActionPanel.Section` (`title`), `ActionPanel.Submenu` (`title` required, `icon`, `shortcut`, `autoFocus`, plus filtering and search props).

**Action** (base: `title` required; `icon`, `onAction`, `shortcut`, `style`, `autoFocus`). Built-ins: `Action.CopyToClipboard` (with `concealed`), `Action.Paste`, `Action.Open`, `Action.OpenInBrowser`, `Action.OpenWith`, `Action.ShowInFinder`, `Action.Trash`, `Action.Push` (`onPush`, `onPop`), `Action.SubmitForm`, `Action.CreateSnippet`, `Action.CreateQuicklink`, `Action.ToggleQuickLook`, `Action.PickDate`.

**Feedback**:
- `Toast`: styles `Animated`, `Success`, `Failure`. Props `title` (required), `message`, `primaryAction`, `secondaryAction` (each with `title`, `onAction`, `shortcut`), `style`. Methods `show()`, `hide()`.
- `HUD` (`showHUD`): `showToast` falls back to it when the Raycast window is closed.
- `Alert` / `confirmAlert`: `title` required, `message`, `primaryAction`, `dismissAction`, `icon`, `rememberUserChoice`. Action styles `Default`, `Destructive`, `Cancel` (Cancel has no description on the page).

### 3.2 Limits

Verified:
- **Grid columns**: 1 to 8. **Grid.ItemSize** is deprecated.
- **Grid aspectRatio**: `1` (default), `3/2`, `2/3`, `4/3`, `3/4`, `16/9`, `9/16`.
- **Grid fit**: `contain` (default, letterboxes) or `fill` (may crop).
- **Pagination** (`onLoadMore`, `hasMore`, `pageSize`): needs `@raycast/api` 1.69.0 or later. Memory limits can stop `onLoadMore`. Docs suggest showing about 20 items, since if everything fits on screen `onLoadMore` never fires.
- **Accessories**: multiple accessories since v1.31.0.
- **Grid.Item accessory**: the docs conflict. The migration note says Grid.Item does not "currently" support accessories; the props table lists `accessory`. Treat as unverified.
- **Icons in the Store**: 512x512 PNG; default Raycast icon is rejected.
- **Screenshots**: max 6, at least 3 recommended; 2000x1250 PNG.
- **Titles**: Title Case; extension titles are nouns; command titles usually verb plus noun; no articles.
- **Toast and Form**: no character or numeric limits stated.

### 3.3 What they deliberately do not allow

Verified from the guidelines and API pages:
- **No own navigation stack.** "Avoid introducing your own navigation stack." Use the Navigation API and `Action.Push`.
- **Do not change `navigationTitle` in the root command.** Use it only on nested screens. Avoid long titles and repeated updates.
- **No empty list before data.** Do not return an empty list before data arrives; show a loading state instead (avoids flicker).
- **No invented layouts.** You get List, Grid, Detail, Form, and ActionPanel. Detail markdown is CommonMark plus LaTeX; images come from the extension's assets folder, with `raycast-width`/`raycast-height` query parameters and a tint parameter (the docs disagree on its name).
- **Store rules**: extensions must bring value beyond native Raycast features and not duplicate existing extensions. Screenshots show no other apps and no sensitive data.
- **Secrets are not drafted.** `PasswordField` values are not saved to drafts.

Inference (not stated): the restrictions protect keyboard consistency and a uniform feel across hundreds of third-party commands.

### 3.4 How the host controls appearance

Raycast owns layout, typography, keyboard handling, and the chrome of List, Grid, Detail, Form, and ActionPanel. The extension supplies content and semantic choices:
- `Color` values for accessories, tags, and metadata labels (defaults `Color.PrimaryText`, `Color.SecondaryText`).
- Icons, which must look right in both light and dark themes (Store guideline).
- Markdown content in Detail and List.Item.Detail.

No HostConfig-style knobs are exposed to the extension. The host (Raycast) decides spacing and font size.

### 3.5 Fallback and versioning

Versioning is by the npm package `@raycast/api`, not by a schema. Feature gates are per API version:
- Accessories array: 1.31.0.
- Grid: 1.36.0.
- Pagination: 1.69.0.
- Grid.ItemSize: deprecated; migration path is in the changelog and migration pages.

Failure handling (verified from best practices):
- Unhandled exceptions and unresolved promises show an error screen automatically.
- Handle expected errors yourself. Usually show a `Toast` with `Toast.Style.Failure`, a short title, and the error as `message`.
- Prefer stale data over a hard failure: "if a network request fails but you can read the cache, show the cache."
- Runtime dependencies: show a helpful message when a command needs an app; enable only the action that needs it if the dependency is missing.

### 3.6 Design guidance

Verified:
- **Titles.** Extension titles are nouns, understandable in the store. Command titles verb plus noun, no articles, no bare service name. Subtitles add context and must not be descriptions.
- **Action panel.** Give all actions icons or none; never mix. Add an ellipsis to actions that open a submenu. Do not repeat the parent action name in the submenu.
- **Placeholders.** Add placeholders to every text field, text area, and search bar. "Don't leave the search bar without a placeholder."
- **Empty states.** Customize "No results" with `EmptyView`. Never shown while `isLoading` and the search bar is empty.
- **Loading.** Render immediately; start with an empty list or static form, then fill it. Use `isLoading` on List, Grid, Detail, and Form.
- **Forms.** Validate on blur, clear the error on change. Submit is blocked while any item has an error. Recommended: `useForm` from `@raycast/utils`.
- **Accessories vs detail.** When `isShowingDetail` is on, avoid accessories on the same item; put the information in the detail.
- **Async search.** Use `throttle` on `onSearchTextChange` when fetching.
- **Configuration.** Preferences API for setup and credentials, not separate setup commands. README for setup-heavy extensions.
- **Destructive actions.** `Action.Style.Destructive` for the action, and a confirmation `Alert` (`confirmAlert`) for irreversible steps.

Unverified: the "do not use more than N accessories" rule. I did not find it in the list, grid, store, or best-practice pages.

### 3.7 Keyboard model

Verified (API and best-practice pages):
- The first and second actions get default shortcuts automatically.
  - **List, Grid, Detail**: primary Enter; secondary Cmd+Enter.
  - **Form**: primary Cmd+Enter; secondary Cmd+Shift+Enter.
- Custom shortcuts are allowed (`{ modifiers, key }`), but a custom shortcut does not appear in the panel.

From the manual (search summary, not opened):
- Enter runs the primary action (first action in the panel). Cmd+K opens the Action Panel. Inside the panel: Enter runs the selected action, Cmd+Enter the secondary, Option+Enter the tertiary, Esc closes the panel or backs out of a submenu.
- Conflict: the search bar page says Cmd+Enter is the secondary action, while the Translate command makes Cmd+Enter primary. Read this as a per-command override.

### 3.8 Interaction patterns worth borrowing

- **Primary action on Enter**, with secondary on Cmd+Enter. The user never has to open a menu for the common case.
- **Action Panel with sections**: group related actions (for example all copy actions in one section).
- **Submenus** with an ellipsis label for second-level choices.
- **Push and pop** for nested screens, with `onPush` and `onPop` callbacks.
- **Toast with actions** (Undo, cancel, copy stack trace) and an **Animated** toast updated to Success or Failure. This covers long operations.
- **Toast-to-HUD fallback** when the window is closed.
- **Pagination** with a host-driven `onLoadMore`.
- **Quick Look** toggled by an action.
- **Search bar dropdown** (`List.Dropdown`, opened with Cmd+P), used for filtering a list by category.
- **Tags with onAction** in metadata: tags are clickable filters.
- **Form drafts** (`enableDrafts`), restored from launch props, except secret fields.
- **Empty view with action**: an empty state that offers the next step (create item) instead of just saying nothing matched.

## 4. Cross-cutting comparison

| Aspect | Slack Block Kit | Adaptive Cards | Raycast |
|---|---|---|---|
| Unit | Flat list of blocks, max 50 per message | Body tree of elements in containers and columns | Whole views: List, Grid, Detail, Form |
| Look controlled by | Slack (semantic styles only) | Host, via HostConfig (named scales and color roles) | Raycast (no config exposed) |
| Size and spacing | None exposed | Named scales with host px values | None exposed |
| Versioning | API-level; no schema version found | Explicit `version`, `requires`, `fallback`, `fallbackText` | Per `@raycast/api` version |
| Unknown content | Not documented | Dropped; warning to host | Not applicable (typed API) |
| Plain-text fallback | Top-level `text` | `fallbackText` | Not applicable |
| Actions | Buttons, overflow menu, confirm object | Action.Submit / OpenUrl / ShowCard / ToggleVisibility / Execute | ActionPanel with sections and submenus |
| Primary action | Button style `primary` | Teams: 1 to 3 primary, up to 6 | First action; automatic Enter |
| Empty / loading | Not documented | Not documented | EmptyView; isLoading; no empty flash |
| Keyboard | Not documented | Not documented | Enter, Cmd+Enter, Cmd+K |
| Numeric limits | Many (text, counts, blocks) | Few stated | Grid columns, pagination size |

## 5. Ideas for us

Our vocabulary is: Column, Row, Grid, Card, Heading, Text, Markdown, Code, Badge, Alert, Divider, Image, Button, Form, Input, TextArea, Select, Checkbox, CheckboxGroup, RadioGroup, Switch, List, Table, Chart, Tabs. Source tags: **[AC]** Adaptive Cards, **[SB]** Slack Block Kit, **[RC]** Raycast, **[own]** our inference, not from any source.

### Components and props we are missing

1. **KeyValue / Facts component.** `items: [{label, value}]`, max 10 entries, label width capped. [AC] FactSet (label max width 150px default); [SB] section fields max 10; [RC] Detail metadata Label. Fills the gap between Text and Table.
2. **EmptyState.** `title`, `description`, `icon`, optional `actions`. An `empty` slot on List, Grid, and Table. Never shown while loading. [RC] List.EmptyView and its loading rule.
3. **Loading state as a host concept.** `loading: true` on List, Grid, Table, Form, and Chart. Rule: the model must not emit an empty List before its items; the host shows a loader while items stream in. [RC] "Don't return an empty list before data"; ties to our streaming invariants.
4. **List item accessories.** Typed array: `text`, `icon`, `date`, `tag`, `tooltip`, with a host-enforced max (we pick the number; the Raycast number is unverified). [RC] List.Item.Accessory; accessories are not on Grid items.
5. **List item keywords** for host-side search. Default filter on title and keywords; `filtering: false` when the model handles search itself. [RC] List.Item keywords and filtering.
6. **Pagination on List and Grid.** `pagination: {hasMore, pageSize, onLoadMore}`, with page size guidance (about 20). [RC] pagination.
7. **Action model.** Each action: `label`, optional `icon`, `style: regular | destructive`, and a `shortcut` picked from a fixed enum rather than free text. Primary and secondary come from position, not from a flag. [RC] ActionPanel and style; [AC] Action.Submit style; [SB] button style.
8. **Menu (overflow) component.** Sections and one level of submenu. Rule: submenu labels end in an ellipsis. Cap actions per surface. [RC] ActionPanel sections and submenu; [SB] overflow menu; [AC] maxActions default 5.
9. **Confirm on destructive actions.** `confirm: {title, message, confirmLabel, style: destructive}` attached to a Button. Dismiss must have no side effects. Optional "do not ask again" only for non-destructive flows. [RC] Alert and dismissAction, rememberUserChoice; [SB] confirmation dialog object.
10. **Feedback events (Toast).** `notify` with `title`, optional `message`, `variant: progress | success | failure`, and up to two actions (for example Undo). A progress notice that the host later updates to success or failure. Placement and dismissal belong to the host. [RC] Toast styles and actions.
11. **Form field `error` prop.** Validate on blur, clear on change, and block submit while any field has an error. [RC] Form validation rules.
12. **Input label and hint as first-class props.** Placeholder never replaces the label. Lint rule: an input with a placeholder but no label is an error. [AC] input label (1.3) and placeholder warning; [SB] input block label and hint; [RC] placeholder rule.
13. **Image requirements.** Required `alt`; `size` as a named enum (small, medium, large); `fit: contain | fill`; no decorative padding. [SB] alt_text; [AC] altText and imageSizes; [RC] Grid fit.
14. **Grid constraints.** `columns` 1 to 8; `aspectRatio` from a fixed enum (1, 3/2, 2/3, 4/3, 3/4, 16/9, 9/16); `fit` enum. [RC] Grid props.
15. **Text tone and weight as semantic enums.** `tone: default | subtle | accent | good | warning | attention`, `weight: lighter | default | bolder`, `size: small | default | medium | large | extraLarge`, `maxLines`. [AC] TextBlock properties. Remove any color or size values the model could set directly.
16. **Heading that means heading.** Our Heading should emit a heading role for assistive tech, not just bigger text. [AC] TextBlock style `heading` (1.5).
17. **Table options.** `firstRowAsHeader` (default true), `showGridLines`, column widths as px, weight, or auto/stretch, and cell alignment defaults. [AC] Table. Decide whether cells may hold nested blocks (AC allows a Table inside a cell; that is a complexity risk for us). 
18. **Column width rules.** Precedence: fixed px, then weight, then auto, then stretch. Rule: never set auto on every column; at least one column stretches. [AC] renderer spec and Teams design guidance.

### Host config knobs to add

19. **Spacing scale by name.** `gap` / `spacing` on Column, Row, Card, Section takes `none | small | default | medium | large | extraLarge | padding`. The host maps names to pixels. [AC] HostConfig spacing (defaults 3, 8, 20, 30, 40, 20; a starting point, not a requirement).
20. **Font size and weight scales by name.** Host-owned mapping; model picks names only. [AC] fontSizes (10, 12, 14, 17, 20) and fontWeights (200, 400, 800).
21. **Color roles by name.** `default, dark, light, accent, good, warning, attention`, each with a default and a subtle variant; container styles `default, emphasis, good, attention, warning, accent`. Model never emits hex. [AC] foregroundColors and containerStyles; principle 8.
22. **Separator as a flag on any block.** Keep Divider, but allow `separator: true` on Row, Column, Card, and List items. Host sets line thickness and color. [AC] separator and SeparatorConfig.
23. **Interactivity flag.** `supportsInteractivity: false` renders the document read-only: Buttons and Inputs are shown disabled or omitted. Useful for transcripts and previews. [AC] supportsInteractivity.
24. **Actions layout config.** `maxActions` (default 5), `orientation` (horizontal or vertical), `alignment`, `spacing`. [AC] ActionsConfig.
25. **Disclosure component** with a host-chosen mode: inline toggle or popup. Model asks for "collapsible"; host decides how. [AC] Action.ShowCard mode and Action.ToggleVisibility; [SB] overflow menu for lesser-used options.

### Versioning and fallback

26. **Document version and fallback text.** Every document declares `version`. If the host does not support it, render the top-level `summary` text (plain, markdown subset) and emit one diagnostic. Matches our one-diagnostic-per-recovery rule (AGENTS.md section 5). [AC] version and fallbackText; renderer spec MUST.
27. **Unknown component handling.** Drop the unknown subtree (do not bubble up), keep rendering siblings, and report one warning. [AC] "MUST be dropped" and "drop" fallback.
28. **Element `fallback` and `requires`.** Optional: `fallback: drop | <component>` and `requires: {feature: minVersion}` so new components degrade to a simpler component instead of vanishing. [AC] fallback and requires (1.2).
29. **Plain summary on every document.** A required `summary` string used for notifications and screen readers. Screen readers read only the top-level text in Slack. [SB] top-level text; desktop notifications pull from blocks first and fall back to text.
30. **Show formatting errors, do not hide them.** When a text function or binding fails, show the raw expression in the output (in dev mode) so the author sees it. [AC] formatting-failure rule. Tension: this conflicts with a polished end-user view; decide per surface. Marked [AC] with our own caveat.
31. **Keep behavior stable across versions, or version it explicitly.** Example: Action.Submit input scope changed between AC 1.x and 2.x. Our forms should define input scope once and freeze it per document version. [AC].

### Rules for the design doc and the prompt

32. **Model picks names, host picks values.** Everything visual is a semantic enum: spacing, tone, size, weight, columns, aspect ratio, image size. No px, no hex, no fonts. [AC] principle 8.
33. **Restricted markdown, rich text preferred.** Our Markdown component takes a documented subset (bold, italics, links, lists, inline code). No images, raw HTML, or tables inside Markdown. Prefer structured Text spans when the model is generating content. [SB] rich text over markdown; [AC] TextBlock markdown subset and RichTextBlock with no markdown.
34. **Hard text limits with clamp-and-diagnose.** Start from Slack's numbers: header text 150; button label 75; block text 3000; input label and hint 2000. Clamp and emit one diagnostic. [SB] verified limits; the numbers are a starting point for us. [own] on the exact values.
35. **Document size cap per surface.** Slack caps messages at 50 blocks and modals at 100. We set a cap per surface (chat transcript vs full-window). Above the cap, the model uses Tabs or a list instead. [SB] block caps; [own] on our numbers.
36. **Button labels stand alone.** Lint rule: no bare "OK", "Yes", "Click here"; destructive labels name the thing ("Delete draft"). [SB] "cancel subscription" example.
37. **Title and subtitle rules.** Titles are short nouns or verb-noun phrases, Title Case, no articles. Subtitles add context and are never a second description. [RC] store naming and subtitle rules.
38. **One primary action per group, max three per card.** [AC] Teams 1 to 3 primary, up to 6; [SB] design guidance on overflow for lesser-used actions.
39. **No emoji as controls.** Emoji may appear in text; never as a button or as the only label. [SB] design guidance.
40. **No more than three large flashes per second.** Relevant to streaming patches: patch in place, never remount a whole subtree on each token. [SB] flash rule; [own] on streaming application.
41. **Stale interactive state.** When a form or flow is done or expired, the host disables its controls and leaves a text record. [SB] update message after flow ends.
42. **Keyboard model fixed by the host.** Primary on Enter, secondary on Cmd+Enter, panel on Cmd+K, Esc to close. The model may request a shortcut only from a fixed enum, never free text. [RC] keyboard model; [own] on restricting it.
43. **Selection tap targets.** Only one tap target per row or card (a row-level action). Nested tap targets confuse hit testing and screen readers. [AC] selectAction on several element types; [own] on limiting to one.
44. **Action results are data, not code.** Actions name an intent and carry a typed payload. The host decides what submit means and whether to call a service. [AC] principle 1 and Action.Submit; [RC] SubmitForm onSubmit callback.

## 6. Unverified and open items

- Raycast "do not use more than N accessories": not found in any page I read.
- Raycast HIG: 404 at the path I tried; I could not find a live replacement. The best-practice and guideline pages were used instead.
- Raycast Grid.Item accessory: docs conflict (migration note vs props table).
- Raycast Cmd+Enter: search summary conflicts between the search bar page (secondary) and the Translate page (primary).
- Raycast Action.Style.Cancel: listed with no description.
- Raycast TextArea `enableMarkdown` shortcut set: only Cmd+B and Cmd+I were named in the summary.
- Slack static select cap of 100 options: search summary, not opened; a legacy page says nested options share one 100-option total. Confirm before relying on it.
- Slack section `expand` and header `level` (1 to 4): seen in fetch output, not confirmed on the reference pages.
- Slack Block Kit: unknown-block handling, nesting limits, total characters per message, and the new block types (Card, Plan, Task card, Data table, Markdown, Video, Carousel) were only seen on the index; not read in detail. Slack time picker description reads oddly ("numerical data"); likely a docs error.
- Slack desktop notification change (2026-07-13): search summary only.
- Adaptive Cards: fallback and versioning pages returned 404; behavior taken from the renderer spec, element docs, and a search summary.
- Adaptive Cards: `spacing: none` is in the schema enum but has no HostConfig entry; behavior unverified.
- Adaptive Cards: Table `firstRowAsHeader` (documented) vs `firstRowAsHeaders` (used in examples); check against the parser.
- Adaptive Cards: Carousel, Badge, Icon, CodeBlock, Rating, ProgressBar, Charts: versions and schema status not verified.
- Adaptive Cards: Teams mobile and Outlook mobile version support: community Q&A and an AI-written answer only.
- Adaptive Cards: `maxActions` overflow behavior (dropped or moved) not stated in pages read.
- Our own numbers in section 5 (accessory cap, text limits, block caps, spacing values) are proposals, not sourced.

## 7. Sources

Slack Block Kit (docs.slack.dev):
- https://docs.slack.dev/block-kit/
- https://docs.slack.dev/block-kit/designing-with-block-kit
- https://docs.slack.dev/reference/block-kit/blocks/
- https://docs.slack.dev/reference/block-kit/blocks/section-block
- https://docs.slack.dev/reference/block-kit/blocks/actions-block
- https://docs.slack.dev/reference/block-kit/blocks/header-block
- https://docs.slack.dev/reference/block-kit/blocks/input-block
- https://docs.slack.dev/reference/block-kit/block-elements/
- https://docs.slack.dev/reference/block-kit/block-elements/button-element
- https://docs.slack.dev/reference/block-kit/block-elements/checkboxes-element
- https://docs.slack.dev/reference/block-kit/block-elements/select-menu-element/ (search summary)
- https://docs.slack.dev/reference/block-kit/composition-objects/text-object.md
- https://docs.slack.dev/reference/block-kit/composition-objects/ (index)
- https://docs.slack.dev/messaging/formatting-message-text/ (search result only)
- https://docs.slack.dev/changelog/2026/07/13/notification-changes/ (search summary only)

Adaptive Cards:
- https://adaptivecards.microsoft.com/explorer/ (redirected from adaptivecards.io/explorer/)
- https://learn.microsoft.com/en-us/adaptive-cards/schema-explorer/adaptive-card
- https://learn.microsoft.com/en-us/adaptive-cards/schema-explorer/text-block
- https://learn.microsoft.com/en-us/adaptive-cards/schema-explorer/table
- https://learn.microsoft.com/en-us/adaptive-cards/rendering-cards/host-config
- https://learn.microsoft.com/en-us/adaptive-cards/rendering-cards/implement-a-renderer
- https://learn.microsoft.com/en-us/adaptive-cards/resources/principles
- https://learn.microsoft.com/en-us/microsoftteams/platform/task-modules-and-cards/cards/design-effective-cards (legacy, live)
- https://learn.microsoft.com/en-us/adaptive-cards/authoring-cards/input-validation (search result only)
- https://learn.microsoft.com/en-us/adaptive-cards/authoring-cards/text-features (search result only)
- https://adaptivecards.microsoft.com/?topic=design-best-practices (referenced, not fetched)

Raycast:
- https://developers.raycast.com/api-reference/user-interface/list
- https://developers.raycast.com/api-reference/user-interface/grid.md
- https://developers.raycast.com/api-reference/user-interface/form.md
- https://developers.raycast.com/api-reference/user-interface/detail.md
- https://developers.raycast.com/api-reference/user-interface/actions
- https://developers.raycast.com/api-reference/user-interface/action-panel.md
- https://developers.raycast.com/api-reference/feedback/toast.md
- https://developers.raycast.com/api-reference/feedback/alert.md
- https://developers.raycast.com/basics/prepare-an-extension-for-store.md
- https://developers.raycast.com/information/best-practices.md
- https://manual.raycast.com/extensions-guidelines
- https://manual.raycast.com/keyboard-shortcuts (search result only)
- https://manual.raycast.com/action-panel (search result only)
- https://developers.raycast.com/misc/migration/v1.31.0 (search result only)
