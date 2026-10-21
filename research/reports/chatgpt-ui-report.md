# How ChatGPT produces and renders rich / generative UI

Investigated 2026-10-21 on chatgpt.com (Plus account, model slug `gpt-6-thinking`, web UI), by driving the real signed-in Chrome with 8 test prompts and reading the SSE stream (a fetch tee installed from page JS) and the DOM. Read-only inspection otherwise. All quoted snippets are shortened.

## 1. Summary

ChatGPT's current web UI no longer treats an answer as "markdown plus a few special cards". **Every assistant message is compiled into a UI tree** called DIL (name seen in `model_dil_v2`, `DilRenderer`, `DIL.render`, `DIL execution sandbox`). Three mechanisms feed that tree:

1. **Model-authored layout DSL embedded in the text stream.** The model writes markdown with JSX-like tags: `<box>`, `<row>`, `<title>`, `<text>`, `<icon>`, `<grid>`, `<divider>`, `<form>`, `<radio-group>`, `<checkbox>`. Weather, stock, currency and laptop-form "widgets" are built this way from primitives. They are not server-defined widget types.
2. **Named high-level components with data bound by reference.** `<MapWidgetV2 points={[{ref:"turn..business3"}]}>`, `<SoccerGames game_ids={[...]}/>`, `<Entity ref=.../>`, `<AsyncImage ref=.../>`, `<Rating/>`, `<Cite refs=.../>`, `<Link>`, `<SuggestAutomation label=.../>`, `<WritingBlock>`. The model supplies ids and labels. The server resolves the real payload (lat/long, scores, images) from tool results and pushes it as side metadata.
3. **Client-defined widgets extracted from the markdown by range.** Code blocks, math, charts and writing blocks are `content_references` of type `client_defined_widget` (`code_block`, `writing_block`, `charts_widget_v2`). They carry `start_idx`/`end_idx`/`matched_text` into the text, so the text stays a readable fallback.

Charts are a native interactive Recharts component (not an image), driven by a JSON spec the model writes inline. Forms are real stateful UI: the model writes JS state hooks and an `onSubmit` that issues a new chat turn. Canvas has been replaced by an inline editable "Writing" block. Image requests went to image generation, not image search. A timer is a scheduled-task card, not a live countdown.

## 2. Streaming protocol observations

- **Transport:** `POST /backend-api/f/conversation`, response `text/event-stream`. First line `event: delta_encoding` / `data: "v1"`. This marks a delta-compressed JSON-patch dialect. Sampled events carried `c` sequence counters.
- **Event kinds seen** (`data:` JSON): `resume_conversation_token` (resumable stream), `input_message`, `title_generation`, `message_marker` (`cot_token`, `user_visible_token`, first/last), `url_moderation`, `server_ste_metadata`, `message_stream_complete`, `conversation_detail_metadata`, `streaming_parent_patch` (re-parents continuation messages when one answer is split across several messages).
- **Patch ops:** `{"p":"/message/content/parts/0","o":"append","v":"..."}`, with multi-op arrays in `{"o":"patch","v":[...]}`. Later deltas omit `p`/`o` and inherit the last path. Op types seen: `add`, `append`, `replace`, `remove`, `truncate`. Text is append-only. Metadata is patched by JSON pointer.
- **Whole-message envelope:** an initial `{"p":"","o":"add","v":{"message":{id, author, content:{content_type, parts}, metadata,...}}}`. Then deltas. Content types seen: `text`, `model_editable_context`, `thoughts`, `reasoning_recap`, plus tool-call messages addressed by `recipient` (for example `->web.run`, and a code-typed message to an automation `.create` tool for the timer).
- **The UI tree is streamed beside the text.** Under `/message/metadata/model_dil_v2/` there are:
  - `code`: compiled JS/JSX, `DIL.render(__dil.jsx(()=>{...}))`, using `__dil.jsx("title",{size:"lg"},__dilConstants["0"])`. It is **re-sent whole (`replace`) each time it grows**, about 6-7 times for a short answer.
  - `constants`: a map `{ "2":"Rust", "3":" - Fast..." }` of string literals, **appended** incrementally. Code references them by index. Text is lifted out of the program, so the code is mostly structure.
  - `fallbackMarkdown`: the plain-markdown version, also patched.
  - `clientDefinedWidgets`: `{<uuid>:{status:"resolved", refIndex:0, isStreaming:false}}`.
  - `requiredComponents`, `appData.opGenui {componentResults, interactionSurface:"mainline", modelDataBindings}`, `appDirectiveSequenceActive`.
  - `recoveryDiagnostics`: `[{code:"unterminated_tag", action:"recovered_parse", line, column}]`. The server runs a **tolerant incremental parser** over the half-written model text on every delta and records where it had to auto-close (`unterminated_tag`, `unterminated_braced_value`). That is how a half-streamed `<box ...` still renders.
- Other metadata patches: `content_references` (appended, with `matched_text`, `start_idx`, `end_idx`, `safe_urls`), `genui_components` (`[{type:"charts_widget_v2", tree_range:[1,2], start_index, end_index}]`), `view_state/widgets` (resolved payloads, for example the map points).
- **Cadence:** about 15 to 140 SSE chunks per answer. A 3-section markdown answer streamed in about 8 s, with the first visible token at about 1.7 s. Charts, maps and forms arrive as ordinary text deltas of JSON or JSX. The component appears when its tag or JSON closes.
- **Not everything is on the SSE stream.** For the image request the SSE closed after about 3.5 s with `message_stream_complete`. The UI then showed a dot-matrix "Adding final touches 95%" progress and, about 60 s later, the image. The client polled `GET /backend-api/conversations/{id}` several times (see the resource timing). Async tools therefore use stream-then-poll.
- **Streaming markdown is not rendered by a client markdown parser alone.** The server turns text into the tree and the client renders the tree. The DOM root for text is `DilRenderer DilResponseRoot Reveal` containing `MarkdownRoot`, with `Reveal` suggesting an animated reveal on new nodes.

## 3. Per-test findings

| # | Prompt (short) | UI that appeared | Streaming | Underlying representation |
|---|---|---|---|---|
| 1 | Markdown demo (H2, bullets, table, Rust code) | Normal prose, bullet list, styled table (`Table`/`TableFrame`), code block with language label, copy and wrap buttons | Text appended in small chunks; the DIL `code` was replaced as it grew; constants appended | Markdown text in `parts[0]`, mirrored as DIL tags `title`, `text`, `list`, `list-item`, `bold`, `table`, `table-row`, `table-cell{header}`, `code`. The code block is a `client_defined_widget` (`code_block`, `language:"rust"`) |
| 2 | Bar chart of 5 country populations | Native interactive chart (Recharts: `recharts-responsive-container`, hover tooltip, share and "..." menu), then a table, a conclusion and a source caption with cite chips. 0 `<img>`; no image | Used web search first (`web.run`), "Working for 11s", then streamed | Model wrote `<Chart content={{chartType:"bar", meta:{title,description}, xKey:"country", series:[{dataKey,label,valueSuffix}], yAxisMin:0, data:[{...}]}}/>` inline. Server rewrote it to a `genui` marker (`charts_widget_v2`) plus `genui_components`. Data lives in the spec, not in a Python image |
| 3a | Tokyo weather now | Bordered card: title row, big temperature, icon, high / low / rain-chance grid, alert line | Text then card, the card streaming as JSX text | Pure primitives: `<box border radius="xl" padding={3} gap={2}><row ...><text color="secondary" size="xs">`, `<title size="2xl" tabularNums>`, `<icon name="cloud-sun" .../>`, `<grid>`, `<grid-item>`, `<divider/>`. Model authored. Data came from `web.run` |
| 3b | AAPL price | Card with price, red change, premarket quote, 52-week range, source line, publisher chips, a "Yahoo Finance" link | Same | Same primitives. Honest about staleness ("could not verify a newer live price"). No special stock widget was needed |
| 3c | Places on a map | Interactive vector map (canvas, WebGL-like), rating pins, expand button, then rows with photo, rating, "6-8 minutes walk" | "Working for 30s", map streamed in mid-answer | `<MapWidgetV2 title points={[{ref:"turn..business3",label:"Duck ramen"}]} carousel={{visibility:"hidden"}}/>`. Server resolved refs into `view_state/widgets` with name, lat, long, address, category. Rows use `<Entity ref>`, `<AsyncImage ref aspectRatio="1:1">`, `<Rating>` |
| 3d | Premier League scores | Score rows (crest, score, "Full-time / Oct 17", crest) in a divided list, then bullets | Same message | `<SoccerGames game_ids={["72221330",...]}/>`. The model passes ids; the server and client fill in the rest. This is the one "special" card among these, and it is a named component with ids |
| 3e | Currency conversion | Three rows with flag, name, rate and big amount | Text-like | Primitives again (`box`, `row`, `title`, `text`, `divider`, `icon`) |
| 3f | 5-minute timer | A task card "Five-minute timer, Oct 21 7:46 PM, Open", plus "Timer started" text | Tool-call message, JSON `{"title":"Five-minute timer","prompt":"..."}` streamed as `/message/content/text` | It is a **scheduled automation/task** (an `.create` tool; `story_events`), not a client countdown widget. Note: this created a real scheduled task in the account (see Limitations) |
| 4 | "Open a canvas, draft README" | The reply said canvas is "no longer available", then an inline editable document block labelled "Writing" with copy and expand | Block streamed live (`isStreaming` flag), then resolved | Model emitted a directive `:::writing{variant="document" id="58321"} ... :::` and the server turned it into `<WritingBlock id variant="document">` plus a `client_defined_widget` (`writing_block`) reference. A side-panel canvas surface is gone in this UI |
| 5 | + menu / Plugins | + menu: Add photos and files, Add from library, Create image, Add browser tabs, Web search, Work in a project, Deep research, Sketch, GitHub, Chrome. Sidebar "Plugins" page ("Connect plugins to let ChatGPT work across your tools"), tabs Plugins / Skills; the "Apps" name is now "Plugins" | n/a | Hundreds of third-party plugins (Gmail, Drive, Canva, Figma, Spotify, Stripe, Booking.com, ...). I did not enable any |
| 6 | Laptop picker, "clickable choices or a form" | A real form: radio group (budget), checkbox group with icons (use), size choice, submit | `{@body ...}` directives first, then JSX | `{@body const [budget,setBudget] = DIL.useState("...")}`, `{@body const budgets = [...]}`, `<form gap={4} onSubmit={()=>GenUI.issueNewTurn("Help me pick a laptop. My budget is "+budget+"...")}>`, `<radio-group value onChange>`, `<radio>`, `<checkbox>`. So model-authored JS with state and an action that **sends a synthetic user turn** |
| 7 | Pictures of Shiba Inu puppies | One generated image (4-photo collage with infographic text), "Edit" and share buttons, dot-matrix progress | SSE ended in 3.5 s; UI polled; image about 60 s | Routed to **image generation**, not image search. No carousel observed |
| 8 | Citations (from 2, 3b, 3d) | Source chips ("Investing.com +1", "Reuters +1") inline after sentences | Chips arrive with the sentence | `<Cite refs={["turn347284search4", ...]}/>` in the text; `content_references` carry the URLs; `<Link>` for inline links. `<caption>` for figure source lines |

## 4. Rendering architecture inferences

- **Three-layer pipeline.** (a) The model writes one text stream: markdown plus JSX-ish tags plus `{@body ...}` directives plus `:::name{}` fences plus private-use-delimited `genui` markers. (b) A server-side incremental compiler parses it (tolerantly), splits string literals into a `constants` table, emits `code` (JS calling `__dil.jsx`), emits a `fallbackMarkdown`, and extracts `content_references` and widget payloads. (c) The browser renders from that.
- **Sandboxed execution.** The page contains one hidden `iframe` titled "DIL execution sandbox", `sandbox="allow-scripts"` only (no `allow-same-origin`), from `cdn.platform.openai.com/deployments/dil/v14/runner.html` (versioned runner). So model-derived code (hooks, `useState`, handlers) runs in an opaque-origin iframe, and the main page renders the resulting element tree with its own native React components (`TextBase`, `Title`, `Box`, `Table`, `Divider`, `Pressable`, ...). The host never `eval`s model code. Events and state cross a message boundary. `DIL.useAppData(appData => appData.opGenui...modelDataBindings)` is how data is passed in. `DIL.useConstants()` is how strings are passed in.
- **Closed vocabulary.** Tag names map to a fixed component catalog, with design tokens as props (`size="2xl"`, `color="secondary"`, `radius="xl"`, `padding={3}`, `gap={2}`, `tabularNums`). Unknown tags would have nowhere to go. A `requiredComponents` field suggests lazy loading of heavier components (maps, charts).
- **Data binding by reference.** The model never emits coordinates, scores or image URLs for tool-backed widgets. It emits `ref` / `game_ids` that point at entities produced by tool calls in the same turn (`turn516955business3`, `turn347284search4`). The server joins them. This keeps tokens low, avoids hallucinated data, and lets the client lazy-load media.
- **Server-defined widgets vs model-authored UI is both.** Primitive-built cards (weather, stock, currency, forms) are model-authored. Maps, charts, sports, code, writing, entities and rating are server/client-defined components. The model picks and configures them.
- **Interactivity back-channel.** `GenUI.issueNewTurn(text)` turns UI events into a follow-up user message. That is the whole action model I saw (`SuggestAutomation` is a similar affordance). No arbitrary side-effecting calls from the widgets were observed.
- **Apps SDK model (inferred, not tested).** Plugins are a catalog of connectors. The structure suggests a tool returns structured data (the `componentResults` / `modelDataBindings` slots) and a UI template renders it. Not verified because enabling a plugin was out of bounds.
- **Resilience built in.** `fallbackMarkdown` for every message, `recoveryDiagnostics` for malformed streams, a versioned runner (`v14`), and `contract`-style versioning (`delta_encoding v1`, `fallbackMarkdownVersion:1`).

## 5. Comparison with a DSL approach like openui

| Aspect | ChatGPT (observed) | openui-style compact DSL |
|---|---|---|
| Authoring form | Markdown + verbose JSX (`<box border radius="xl" padding={3} gap={2}>`) + JS directives | A purpose-built compact language, can be far terser |
| Tokens | High for custom layouts. The weather card is roughly 40 lines of nested tags. ChatGPT mitigates with `ref` binding, named high-level components, and moving strings into a constants table, but custom cards stay expensive | Lower per card if tag and prop names are short and tokenizer-friendly; defaults inferred |
| Flexibility | Very high. Arbitrary layouts from primitives, plus state hooks and handlers | Bounded by the DSL; add escape hatches deliberately |
| Safety | Good: closed component set, code executed only inside an opaque-origin iframe, host renders natively. But "JS in the model's output" is still a large surface and needs the sandbox | A declarative language with no code is simpler to make safe by construction; no sandbox needed |
| Determinism | Moderate: same prompt gives different layouts; styling depends on model discipline with tokens | Higher if the grammar is strict and a validator rejects bad output |
| Streaming | Needs a tolerant incremental parser and recovery diagnostics, as seen. Whole-program re-send on each growth is wasteful | Design for prefix-parseable output, with append-only nodes |
| Fallback | `fallbackMarkdown` always present | Define one explicitly |

## 6. Ideas to adopt for a Rust + GPUI generative-UI system, and things to avoid

Adopt:
- **A closed, tokenised component catalog** (layout primitives plus a few domain components such as chart, map, table, code, form) with design-token props, not raw styles. The model composes. The renderer is native GPUI.
- **Reference-based data binding.** Tool results get ids. The model emits ids. The core resolves them. Keeps output short and facts grounded.
- **Server-side (core-side) tolerant incremental parser** producing a stable tree on every prefix, with auto-close and a diagnostics list. Render partial trees. Keep node identity stable so GPUI can diff and animate reveals.
- **Separate text from structure:** extract string literals into a constants table and keep the structure small. Send append-only node and constant ops, not a full re-send.
- **Patch protocol:** JSON-pointer ops (`add`/`append`/`replace`/`remove`) over SSE with sequence numbers and a resume token. Good model for a Rust event stream.
- **Content-reference ranges** (`start_idx`/`end_idx` into the text) so rich widgets can degrade to the plain text.
- **Always ship a plain fallback** (markdown or a minimal text tree) and a versioned renderer contract.
- **A single typed action channel** (like `issueNewTurn`) for UI events. State for forms stays client-side and is submitted as one structured event.
- **Async tool state** (progress UI plus poll or push) for slow results such as images.

Avoid:
- Executing model-authored JS. ChatGPT needs an iframe sandbox for it. In a native Rust core there is no equivalent cheap sandbox, so keep the language declarative with a small expression subset (or a WASM-sandboxed evaluator if state logic is needed).
- Re-sending the whole compiled program on every growth step.
- Letting the model invent data for tool-backed widgets.
- Free-form styling, and a large undocumented vocabulary. The model drifts (e.g. `gridxs` and `<S` showed up as stray tags in the extracted text, and the server needed recovery for unterminated tags).
- Overloading the markdown stream with several syntaxes (JSX, `{@body}` directives, `:::` fences, private-use `genui` markers). ChatGPT has four. One consistent syntax is easier to prompt and parse.

## 7. Limitations of this investigation

- One account, one model (`gpt-6-thinking`) and one UI build; features appear to vary by account and rollout. 8 prompts only. Outputs depend on the web-search results of that moment.
- I tee'd the SSE stream by wrapping `window.fetch` after page load; I summarised event shapes rather than reading every event. Several payload views were redacted by the tool for content that looked like URLs/tokens, so some snippets are reconstructed from filtered slices.
- No plugin (Apps SDK) widget was exercised, so the Apps SDK part is inference only. The `/plugins` page was viewed read-only.
- The DIL runner iframe and the host components were seen only from outside (DOM, class names); I did not decompile the runner, so the exact message protocol between iframe and host is unknown.
- Image search and carousels were not triggered (the image prompt went to image generation). No Python/code-interpreter chart was triggered because the model chose the native chart.
- **Side effect to be aware of:** the timer prompt created a real "Five-minute timer" scheduled task in the user's ChatGPT account (it fires at about 7:46 PM IST on 2026-10-21). I did not delete it (deletion is outside my rules). The test chats created (Markdown Demo, 2023 Population Bar Chart, Tokyo Weather Apple Stock, Ramen And Premier League Scores, Five-minute timer, Draft uitool README, Laptop Recommendation Form, Shiba Inu Pictures Facts) remain in the chat history, and the Shiba image used image-generation quota.
