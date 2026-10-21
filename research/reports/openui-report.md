# OpenUI (thesysdev/openui) Study Report

Prepared for a clean-room Rust reimplementation (Rust core + GPUI renderer). All descriptions are paraphrased. Paths are relative to `research/openui/` unless noted. The spec files (`spec/overview.md`, `spec/language.md`, `spec/prompt.md`) are the authoritative description. The TypeScript in `packages/` is one implementation of that spec.

Repository layout, briefly:

- `spec/`: three prose documents plus 54 conformance cases.
- `packages/lang-core`: the framework-free core, about 8k lines.
- `packages/react-lang`: the React renderer.
- `packages/react-ui`: the component library.
- `packages/react-headless`: chat state and stream adapters.
- Sibling renderers: `vue-lang`, `svelte-lang`, `angular-lang`.
- Other packages: `a2ui`, `server`, `langchain`, `assistant-ui`, `openui-cli`, `react-email`, `browser-bundle`, `devtools`, `observability*`.
- `benchmarks/`: token-count comparison.

---

## 1. Purpose and mental model

**Problem.** An LLM should produce interactive UI, not just text. JSON trees and raw HTML are verbose and stream badly.

**Solution.** OpenUI Lang is a small line-oriented language. The model writes one statement per line, each of the form `name = expression`. The expressions are mostly component calls with positional arguments. The client parses the text on every streamed chunk and draws a UI that is always valid.

**Three roles** (`spec/overview.md` intro):

- The **model** writes the program.
- The **client** parses, evaluates and renders it.
- The **host** is the app around the client. It calls the model, supplies tools, and stores messages.

**What the model produces.** Typically a `root = Card([...])` line first, then children by name, then leaf data. Names can be used before they are defined (hoisting), so structure streams in before content. Programs may also contain:

- `$state` variables.
- Expressions: arithmetic, ternary, member access.
- Built-in `@Functions`.
- `Query(...)` and `Mutation(...)` statements bound to host tools.
- Actions, such as `@ToAssistant`, `@Set` and `@Run`, attached to buttons.

**What the client does:**

1. Strips any host marker lines and any prose or fences.
2. Splits the text into statements and parses them into an AST.
3. Maps positional arguments onto named props using the component library's schema.
4. Validates, dropping the smallest broken unit.
5. Resolves references starting at `root` to build a tree.
6. Evaluates reactive expressions against a state store and query results.
7. Renders the tree with native components.
8. Turns user interaction into state writes, tool calls, or "events" delivered to the host. The most important event, `continue_conversation`, becomes the next user turn.

**Design goals** (`spec/overview.md` 1.2): few tokens (the benchmark in `benchmarks/README.md` reports about 47-53% fewer tokens than YAML, JSON-patch and a normalized JSON tree on seven sample UIs); streaming by default; forgiving; interactive without model round trips.

**Chat versus edit.** In a chat app a click becomes a new message, and the model answers with a new program. On an editing surface the model gets the current program and replies with only the changed statements, which the host merges (section 6).

---

## 2. The language

Authoritative: `spec/language.md` (sections 1-3, 5-6). Implementation: `packages/lang-core/src/parser/{lexer,tokens,statements,expressions,ast,builtins}.ts`.

### 2.1 Source preparation (`language.md` 1.2; `parser/parser.ts` fence/comment stripping)

In order:

1. If the text contains triple-backtick fences, only the fence contents are kept (joined by newlines) and surrounding prose is ignored. Backtick runs inside double-quoted strings do not close a fence. An unterminated fence runs to the end of the text.
2. `//` and `#` line comments are stripped, but not inside strings.
3. The result is trimmed.

The lexer never fails. Unknown characters are skipped.

### 2.2 Statement model

- A program is a sequence of `target = expression` statements.
- The target is an identifier or a `$name`.
- A newline ends a statement only when three things hold. Bracket depth is zero. The line is not inside an open string (strings may contain raw newlines). And no ternary is "open": a `?` has been seen but its `:` branch has no complete operand yet, or the next line begins with `?`.
- Lines that do not look like `name = ...` are silently skipped.
- Statements are classified in order:
  1. Whole right side is `Query(...)`: query statement.
  2. Whole right side is `Mutation(...)`: mutation statement.
  3. `$` target: state declaration (the right side is the default).
  4. Anything else: value statement.
- Duplicate names: the later one wins. While streaming, a half-closed pending duplicate never replaces a completed one.
- Classification is done in `parser/parser.ts`. The statement splitter is `parser/statements.ts`.

### 2.3 Informal grammar

```
program      := statement (NEWLINE statement)*
statement    := (identifier | $state) "=" expression
expression   := ternary, with the usual JS-like precedence ladder
                ?:  <  ||  <  &&  <  == !=  <  > < >= <=  <  + -  <  * / %  <  unary ! -  <  postfix
postfix      := primary ( "." field | "[" expression "]" )*
primary      := string | number | true | false | null
              | array "[" args "]"  | object "{" key ":" expr ... "}"
              | call  | reference | "(" expression ")"
call         := (ComponentName | "@"Name) "(" args ")"
args         := expression ( ","? expression )*     -- commas optional, trailing commas ignored
```

Lexical points worth noting:

- Identifiers are `[A-Za-z_][A-Za-z0-9_]*`. An uppercase first letter in call position means a component. A bare identifier in operand position is always a reference, whatever its case.
- `$name` is state. `@Name` is a function or action call.
- Keywords are only `true`, `false` and `null`.
- Strings are double-quoted (JSON escapes) or single-quoted (a smaller escape set).
- A single `&` or `|` is read as `&&` or `||`.
- A `-` is a number sign only when a digit follows and the previous token is not a value. So `[1 -2]` is two elements and `1 - 2` is subtraction. Numbers allow exponents. A `.` belongs to a number only if a digit follows.
- Object keys may be names, strings, numbers, component names or `$` names, and all become string keys.

### 2.4 Components and positional arguments

- `Button("Save", saveAction, "primary")` maps by position onto the prop names the library lists in order.
- Optional trailing arguments can be omitted. `null` skips an optional one in the middle.
- Extra arguments are dropped with an `excess-args` error.
- Component slots take a component, a reference, or an array of them.
- The prompt tells the model never to use `key: value` syntax for arguments.

### 2.5 Hoisting and entry

- A reference to any statement is legal anywhere in the program.
- A name with no statement yet is **unresolved**. That is a status, not an error. It evaluates to null in expressions, renders nothing as a child, and is omitted from arrays.
- Cycles are treated as unresolved at the point they close.
- A statement referenced from two places is evaluated separately at each. The tree is copied, the state store is shared, and a query referenced twice still fetches once.
- **Entry** is the statement named `root`. If there is none, it is the first value statement whose value is a call to the library's declared root component (a non-fatal `no-root` warning). Otherwise the stream-end result is a fatal `no-root`.
- **Orphans** are value statements that `root` cannot reach. They are reported in metadata and not rendered.

### 2.6 Expressions and evaluation semantics (`language.md` 3.1-3.3)

- Values are strings, numbers, booleans, null, arrays, objects and component instances.
- Coercion follows ECMAScript. `toNumber` maps unparsable strings to 0 (NaN never surfaces). Truthiness and loose equality follow JS rules. The spec says non-JS clients must reproduce ECMA-262 `ToNumber`, `IsLooselyEqual` and `Number::toString` exactly.
- `+` concatenates if either side is a string (null becomes empty). Division by zero gives 0. `&&` and `||` short-circuit and return the deciding operand.
- Member access on an object reads the field. On an array it **plucks** that field from every element (`rows.amount` is the list of amounts). `.length` is the count. Null propagates. Numeric `.0` is a field name, not an index. `[i]` indexes.

### 2.7 Built-in `@` functions (`language.md` 3.4-3.5; `parser/builtins.ts`)

| Group | Functions |
| --- | --- |
| Aggregation | `@Count`, `@Sum`, `@Avg`, `@Min`, `@Max`, `@First`, `@Last` |
| Reshaping | `@Filter(array, field, op, value)` with ops `==`, `!=`, `<`, `>`, `<=`, `>=`, `contains`; `@Sort(array, field, dir)`, stable, numeric if both sides look numeric, otherwise locale collation |
| Math | `@Round`, `@Abs`, `@Floor`, `@Ceil` |
| Iteration | `@Each(array, "varName", template)`, which is lazy |

- Field arguments accept dot paths such as `"customer.name"`.
- `@Each` evaluates the template once per element. The loop variable shadows same-named statements. There is no index. The element is bound before deferred evaluation, so an action inside the template captures its own element.
- Built-ins are defined with the same `defineFunction` machinery that libraries use for custom functions (via Zod schemas). Names are reserved.

### 2.8 State bindings (`language.md` 5)

- `$x = default` declares state. An undeclared `$x` that is referenced is auto-declared with null.
- State is a flat map scoped to one rendered program.
- Everything reactive: any statement that reads a `$var` or a query result re-evaluates when it changes. `$` only marks state that the user or an action writes.
- **Two-way binding.** Passing a `$var` directly to a prop that the library marks as bindable (printed `$binding<type>` in the signature) makes the component display the value and write changes back. Elsewhere the `$var` evaluates to its current value.
- Re-parsing never overwrites a value the user produced and never deletes keys. Defaults only fill unset or still-pristine entries.

### 2.9 Forms (`language.md` 5.3-5.4)

- Fields group under the nearest enclosing "form" component. Fields outside any form use an unnamed page scope.
- Validation rules are an object on input components: `required`, `email`, `url`, `numeric`, `minLength`, `maxLength`, `min`, `max`, `pattern`. Every rule except `required` skips empty values. The first failing rule gives the field's error. A form submits only when every field passes.

### 2.10 Query and Mutation (`language.md` 6.1-6.2; `runtime/queryManager.ts`)

- `q = Query(toolName, argsObject, defaultValue, refreshSeconds?)`.
- `m = Mutation(toolName, argsObject)`, run only by `@Run(m)`.
- Both are statement forms only. Inline use in a value position reports `inline-reserved`.
- Naming a query `$x` still runs it, but `$x` in an expression reads the state store, not the result. The prompt warns against this.

### 2.11 Actions (`language.md` 6.3)

- Built-in actions: `@ToAssistant(message, context?)`, `@OpenUrl(url)`, `@Set($v, expr)`, `@Reset($a, $b)`, `@Run(ref)`.
- `Action([a, b, ...])` sequences them. A single action needs no wrapper. A bare list of actions is just data.
- `@Run`, `@Set` and `@Reset` name their targets instead of evaluating them. `@Each` is deferred the same way.
- A library can declare custom actions (`@CopyToClipboard(text)`). They are validated and delivered to the host as events, never executed by the client.
- **Execution order.** Actions run in order. A mutation run is awaited, and a failed mutation halts the remaining actions. Query refetches and host events are fire-and-forget. Form state attached to each event is read when that step runs, so an earlier `@Set` is visible.

---

## 3. Streaming

Spec: `language.md` section 4 and Appendix A. Implementation: `createStreamParser` in `packages/lang-core/src/parser/parser.ts`. Fixtures: `spec/fixtures/1.0/streaming/`.

**Algorithm in words:**

1. Each chunk is appended to a buffer. The buffer is re-prepared (fences and comments stripped).
2. A scan from a watermark finds newly completed statements. Statement boundaries use the bracket, string and ternary rules from section 2.2. A newline is provisional until the next line's first non-space character has arrived, because that line might start with `?` and continue a ternary.
3. Completed statements are parsed once and cached. They never change.
4. The remaining **pending tail** is re-parsed on every chunk after *implicit closing*:
   - An open string is closed. If the text ends right after a backslash, a backslash is added first.
   - Open brackets are closed in reverse order.
   - A non-matching close bracket is ignored.
5. A pending statement that still fails to parse is simply not produced.
6. A pending statement that needed closing never replaces a completed statement with the same name.
7. If the cleaned text no longer starts with the cached completed prefix, the cache is discarded and parsing restarts. The spec's example is a code fence that appears late and shifts what is stripped.
8. The host signals end of stream (`isStreaming` going false in React). That finalizes the pending tail, so closed statements become completed, including redefinitions. The language itself has no end marker.
9. A `$name` at the very end of the buffer may still be growing, as in `$d` of `$dishes`. The implementation skips auto-declaring it until the stream ends.

**Guarantees:**

- A valid render exists after every chunk. An unresolved element in an array is omitted, with no placeholders.
- At stream end the result **must equal a batch parse** of the full text: statements, entry, errors and state defaults. The conformance runner checks both the streamed and the batch result for every streaming case.
- Required-prop errors caused only by unresolved references are not reported while streaming. `no-root` and enum checks wait for the end.
- Queries and mutations never run while streaming. State declarations initialize as they arrive, and a default read from a truncated statement is later replaced unless the user changed it (the store's "pristine" flag, `runtime/store.ts`).
- Interactive components are disabled while streaming. The React Button sets `disabled` from `isStreaming`.

**Cost note.** Completed statements are cached, but `buildResult` re-materializes the whole tree (symbol table, reference resolution, validation, orphan tracking) on every chunk. Per-chunk work is therefore proportional to program size rather than chunk size. It is a full reparse of the tail plus a full re-resolve of everything. React then re-evaluates props and reconciles.

---

## 4. Validation and error recovery

Spec: `language.md` section 8. Implementation: `parser/validation.ts`, `parser/materialize.ts`, `parser/enrich-errors.ts`, `parser/types.ts` (error types). Fixtures: `spec/fixtures/1.0/errors/`.

**Principle: drop and render.** A mistake removes the smallest unit and everything else renders:

- An invalid argument degrades the prop.
- An invalid component is dropped.
- An invalid statement is dropped.
- Nothing the model writes may crash the host. Every removal is reported.

**Recovery per code** (the error table is in `language.md` 8.2):

| Code | Recovery |
| --- | --- |
| `unknown-component` | Component dropped. In a computed expression it remains but renders nothing |
| `missing-required` / `null-required` | Schema default if one exists, otherwise the component is dropped |
| `type-mismatch` | Schema default if any. Otherwise an optional prop or array item is removed, and a required prop drops the component. Bare values where a list is needed also mismatch, and data in a component-only slot is rejected |
| `excess-args` | Extras dropped, component renders |
| `inline-reserved` | `Query` or `Mutation` in value position evaluates to nothing |
| `unknown-function` | Evaluates to null, statement kept |
| `no-root` | Fatal if no entry. Warning if the fallback entry renders |
| `parse-failed`, `parse-exception` | Fatal. The client must catch exceptions |
| `runtime-error` | The offending prop becomes null |
| `render-error` | Component error boundary keeps the last good render |
| `tool-not-found`, `tool-error`, `mcp-error` | Query keeps defaults or last data. Mutation result carries the error |

**Error object** (`language.md` 8.3): `source` (parser, runtime, query or mutation), `code`, `statementId`, `component`, `path` (a JSON-pointer-like prop path), `message`, `hint`, an optional `toolName`, and `severity: "warning"` for non-fatal cases. The hint is a short signature with required props starred, or the list of available names for an unknown name.

**Feedback to the LLM.** The structure is designed to be pasted into the next request (`spec/prompt.md` 6.2). Because the model can answer with a patch (section 6), the loop is: report errors, then the model sends a one-line fix, which is merged into the stored program. Emission rules:

- Clear errors when a stream starts.
- Emit on change only.
- Signal recovery with an empty list.

The code comment in `types.ts` notes that transient or network errors are excluded from the correction loop. `Renderer`'s `onError` doc says to retry data-source failures instead of regenerating.

---

## 5. Component library model

Spec: `spec/prompt.md` section 2 (LibrarySpec), section 3 (rules). Implementation: `packages/lang-core/src/library.ts`, `extend.ts`, `functions.ts`, `signature.ts`, `parser/validate-library.ts`.

### 5.1 How a library is defined (TypeScript)

- Each component is declared with `defineComponent({ name, props, description, component })`. `props` is a Zod v4 object schema whose **key order is the positional order**. The `component` field is opaque to lang-core and is the framework adapter's renderer.
- `createLibrary({ components, componentGroups?, functions?, actions?, root?, id? })` assembles them. It rejects any name that collides with a built-in, a reserved call or another component.
- A library offers `prompt()`, `toSpec()` and `toJSONSchema()`.
- A prop marked `reactive(...)` is a bindable prop. `lang-core/src/reactive.ts` just tracks which schemas carry that mark. The schema itself cannot carry it. Only the printed signature (`$binding<string>`) does, and that is where a consumer learns it.

### 5.2 Serialized LibrarySpec (a single JSON document; `prompt.md` section 2)

| Field | Meaning |
| --- | --- |
| `id`, `version` (optional) | Metadata for the `library=` attribute on stored messages. They do not alter parsing. `toSpec()` emits `id` only. |
| `root` | Optional root component name, used by the fallback-entry rule |
| `schema` | JSON Schema. `properties` lists component names (the positional ordering list). `$defs` has one entry per component, and per action referenced by a prop. Each has `properties` (ordered), `required`, per-prop `default`, `description`. `functions` and `actions` hold `params` and `returns` schemas. |
| `components` | Per component: a printed `signature` and a `description`, derived from the schema. The schema wins on disagreement. |
| `componentGroups` | Optional list of `{ name, components, notes? }` that split the prompt's component list into titled sections |
| `functions`, `actions` | Signature and description of custom functions and actions |

How schemas reference each other:

- A component-typed prop is a `$ref` to a `$defs` entry, or an `anyOf` of such refs. Lists put this in `items`.
- Action slots use `$ref` to a shared `ActionExpression`, or an `anyOf` of refs to specific action names. In 1.0 the restriction affects only prompt and schema, not runtime acceptance.
- A reader classifies a `$ref` by name. Names found in top-level `properties` are components. Names in `actions`, plus `ActionExpression`, `OpenUrl` and `ToAssistant`, are actions.

Signature printing format (`prompt.md` 5.1): `Name(prop: type, opt?: type, ...)`. Types print as `string`, `number`, `boolean`, `any`, enums as quoted alternatives, arrays as `T[]`, inline objects as `{f: type}`, unions with `|`, bindings as `$binding<type>`, restricted action slots as `@Action | @Other`. Implementation: `signature.ts`.

Library rules (`prompt.md` 3):

- Required props before optional ones.
- Component, function and action names start with an uppercase letter.
- Names must not collide with built-ins.
- `root` and group members must exist.
- Custom functions must be pure and synchronous.

### 5.3 Groups, extension, custom functions and actions

- **Groups** are presentation for the prompt only. Unlisted components fall under "Other". Notes are free-form usage tips that are prompt text (the React UI library carries many of them in `packages/react-ui/src/genui-lib/openuiLibrary.tsx`).
- **Extension.** `library.extend({ components: { add, override, remove }, functions: { add }, actions: { add } })` returns a new library and leaves the base unchanged. Adding a component can name "slots" (`"Parent.prop"`) so it joins those unions. Overriding rewires every referring slot. Removing drops the component from unions and groups. Removing the root, or the only type a prop accepts, throws. Adding an existing name throws. See `lang-core/src/extend.ts`.
- **Custom functions** (`defineFunction`): name, description, a Zod `params` object (positional by key order), `returns`, and a pure `fn` that receives one object keyed by param names. The function is never serialized. Each client implements it. Arguments are checked like component props: literals at parse time and runtime values at call time. Failures yield null with a `runtime-error`.
- **Custom actions** (`defineAction`): the same shape without `returns`. The client does not run them. The host receives an event whose `type` is the action name and whose `params` hold the arguments by name.

### 5.4 Backward compatibility (`overview.md` section 3, `prompt.md` 3.2)

Stored programs have no prop names, so these rules are essential:

- Append props only at the end.
- Never reorder or remove a prop.
- Give any new required prop a default.
- New built-in names are reserved.

---

## 6. Prompt generation, message protocol, edit mode

### 6.1 Prompt generation (`parser/prompt.ts`; `prompt.md` section 4)

The prompt is a pure, deterministic function of the LibrarySpec and options. The same inputs give byte-identical output across platforms. Options: `toolCalls`, `bindings`, `builtinFunctions`, `editMode`, `inlineMode`, `tools` (names or rich descriptors with input and output schemas), `preamble`, `examples`, `toolExamples`, `additionalRules`.

**Order of sections** (from `generatePrompt`, which `generateSystemPrompt` wraps):

1. Preamble: tells the model its whole response must be OpenUI Lang.
2. Syntax rules, in order:
   - Statement shape.
   - `root` as the entry.
   - Expression kinds.
   - Reference usage.
   - A strong warning that unreferenced variables are dropped.
   - Positional-only arguments (with an explicit warning that colon syntax silently breaks).
   - Omitting optional args from the end.
   - Optionally state and expression rules, depending on flags.
3. Component signatures, one line each (signature, em dash, description), grouped with group notes and an "Other" bucket.
4. Built-in function list. It is printed only if tools or bindings are on, or `builtinFunctions` is set. Custom functions always print.
5. Query and Mutation sections, if tool calls are enabled.
6. Action section, if any component uses action expressions, or alone if only custom actions exist.
7. Interactive-filters pattern, if both tools and bindings are on.
8. Tool workflow guidance and the tool list, with default-value hints from output schemas.
9. Hoisting and streaming guidance: a recommended statement order of root, then state, then queries, then components, then leaf data.
10. Examples (static and tool examples).
11. Edit-mode section (patching, deletion, patch size guidance), if enabled.
12. Inline-mode section (prose plus fenced code, text-only answers to questions), if enabled.
13. Important rules, followed by additional rules.

Design principle (`prompt.md` 1): the prompt must not teach features the target client lacks. Capability flags gate sections.

### 6.2 Conversation and events (`prompt.md` 6)

- History contains the model's earlier programs as text.
- Marker lines are stripped before anything reaches the model.
- Error lists may be fed back.
- An action that continues the conversation becomes a user turn made of three parts: the human message, the arbitrary `context` value, and the form state read at action time. The reference format is a content marker followed by a context marker holding a JSON array.

### 6.3 Message protocol (`prompt.md` 7; `packages/lang-core/src/message.ts`)

A stored or streamed response is plain text wrapped in marker lines that start at column 0 with a fixed prefix (`]]>openui:` plus a kind):

- `content` (optionally with URL-encoded attributes such as `library=id@version`) opens the program section.
- `context` opens a host-defined JSON section, for example form state or click context.
- `end` is a one-line marker meaning the stream completed. A stored message without it was cut off, but it still renders.

Reader rules:

- The last `content` section wins. This handles a regenerated reply appended after a cut-off one, and a mid-line content marker starts a new line.
- Text before the first marker is content if there is no content marker.
- Unknown attributes are ignored. Unknown kinds are stripped.
- While streaming, a last line that is or may become a marker is held back.
- The model never sees or writes markers. The party that builds the model request strips them.

### 6.4 Edit mode (`language.md` 7; `parser/merge.ts`)

The host sends the current program with the request. The model replies with only the changed statements, and the host merges by name after the patch has fully streamed:

- Same name replaces. New names are appended. Untouched names stay.
- `name = null` at top level in a patch deletes the statement (state declarations included, though not their store keys).
- After the merge, statements `root` cannot reach are garbage collected. Exceptions: state declarations, anything referenced from query or mutation args, and the case where there is no `root` statement.
- If the reply is prose plus a fenced patch, only the fenced part is merged.
- The merge must use the same boundary rules as the parser, including multi-line ternaries.
- **Observation.** `merge.ts` imports the tokenizer and `split`, but has its own bracket-and-string scanner for statement splitting. In the file as read, that scanner shows no ternary-continuation handling. The spec demands byte agreement across the three splitters. I did not run this, so treat it as a reason to test, not a confirmed bug. A Rust port should have exactly one splitter used by all three.

---

## 7. Runtime

Implementation: `packages/lang-core/src/runtime/` (`evaluator.ts`, `evaluate-prop.ts`, `evaluate-tree.ts`, `store.ts`, `queryManager.ts`, `toolProvider.ts`, `mcp.ts`, `state-field.ts`) and the glue in `packages/react-lang/src/hooks/useOpenUIState.ts`.

**Evaluation.**

- The AST has these node kinds: literals, array, object, reference, placeholder (unresolved), state ref, runtime ref (query or mutation), binary op, unary op, ternary, member, index, assign, and call.
- Parsing lowers the program into a tree of "element nodes". Each has a type name, a statement id, a props map of values or still-unevaluated AST nodes, a `partial` flag, and a `hasDynamicProps` flag so fully-static subtrees skip evaluation.
- Evaluation walks that tree each time state or query results change. It resolves state reads, query results via a reference resolver, and calls to builtins, custom functions and actions.
- Action calls evaluate to an **action plan**: an ordered list of steps (run, to-assistant, open-url, set, reset, custom). The plan is data held by the component until a click.
- Evaluation errors are caught per prop. The prop falls back and a `runtime-error` is recorded.

**Reactive store** (`store.ts`).

- A flat map plus listeners and an immutable snapshot, in the style of an external store.
- It has a "pristine" set so declared defaults can be replaced while the program is still being streamed, but user-set or persisted values never are.
- `initialize(defaults, persisted)` never deletes keys.
- Form fields are stored under the form name as `{value, componentType}` wrappers. Page-level fields are stored under their own name.
- The React layer subscribes with `useSyncExternalStore` for both store and query manager.

**Forms.**

- Fields call `useStateField`/`useFormValidation` hooks (`react-lang/src/hooks`). `resolveStateField` in lang-core decides whether the field is bound to a `$var` ("reactive assign" marker) or is a form field.
- The Button runs form validation before firing actions that would send data (ToAssistant, custom actions, mutation runs). This is component behavior, not core.
- Validators live in `lang-core/src/utils/validation.ts`. The `pattern` rule builds a regex from model-supplied text.

**Query and mutation lifecycle** (`queryManager.ts`, spec 6.1-6.2).

- Queries run when streaming ends and again when a `$var` written literally in their args changes. Dependencies are computed at parse time by walking the args AST.
- A stable-stringified cache key of tool, args and deps decides refetch.
- During a refetch the old result stays visible. Results for stale keys are discarded. Refresh intervals are timers.
- Reserved snapshot keys `__openui_loading`, `__openui_refetching` and `__openui_errors` expose fetch state to the host only. Expressions cannot read them.
- Mutations are registered, then fired through `@Run`. Status goes `idle`, `loading`, `success` or `error`. A second run during `loading` is rejected. Args are evaluated at fire time.
- A `@Run` on a query invalidates and refetches. It never halts the action plan.

**Tool provider.** The interface is a single async `callTool(name, args)`. The React `Renderer` accepts either a plain map of async functions or any MCP-client-shaped object. MCP responses are unwrapped by preferring `structuredContent`, then JSON-parsing text content, then raw text, and `isError` throws. A missing tool raises a specific error carrying the available tool names, which becomes `tool-not-found` with a hint. Lookup is case-sensitive.

**How actions reach the host or assistant.** `triggerAction` in `useOpenUIState.ts` walks the plan step by step. Each step either mutates the store, calls the query manager, or emits an event through the host's `onAction`. The event has `type`, `params`, `humanFriendlyMessage`, optional `formName` and `formState`. Types are `continue_conversation`, `open_url`, or the custom action name. `react-headless` (chat store plus adapters for OpenAI, Vercel AI SDK, AG-UI and LangGraph streams) is the usual consumer, turning `continue_conversation` into a new user message.

---

## 8. Renderer contract

What `react-lang` takes from the core (`packages/react-lang/src/Renderer.tsx`, `hooks/useOpenUIState.ts`, `context.ts`, `library.ts`):

1. `library.toJSONSchema()` goes to `createStreamingParser(schema, library.root)`.
2. Each render calls `streamParser.set(fullText, !isStreaming)`. The parser diffs against its buffer and resets if the text was replaced.
3. The result carries these fields:
   - The root element node, or a runtime-chosen root expression.
   - Metadata: unresolved names, orphans, errors, an incomplete flag.
   - State declarations.
   - Query statements and mutation statements, as ASTs.
4. The renderer owns a store and a query manager, builds an evaluation context (read state, resolve refs, custom function table, action schemas), and evaluates the tree.
5. Then it recursively renders element nodes by looking up the registered renderer by type name.

**What a component receives.** In React it gets `props` (already evaluated values), a `renderNode(value)` function for rendering child values (elements, arrays, primitives), and its `statementId`. Children are ordinary props that hold element nodes or arrays of them, so containers call `renderNode(props.children)`. Primitives render as text. Interaction goes through hooks: `useTriggerAction`, `useFormName`, `useIsStreaming`, `useStateField`, `useFormValidation`, `useOpenUI`. Bound props arrive as a marker object, and `useStateField` resolves it to a value and a setter.

**How re-rendering happens during streaming.** The text changes, so `useMemo` re-runs the parser, then re-evaluates the tree, then React reconciles. Store and query changes re-render through `useSyncExternalStore`. Each element is wrapped in an error boundary that keeps the last good children on failure and reports `render-error`. Query loading shows a dimmed container and a spinner.

**Evidence the core is renderer-agnostic.** `vue-lang`, `svelte-lang` and `angular-lang` are separate renderers over the same core. The spec says each platform writes its own components and decides binding and form behavior (`prompt.md` 2.6).

**What a native GPU renderer must supply:**

- The library as a set of component implementations. This includes the schema (prop order, types, defaults, required flags, which props are bindable), a description for prompting, and a draw/layout function per component.
- A way to render child values: elements, arrays and scalars, with null and unresolved rendering nothing.
- A driver that, on each text update, runs the stream parser and then evaluation, and diffs the tree against the retained UI tree. That is the part React gives for free.
- A form and state scope model. The nearest form name must be exposed to descendants, and the fields must be wrapped as `{value, componentType}` entries.
- The action pipeline and a host event sink.
- Disabling of interactive controls while streaming.
- An error boundary equivalent: catch panics or layout failures per component and keep the last good element.
- A loading and dimming affordance for query state.
- A `ToolProvider` implementation (and optionally MCP).
- Timers for query refresh.
- A text and rich-text subsystem, since many components hold markdown or inline text.
- Icon lookup (the React library uses lucide by kebab-case name).
- Charts.
- Theming.

---

## 9. Conformance suite

Location: `spec/fixtures/1.0/`. Guide: `spec/fixtures/1.0/README.md` plus one README per area. Spec cross-reference: `language.md` Appendix B.

**Structure.** `library.json` is the shared test LibrarySpec (root `Stack`, twelve components, a custom function `@Upper` and a custom action `@CopyToClipboard`). Each area has folders `NNN-short-name/`. The areas and their case counts:

| Area | Cases | Covers |
| --- | --- | --- |
| `grammar` | 7 | Fences, comments, escapes, minus and number lexing, precedence, member digits, serialization round trip |
| `entry` | 7 | Root rules, fallback entry, ternary roots |
| `streaming` | 4 | Chunked input, multi-line ternary, late fence, truncated escape |
| `evaluation` | 9 | Statement kind, ref cycles, duplicates, coercion, pluck, sort and locale, custom functions |
| `state` | 5 | Auto-declare, two-way binding, form scope, validation rules |
| `actions` | 10 | Multiple actions, event context, query rerun, mutation, tool-not-found, custom actions |
| `editing` | 5 | Replace/add, delete and GC, state delete, fenced patch, no-root |
| `errors` | 12 | Each error code and the error object shape |

Fixture ids are not contiguous. The streaming README notes that 002 and 004 are unused. The suite holds only cases the reference implementation passes.

**Case files:**

- `input.oui` is the program, or `chunks.json` is an array of strings for streaming.
- `expected.json` is the expected result.
- `patch.oui` is for editing cases.
- `interactions.json` is for action and form cases.
- `validation.json` is for rule cases.

**`expected.json` describes the post-stream result:**

- `root`: the rendered tree summary. Each component is `{type, props}` with null or absent props omitted, actions summarized as `{actions:[{action,args}]}`, and bindings as `{$binding: "$name"}`. Values come from evaluating at stream end with queries unresolved (defaults) and mutations in the idle shape.
- `errors`: compared as a multiset of `code` and `statementId`. An entry without `statementId` matches any. Optional `errorObjects` check `source`, `code`, `statementId`, `component`, `path` only. Message and hint are never compared.
- `unresolved` and `orphans`: compared as sets.
- `incomplete`: boolean.
- Optional `state` and, for editing, `statements` (names in merged order).

**Two levels.** Level 1 needs a parser and evaluator only. Level 2 adds `interactions.json`, a list of `{click: id}`, `{type: [id, text]}` and `{tool: name, result: json}` steps. It expects host events (compared on listed keys), final state, and optionally the tool calls made. Reference runners: `packages/lang-core/src/__tests__/conformance.test.ts` and `packages/react-lang/src/__tests__/conformance.interactions.test.tsx`. The `OPENUI_FIXTURES` environment variable redirects them.

Gaps: the changelog and fixtures list say several areas have no fixtures. No fixtures exist for `@Each`, message-protocol parsing, prompt output, LibrarySpec shape or library extension, which the spec marks as "checked on the library".

---

## 10. Component inventory

Source: `packages/react-ui/src/genui-lib/openuiLibrary.tsx` (general UI, root `Stack`) and `openuiChatLibrary.tsx` (chat flavour, root `Card`). Low-level React widgets live in `packages/react-ui/src/components/`.

The two libraries are not identical. The chat library adds `FollowUpBlock`/`FollowUpItem`, `SectionBlock`/`SectionItem`, and a `sources` prop on Card. The general library (root `Stack`) adds `Stack`, `Modal`, `Form` layout pieces and similar items the chat library omits (for example `Separator` placement differs).

**Layout:** `Stack` (row/column flex with gap, align, justify, wrap), `Tabs`/`TabItem`, `Accordion`/`AccordionItem`, `Steps`/`StepsItem`, `Carousel`, `Separator`, `Modal` (bound open flag).

**Content:** `Card`, `CardHeader`, `TextContent` (sized text), `MarkDownRenderer` (markdown with math), `Callout`, `TextCallout`, `Image`, `ImageBlock`, `ImageGallery`, `CodeBlock` (syntax highlighted), `InlineHeader`.

**Tables:** `Table` (column-oriented, with `Col`), `EditableTable` (inline cell editing with typed columns).

**Charts, 2D:** `BarChart`, `LineChart`, `AreaChart`, `RadarChart`, `HorizontalBarChart`, `Series`.

**Charts, 1D:** `PieChart` (including donut), `RadialChart`, `SingleStackedBarChart`, `Slice`.

**Charts, scatter:** `ScatterChart`, `ScatterSeries`, `Point`.

**Forms:** `Form`, `FormControl`, `Label`, `Input`, `TextArea`, `Select`/`SelectItem`, `DatePicker`, `Slider`, `CheckBoxGroup`/`CheckBoxItem`, `RadioGroup`/`RadioItem`, `SwitchGroup`/`SwitchItem`, `Chips`/`ChipItem` (pill selection), `OptionCards`/`OptionCard` (large selectable cards).

**Buttons:** `Button`, `Buttons` (button row), `IconButton`.

**Data display:** `Tag`, `TagBlock`, `Icon` (lucide by name), `EntityList` (two-column rows), `ListBlock`/`ListItem`.

**Card blocks:** `SnippetCardBlock`/`Item`, `OverviewCardBlock`/`Item`, `ContextCardBlock`/`Item`, `CompositeCardBlock`/`Item`, `VisualCardBlock`/`Item`.

**Inline building blocks for cards:** `Text`, `BoldText`, `IconText`, `ImageText`, `ImageTextLarge`, `MetricIndicatorInline`, `MetricIndicatorWithStrikethrough`.

**Chat-specific:** `FollowUpBlock`/`FollowUpItem`, `SectionBlock`/`SectionItem`, and a sources strip.

**Other packages.** `packages/react-email` has 44 email building blocks. `packages/react-ui/src/components/` also contains chat chrome (`OpenUIChat`, `AgentInterface`, `ModelSwitcher`, `ToolCall`, `ToolResult`, `Citation`, `Sources`), a theme provider, and `DotMatrixLoader`, none of which are model-facing.

**Hard on a native renderer:**

- **Charts.** The React library draws with d3-scale, d3-selection and d3-shape. Rust needs its own scales, axes, legends, tooltips, hover and responsive layout. It also needs an area, radial, radar, scatter and stacked-bar vocabulary. This is the largest bespoke effort.
- **Tables.** `@tanstack/react-table` underlies `Table` and `EditableTable`. Needs virtualization if large, column sizing, and per-cell editors of several types (text, number, date, select, url). The `Col` cells can themselves hold components.
- **Markdown and math.** react-markdown with GFM, KaTeX math, emoji and soft breaks. Needs a markdown parser, inline layout, and math rendering.
- **Code block.** Syntax highlighting needs a tokenizer such as tree-sitter or syntect.
- **Date picker and calendar.** Locale-aware date logic.
- **Rich popovers and menus.** Select, tooltips, dropdowns and modals depend on Radix and floating-ui for positioning, focus trapping and a11y. GPUI would need its own overlay layer.
- **Image gallery and carousel.** Async image loading, caching, aspect-ratio handling and gestures.
- **Icons.** lucide icons by kebab-case name. Either embed an SVG set or restrict the list.
- **Accessibility.** Radix gives much for free on the web.
- **Easier:** `Stack`, `Card`, `TextContent`, `Button`, `Tag`, `Separator`, `Callout`, `EntityList`, `Tabs`, `Accordion`, `Steps`, `Input`/`TextArea`/`Select`/`Slider`/`Switch`/`Checkbox`/`Radio`.

---

## 11. Strengths, weaknesses and critique

### Keep

- **Statement-per-line, hoisted references.** This is what makes streaming top-down and cheap. Statement boundaries are cheap to detect incrementally. The tail-only reparse with implicit closing is simple and robust.
- **Drop-and-render error policy** with structured, hint-carrying errors designed for the LLM loop. It treats error handling as part of the product.
- **Spec plus conformance fixtures** as the contract, with a "streaming equals batch" invariant and an `expected.json` that summarizes semantic results, not internals. This is directly reusable as a Rust test suite. Aim to pass the 54 cases, since they are language-neutral.
- **Schema as the single source of truth** for validation, positional mapping, prompt signatures and hints. In Rust, a plain data model that serializes to the same LibrarySpec JSON is natural.
- **Deterministic prompt generation** from spec plus flags, and gating of prompt sections by client capability.
- **Separation of host events from execution.** The client runs state and query actions. Everything else goes to the host.
- **Action plans as inert data**, and a distinction between "named target" actions (`@Set`, `@Run`) and evaluated ones.
- **Stored messages as plain text with marker lines,** and the "last content wins" rule for retries.
- **Edit mode as a name-keyed patch** with reachability GC.
- **Backward-compatibility rules** for append-only props.
- **Renderer independence.** The Vue, Svelte and Angular bindings show the core can be separated cleanly.

### Change

- **Positional-argument fragility.** Position is the entire meaning of an argument. LLMs miscount or skip optional middle args, and any schema evolution risks silently remapping stored programs. The mitigation is append-only discipline (spec section 3), which is policy rather than mechanism. Options for Rust: keep positional as the compact default, but also allow optional named-argument syntax (the prompt currently forbids colon syntax and says it "silently breaks"). Store a library version hash with each message. Provide a checked `lint` mode. Consider emitting per-component arity in error hints.
- **Reparse cost.** Completed statements are cached, but the tree is rebuilt, re-validated and re-evaluated from `root` on every chunk, and in React the full response string is passed in and diffed. A Rust core could keep a persistent statement table with dirty tracking, only re-materializing nodes whose statement or transitive dependencies changed, and return a tree diff instead of a new tree. Render cost matters more on a GPU renderer if every frame rebuilds retained layout.
- **Zod and JSON Schema as the schema layer.** Weak typing for bindable props (the binding marker lives only in the printed signature, so a consumer must parse a string to learn it). Define one native schema type with explicit fields (`bindable`, `slot kinds`, `action slots`) and generate JSON Schema as an export.
- **Mixed responsibilities in the Button.** Form validation before actions lives in the component. Move validation gating into core action semantics so every renderer agrees.
- **Several different splitters** (stream scanner, tokenizer-based `split`, and merge's string scanner). Use one lexer and boundary routine for all.
- **Implicit auto-declaration of undeclared `$vars`** with null, plus "pristine" tracking, is subtle. Make state lifecycle explicit and testable.
- **ECMAScript semantics requirement.** The spec requires non-JS clients to reproduce `ToNumber`, `IsLooselyEqual`, `Number::toString` and `localeCompare` exactly. That is expensive in Rust (locale collation, float formatting) and rarely what a UI program needs. Consider a stricter subset: a typed comparison model, numeric-string rules defined in the spec, and byte-order or ICU-root collation. Document divergences and fixture them.
- **Locale-dependent `@Sort`.** Pins the output to the platform's collation. Define a deterministic collation.

### Avoid

- **Expression attack surface.** The language is an interpreter fed by model output, and sometimes by tool data. Concerns to design out:
  - Regex from `pattern` rules is built straight from model text (`lang-core/src/utils/validation.ts`), so catastrophic backtracking is possible. Use a linear-time regex engine such as Rust's `regex`, or reject patterns.
  - Member access on arbitrary objects: I found explicit own-property checks for function lookup but no guard for dangerous keys in plain member reads. This is a JS-specific risk and moot in Rust, but keep it in mind for dynamic maps.
  - Unbounded work: deeply nested expressions, huge `@Each` ranges, big arrays from tool results, ref-expansion blow-up (a shared sub-statement is evaluated separately at every use, so a DAG of references can expand exponentially). Add depth, node and evaluation-step budgets, with errors and not hangs.
  - Tool calls driven by model-written `Query`/`Mutation` with arbitrary tool names and args. The tool allowlist and confirmation policy belong to the host. Consider marking mutations as requiring user-initiated actions only (the spec already stops mutations from running on load).
  - `@OpenUrl` with arbitrary schemes needs a policy for what the host allows.
  - Model-controlled image and link URLs.
- **Web assumptions to drop:**
  - `localStorage` and DOM injection of a keyframes style tag in `Renderer.tsx`.
  - React-specific hook contracts in the "contract".
  - Radix-style compound components.
  - Prose notes in groups that mention CSS-ish layout ("wrap", "justify").
  - The `{value, componentType}` form wrapper, which is a React-library convention. The fixtures use it with the test library, so a Rust test harness will need to mimic it.
- **Unvalidated duplicated knowledge.** Prompt text bakes in library-specific examples (for example `Table` column-orientation, chart data shape) in `packages/react-ui`. Keep prompt examples with each component definition and test them by parsing them.
- **Chat concerns in the core path.** `humanFriendlyMessage` and `continue_conversation` assume a chat host. Keep them as one event kind among many.

### Observations on testing

- The conformance runner exists only for lang-core and React interactions. There is no fixture area for prompt output determinism, message protocol or library extension, so a Rust port will need its own tests there.
- The spec says fixtures are authoritative on disagreement, which is a good rule to adopt.

---

## 12. Open questions

1. **Layout model.** The library uses CSS-like semantics (flex-ish `Stack`, wrap, align, justify, gap tokens). What does GPUI need for equivalence: its own flexbox via taffy, or a smaller set of containers? Does the first native library reproduce the React one 1:1 (and its prompt), or define a new schema?
2. **Compatibility target.** Is the goal to consume programs and LibrarySpecs unchanged (full 1.0 conformance), or a new dialect? This decides whether to port the ECMAScript coercion rules.
3. **Where the tree diff lives.** Core returns a new tree each chunk. Should the Rust core return a keyed diff (by statement id) so the native renderer can retain widget state (scroll, focus, text-input cursor) across chunks? Statement ids are stable, which suggests a natural key. The `@Each` outputs have no keys.
4. **Focus and input state during streaming.** Controls are disabled while streaming, but a re-parse that replaces a component could drop in-progress edits. What identity rule keeps a text input alive when its statement is redefined?
5. **Data and persistence.** `react-lang/src/responseBundle.ts` mentions "cloud scripts" and a script revision tied to queries. This is a hosted-service feature (`packages/lang-core/src/cloud.ts`, `parser/cloud-config.ts`) that I did not examine in depth. It is unclear whether any of it is needed in the spec's 1.0 contract. Treat as out of scope unless confirmed.
6. **Tool trust model.** Who decides whether a model may call a mutation tool, and does the host see a pre-flight list of tools a program intends to use (derivable from query and mutation statements)?
7. **Rich text.** Which markdown subset, if any, does the native library support, given that `MarkDownRenderer` and `TextContent` carry much of the content in practice?
8. **Charts.** Build natively, embed a plotting crate, or ship a reduced chart set? The prompt notes show chart data is column arrays, which maps cleanly to a Rust data model.
9. **Accessibility.** What is the GPUI story, and does the spec need to require anything about it?
10. **Library packaging.** In Rust, can a LibrarySpec JSON be the *only* thing the model-facing side needs, with component implementations registered separately? The TypeScript implementation already separates the two in principle (`toSpec()` versus the renderer map).
11. **Unspecified behaviors.** The spec lists some items as implementation-defined and untested: trailing tokens after a complete expression, the value of a dangling exponent, a partial unicode escape under implicit closing, and refetch-after-merge behavior. Decide these explicitly in Rust.
12. **Multi-statement `Action` in `@Each`.** The spec says there are no fixtures for `@Each`. Its exact deferral semantics (element capture, shadowing) should be pinned with new fixtures before porting.
