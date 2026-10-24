# Launch and marketing channels for the generative-UI library

Research date: 2026-10-21. Scope: Rust core crate plus GPUI renderer (phase 1). Solo developer, small budget. Launch assumed in a few months.

## How to read this file

Each verification tag means:

- **[V]** Read on the official page during this research.
- **[S]** Seen only in a search snippet, a third-party guide, or a mirror. Confirm before relying on it.
- **[U]** Not verified. Either the page could not be fetched or no source was found. Treated as a guess until checked.

Reddit pages could not be fetched directly (the fetch tool refused reddit.com and old.reddit.com), so every subreddit rule below is [S] at best. Re-check each live sidebar and wiki before posting.

## Constraints that shape the whole plan

1. **Show HN needs something people can try.** Landing pages and sign-up walls do not qualify. The title must start with "Show HN:". The project must be something you can run, and you must be available to discuss it [V]. For this product, the sample host app (M8) or a runnable GPUI demo is the Show HN artifact. A bare library crate is a weak fit on its own.
2. **Lobsters blocks new accounts from self-promotion for 70 days.** New users cannot submit self-promo, send invites, or flag content, and they cannot use the `show` tag [V]. Get an account at least 70 days before launch, which means starting about 10 weeks out. Lobsters invites come from existing members, so start in chat early.
3. **Product Hunt bans company accounts and forbids asking for upvotes.** Launch from a personal profile. Ask people to visit and comment, not to upvote [V].
4. **Rust subreddit and Rust announcement rules are strict.** The r/rust mod rule (mirror, [S]) asks for a known Rust contributor, or code under an OSI or FSF license, and invites you to message the mods before posting self-promotion.
5. **This Week in Rust (TWiR) will not take LLM-written articles** [V]. Any launch write-up must be human-written. Machine translation is allowed.
6. **The best launch day is not verified.** One Show HN analysis points to Tuesday through Thursday [S]. Treat that as a hypothesis, not a rule.

## 1. Launch platforms

### Show HN / Hacker News
- **URL:** https://news.ycombinator.com/showhn.html and https://news.ycombinator.com/newsguidelines.html
- **Type:** Tech news aggregator, with a dedicated Show HN section.
- **Audience fit:** High. Systems programmers, Rust developers, and people building AI tools read it. A front-page hit drives the largest single spike a dev tool can get.
- **Rules [V]:**
  - Title starts with "Show HN:". Do not add uppercase, exclamation points, or editorializing.
  - Must be something people can run or hold. Blog posts, sign-up pages, newsletters, and lists are off topic.
  - Must be non-trivial, personally built, and ready to try, ideally without signup.
  - Features and upgrades generally do not qualify. A major overhaul probably does.
  - "Don't solicit upvotes, comments, or submissions." Asking friends to upvote is "not ok."
  - "Please don't use HN primarily for promotion."
  - The guidelines do not state any account-age or posting-frequency rules [V].
- **Best time:** Tuesday through Thursday is one analysis's hypothesis [S]. Post in the US morning. Verify with your own tracking.
- **Traffic claims:** A front-page Show HN post can convert at a high rate, and one vendor claims 10,000 to 50,000 visits in 24 hours [S, vendor claims]. Treat these as unverified.
- **Cost:** Free.
- **Effort:** Medium to high. Stay in the thread all day and answer every technical question.

### Product Hunt
- **URL:** https://www.producthunt.com/launch and https://www.producthunt.com/categories
- **Type:** Product launch board.
- **Audience fit:** Medium for a developer library. Product Hunt is broader and more consumer-leaning. Developer tools often get polite upvotes with few sign-ups [S].
- **Rules [V]:**
  - Launch from a personal account. "Company accounts are prohibited."
  - Launch time suggestion: 12:01 a.m. Pacific Time.
  - "You cannot ask people directly to upvote your product." Ask them to visit and comment.
  - Self-hunting is encouraged.
  - You may relaunch after significant new iterations.
- **Relevant categories [V]:** LLM Developer Tools, AI Agents, Prompt Engineering Tools, AI Workflow Automation, Code Editors, Command Line Tools, AI Infrastructure Tools.
- **Best time:** Choose the day you are most prepared, per the official page [V].
- **Cost:** Free to launch.
- **Effort:** Medium. Needs a gallery, a video under about 60 seconds [S], and a day of replies.

### Lobsters
- **URL:** https://lobste.rs/about and https://lobste.rs/t/rust
- **Type:** Invite-only, technical link aggregator. Quality-focused.
- **Audience fit:** High for the Rust and systems crowd. Smaller than HN but deeper.
- **How to get an invite [V]:**
  - The quickest route is to talk to someone you know from the site in chat. The full invite tree is public.
  - If you have an authored link posted on the site, reach out in chat.
  - If you know nobody, become a regular in the chat room first.
  - Users can invite after their first 70 days.
- **Rules [V]:**
  - Self-promo should be under about 25% of your stories and comments over the last year.
  - New users (first 70 days) cannot submit self-promo.
  - Relevant tags [V]: `rust` ("Rust programming"), `programming` (fallback when no tag fits). `ai` is not a tag. The `vibecoding` tag is described as "Using AI/LLM, coding tools. Don't also tag with `ai`." If the project was built with heavy AI assistance, disclose it and use `vibecoding` honestly.
- **Cost:** Free.
- **Effort:** Low to post, high to earn an invite. Start now.

### Product Hunt alternatives

**Peerlist Launchpad**
- **URL:** https://help.peerlist.io/individual/launchpad (help center, [S] for details below)
- **Type:** Weekly launch board inside a professional network.
- **Audience fit:** Medium. Skews to developers and designers [S].
- **Rules [S]:** Launch window opens Monday each week. Launches run about a week. Individual profiles only; agencies not allowed. Top three get a badge and a feature in the weekly email.
- **Cost [S]:** Launching is free. A Featured Launch starts at $29.
- **Effort:** Low to medium.

**DevHunt**
- **URL:** https://devhunt.org (not fetched; submission details [S])
- **Type:** Dev tool launch board with weekly voting.
- **Audience fit:** Medium. Built for developer tools, libraries and APIs [S].
- **Rules [S]:** Sign in with GitHub and submit a product with logo, tagline, description, URL, screenshots, category and tags. Weekly voting cycles.
- **Cost [S]:** Free listing. Featured listing reported at $49.
- **Effort:** Low.
- **Note:** Audience size claims conflict across sources. Treat as unverified.

**BetaList**
- **URL:** https://betalist.com/support and https://betalist.com/faq (not fetched)
- **Type:** Startup discovery site. Editorially reviewed.
- **Audience fit:** Low to medium for a dev library. Better for consumer or SaaS products.
- **Rules/cost [S]:** One third-party review (October 2026) says submissions now charge a fee, with a refund if rejected. Older pages describe a free route with a long queue. Confirm before paying.
- **Effort:** Low.

**Uneed and Fazier**
- **URLs:** https://uneed.best and https://fazier.com (not fetched)
- **Audience fit:** Low. Mostly SEO backlink directories.
- **Cost/rules [S]:** Conflicting reports. Free queues may be closed or slow. Paid fast tracks are $15 to $30. Check the live sites.
- **Verdict:** Low priority. Only useful as backlinks after the core launch.

## 2. Rust ecosystem

### This Week in Rust (TWiR)
- **URL:** https://github.com/rust-lang/this-week-in-rust and https://this-week-in-rust.org
- **Type:** Official weekly Rust newsletter, community-edited.
- **Audience fit:** High. Every Rust developer who reads newsletters sees it.
- **How to submit [V]:**
  1. Confirm the item fits. Accepted items include how-to intros, walkthroughs, videos, and announcements.
  2. Confirm it was written by a human. No LLM-written articles.
  3. Format the link: link text matches the page title, canonical URL, no `utm_` tracking parameters, and prefixes like `[video]`, `[audio]`, or `[ES]` where they apply.
  4. Open a PR on rust-lang/this-week-in-rust against the draft for the next issue.
  5. Or tweet @thisweekinrust as an alternative.
- **Important section rules [V]:**
  - "Projects/Tooling Updates: PR submissions are no longer accepted for this section." So a crate launch may not go through a PR to that section. Ask the editors which section fits.
  - "CFP for projects" is a route for a project with an OSI license and a public issue tracker. Open an issue that describes a task, label it easy, medium, hard, or tedious, and link contributing guidelines. This gets your project into TWiR's "help wanted" section [V].
- **Deadline/cadence:** Not stated [V].
- **Cost:** Free.
- **Effort:** Low. Submit in the same week as the launch.

### r/rust
- **URL:** https://www.reddit.com/r/rust/ (live rules not fetched)
- **Type:** Subreddit, large Rust community.
- **Audience fit:** High, but high bar.
- **Rules [S, from a mirror of the mod wiki]:**
  - Self-promotion is removed unless the poster is a known, respected Rust contributor, or the code is under an FSF or OSI approved license.
  - You can message the mods first if unsure.
  - Simple questions go to the stickied "Got an easy question?" thread.
  - Live sidebar and flair rules are [U].
- **Best time:** [U].
- **Cost:** Free.
- **Effort:** Medium. Write a real technical post about a design problem, not a link drop.

### users.rust-lang.org (Announcements category)
- **URL:** https://users.rust-lang.org/c/announcements/
- **Type:** Official Rust user forum, announcements category.
- **Audience fit:** High for crate announcements. Long-lived and searchable.
- **Rules:** The category page shows mostly crate and app announcements [V]. The category's own rules were not found [U].
- **Cost:** Free.
- **Effort:** Low. Post a short announcement with the design story and link to docs.

### Rust Discord and Zulip
- **URLs:** https://discord.gg/rust-lang and https://rust-lang.zulipchat.com (not fetched, [U] for exact links)
- **Type:** Official project chat.
- **Audience fit:** High for feedback, low for direct promotion. Lurk and help first. Share in project channels only when relevant.
- **Cost:** Free. **Effort:** Low, steady.

### crates.io and lib.rs discoverability
- **URLs:** https://crates.io and https://lib.rs
- **Practical rules [S, from lib.rs maintainer tips]:**
  - Pick categories in Cargo.toml. Crates with none get flagged.
  - Use up to five keywords. Use alternative terms not already in the name or description, plus one that places the crate among similar ones.
  - Set the `repository` field. Missing it is flagged.
  - Keep releases fresh, or set a maintenance badge.
- **Cost:** Free. **Effort:** Low. Do this before launch.

### awesome-rust and awesome-ratatui
- **awesome-rust:** https://github.com/rust-unofficial/awesome-rust (CONTRIBUTING.md exists, rules not fetched [U])
- **awesome-ratatui:** https://github.com/ratatui/awesome-ratatui [V]
  - Scope: TUI crates "made for or using Ratatui." Categories include AI and Agents, Libraries and Tooling.
  - Entry format: `[name](url) - short description.`
  - Relevant only later, for the terminal renderer. Not for phase 1.
- **Cost:** Free. **Effort:** Low (a PR).

### Rust podcasts and newsletters
- **Rustacean Station:** https://rustacean-station.org. Accepts episode ideas and volunteers via the site. No guest form found. Contact through the site [S].
- **Rust in Production:** https://www.podengine.ai/podcasts/rust-in-production (third-party listing). Hosted by Matthias Endler. Listed as "Accepts Guests." Biweekly Thursday episodes [S]. Check the official show for pitch details.
- **Rust Weekly:** https://rustweekly.com (fetch failed with a TLS error). Submission route [U].
- **Rust Magazine, Show Magazine:** https://rustmagazine.org/topic/show-magazine [V]. Accepts works in progress, completed projects, and Rust events. Submission steps are on /contribution [U].
- **Cost:** Free. **Effort:** Low for newsletters, medium for podcasts.

### Conferences and CFPs
| Event | Dates | Location | CFP status | Source |
|---|---|---|---|---|
| RustWeek 2027 | May 24–29, 2027 (talks May 25–26) | Utrecht, NL | **Open: opens Oct 1, 2026, closes Jan 10, 2027 at 23:59 CET** [V] | https://sessionize.com/rustweek-2027 |
| RustConf 2026 | Sept 8–11, 2026 | Montréal | Closed, and the event has passed [V] | https://rustconf.com |
| RustConf 2027 | Sept 7–10, 2027 | Montréal (aggregator) or Vancouver (rustconf.com save-the-date). Conflict. | Not found [U] | https://rustconf.com |
| RustLab 2026 | Nov 1–3, 2026 | Bologna, IT | Not stated. Speakers already listed. Contact info@rustlab.it [V] | https://rustlab.it |
| EuroRust 2026 | Held in Barcelona (past) | Barcelona | Past [V] | https://eurorust.eu |
| EuroRust 2027 | Oct 13–16 or 14–17, 2027 (conflict) | Prague | Not stated [V] | https://eurorust.eu |

- **Best target:** RustWeek 2027. Its CFP is open now and closes January 10, 2027. A talk about designing a streaming, validated UI DSL in Rust fits the audience. Submit before the launch date if possible.
- **Cost:** Free to submit. Travel is the real cost.
- **Effort:** Medium. A talk abstract plus a slide deck.

## 3. GPUI and Zed

### Zed Discord and the GPUI community
- **URLs:** Zed Discord (invite link found as discord.gg/zed-community [S]). GPUI docs at https://www.gpui.rs. Zed contributing guide at https://github.com/zed-industries/zed/blob/main/CONTRIBUTING.md [V].
- **Type:** Editor community chat and the GPUI framework's home.
- **Audience fit:** High. The renderer's own niche.
- **Rules [S]:** The GPUI docs say the best way to ask about GPUI APIs is to read the source, come to a fireside hack, or ask in the Zed Discord. The gpui.rs page lists no community chat itself [V].
- **Showcase:** None found for GPUI itself [V].
- **Best approach:** Join, answer GPUI questions for weeks, and share a demo in a dedicated channel when it is ready. Ask the team whether a launch announcement is welcome.
- **Cost:** Free. **Effort:** Low to medium, ongoing.

### gpui-component (longbridge) GitHub discussions
- **URL:** https://github.com/longbridge/gpui-component and its Discussions tab [S for discussions link].
- **Type:** Widget library. Its showcase currently lists Longbridge Pro only [S].
- **Audience fit:** High. Our renderer will sit on gpui-component (pinned at =0.8.0 per AGENTS.md). Being a good citizen here matters.
- **Rules:** Not verified. Check the discussions categories live [U].
- **Action:** Report issues and contribute fixes that help our renderer. Ask about a showcase entry once a demo runs.
- **Cost:** Free. **Effort:** Low to medium.

### Jason Lee (@huacnlee)
- **Sources:** https://zed.dev/blog/community-champion-jason-lee [V]. He built gpui-component, has about 115 merged PRs to Zed since February 2024 [V], and is on X and Bluesky [V].
- **Fit:** High. He is the most relevant person in the GPUI-component ecosystem. Approach with a demo and a specific question, not a pitch.
- **Cost:** Free. **Effort:** Low, a single good message.

### awesome-gpui
- **URL:** https://github.com/zed-industries/awesome-gpui [S for contents]
- **Type:** Curated list of projects built with or for GPUI. Categories include Apps and Developer Tools. Entries show star tier and activity status [S].
- **Audience fit:** High, the long-tail search for GPUI projects.
- **How to add [U]:** Likely a PR to the README. Check the file's contribution rules before submitting.
- **Cost:** Free. **Effort:** Low, one PR, after the first public release.

## 4. AI and LLM communities

### Generative-UI peers and their communities (know these, do not spam them)
- **A2UI (Google):** https://github.com/google/A2UI. Declarative JSON with a pre-approved component catalog. README lists v0.9.1 as current production release and v1.0 as release candidate [S]. Community renderers are a section of the project [S]. **Opportunity:** see the "Other high-leverage" section below.
- **AG-UI (CopilotKit):** https://github.com/ag-ui-protocol/ag-ui. An event-based runtime protocol that connects agents and apps. Discord invite listed in two versions [S]; verify the live one before joining.
- **CopilotKit generative-ui repo:** https://github.com/CopilotKit/generative-ui. Describes controlled (AG-UI), declarative (A2UI and Open-JSON-UI), and open-ended (MCP Apps) patterns [S].
- **MCP Apps:** Official MCP extension (SEP-1865) introduced November 2025. Iframe-based interactive UI [S]. Discussion: https://github.com/modelcontextprotocol and https://blog.modelcontextprotocol.io/posts/2025-11-21-mcp-apps/ [V for blog].
- **Vercel json-render:** https://github.com/vercel-labs/json-render. Apache 2.0. Catalog with Zod schemas. Renderers for React, Vue, Svelte, Solid and React Native. Star count was about 13,000 in early 2026 [S].
- **Thesys OpenUI:** https://github.com/thesysdev/openui. MIT. Star count about 11,252 as of Oct 16, 2026 per one source; the GitHub API showed 10,221 on Oct 8 per another [S]. Verify on GitHub.

**Positioning gap:** Every well-known generative-UI project targets web (React, Vue, Svelte) or mobile. A native desktop renderer for GPUI with a streaming, validated DSL is a clear angle. Lead with "native, not web" in every post.

### r/LocalLLaMA
- **URL:** https://www.reddit.com/r/LocalLLaMA/ (live rules not fetched)
- **Audience fit:** Medium. Local-model and open-source enthusiasts, not UI people.
- **Rules [S, from an unofficial wiki mirror, possibly outdated]:** Self-promotion about 10% of your content. Minimum 10 comment karma and 5 subreddit karma to post. Search for similar posts before posting.
- **Cost:** Free. **Effort:** Medium. Only worth it if the demo shows a local model driving the UI.

### r/MachineLearning
- **URL:** https://www.reddit.com/r/MachineLearning/
- **Audience fit:** Low to medium for a UI library. Research-heavy.
- **Rules [S]:** `[P]` tag for projects. A periodic self-promotion thread exists (example: a March 2025 thread) [S]. Check the sidebar.
- **Action:** Use the self-promotion thread, not a main post.

### r/LLMDevs
- **URL:** https://www.reddit.com/r/LLMDevs/
- **Audience fit:** Medium. Builders of LLM apps.
- **Rules [S, later update]:** Free open-source tools can be shared without prior approval. Non-free licenses need approval. Commercial promotion is prohibited.
- **Cost:** Free. **Effort:** Low.

### r/AI_Agents and r/LangChain
- **Status:** [U]. Not verified. Check the live rules before posting.

### AI Engineer (events)
- **URL:** https://www.ai.engineer
- **Events [V]:** AI Engineer Code Summit, Nov 10–12, 2026, San Francisco, described as "application-based." AI Engineer Europe, Feb 17–19, 2027, London. AI Engineer World's Fair 2027, June 29 to July 1, 2027, San Francisco. AI Engineer World's Fair 2026 has already passed.
- **CFP status:** Not found on the site for any event [V]. The "Submit" buttons are for newsletter signup.
- **Action:** Email the team or check the event's own page for speaker applications. Treat as unverified until found.
- **Cost:** Free to apply, travel and ticket cost if accepted.

### Latent Space
- **URL:** https://www.latent.space/about [S]
- **Type:** AI engineering podcast, newsletter and community. Over 197,000 subscribers claimed as of Aug 2026 [S].
- **Pitch route [S]:** News tips and pitches go to a listed address on /about (masked in snippet). Separate address for sponsorship and business. PR outreach about a month before major announcements.
- **Audience fit:** High for AI-engineering readers.
- **Cost:** Free to pitch. **Effort:** Medium. Write a short pitch with a demo video.

### Newsletters
- **TLDR AI:** https://signup.tldr.tech/ai [S]. Claims 1.1 million subscribers. No tool-submission process found. Advertising via tldr.tech contact [U].
- **Console.dev:** https://console.dev [V]. Weekly, reviews 2 to 3 developer tools. Lists 30k+ subscribers (claimed). Published Thursdays. No submission route on the homepage. Check /about [U].
- **Changelog News:** https://changelog.com/submit [S]. Open to your own work. Commercial products excluded. Choose items fellow developers will find interesting, and explain why it's news. You may need an account [S].
- **The Rundown AI and Ben's Bites:** [U]. Submission routes not found. The Rundown claims 600k+ subscribers [S]. Ben's Bites leans toward funding and business news, so it fits a company launch better than a library.

## 5. Social

### X / Twitter
- **Handles:** @thisweekinrust [V via TWiR page]. Rust Foundation, Zed, Longbridge and @huacnlee [S]. Relevant AI/agent accounts: CopilotKit, Google DevRel for A2UI, MCP accounts [U].
- **Hashtags:** #rustlang, #rust, #GPUI, #LLM, #AIagents [U for hashtag effectiveness].
- **Audience fit:** High for Rust and AI builders.
- **Rules:** X does not restrict launches, but reply chains and quote posts with a demo GIF work best [U].
- **Cost:** Free. **Effort:** Medium, ongoing.

### Bluesky
- **Status:** [U]. Rust and Zed communities are active [S via the Jason Lee profile]. Starter packs for Rust developers exist but were not verified.
- **Cost:** Free. **Effort:** Low.

### Mastodon
- **Instances:** https://hachyderm.io (tech focus) and https://fosstodon.org (open source focus). Rules not fetched [U].
- **Fit:** Medium. Good for the Rust and open-source crowd.
- **Cost:** Free. **Effort:** Low.

### LinkedIn
- **Fit:** Low to medium. Better for personal-brand posts than for dev-tool launches. Use it to share the talk or post.
- **Cost:** Free. **Effort:** Low.

### YouTube
- **Plan:** One 2-minute demo video (the GIF's longer cousin). One 15-minute walkthrough later.
- **Rust creators:** Jon Gjengset streams roughly once a month and posts replays [S]. Other Rust YouTubers were not verified [U]. Cold-pitch reviewers only after the demo is solid, and do not pay for reviews without disclosure.
- **Cost:** Free to make. **Effort:** High for the walkthrough.

## 6. Developer directories

### Console.dev
- See section 4.

### OpenAlternative
- **URL:** https://openalternative.co [U for process]. Open-source alternatives directory. Submission via GitHub repo [S].
- **Fit:** Low. We are not an open-source alternative to a proprietary product. Could be a fit if framed as an alternative to a closed generative-UI SDK.
- **Cost:** Free (per third-party guides). **Effort:** Low.

### AlternativeTo
- **URL:** https://alternativeto.net/faq [V for steps]. Add with "Suggest new application" after signing up. Approval has no fixed timeline. Paid $5 priority review reported [S]. Official Terms let AlternativeTo refuse listings without giving a reason [V].
- **Link type:** Reports conflict on whether the website link is nofollow [S].
- **Fit:** Medium. Shows up in "alternatives to X" searches. Add after the first release.
- **Cost:** Free, or $5 priority [S]. **Effort:** Low.

### LibHunt
- **URL:** https://libhunt.com [U for process]. Community-curated list of libraries. It monitors Reddit, Hacker News and Dev.to for open-source repos [S].
- **Fit:** High for a Rust-adjacent library. Listing can happen automatically after a Show HN or Reddit post.
- **Cost:** Free to browse and likely free to add [U]. **Effort:** Low.

### GitHub Trending
- **Mechanics:** GitHub's exact formula was not found [U]. Third-party trackers rank by star gain over a window [S]. Stars drive trending.
- **Rules to follow:** Do not buy stars or use star-exchange groups. GitHub and trackers filter them, and it damages trust.
- **Action:** Make the repo look good on first visit: clear README, demo GIF, topics, description, and a link to the demo.
- **Cost:** Free. **Effort:** Ongoing.

### dev.to and Hashnode
- **dev.to [V]:** Maximum four tags. Use `canonical_url` front matter when cross-posting. Self-promotion and posting frequency rules were not on the editor guide [V]. The code of conduct does not mention promotion [V].
- **Hashnode [S]:** Canonical control via `originalArticleURL` in the GraphQL mutation. Cross-post from dev.to with a canonical back to the original.
- **Cost:** Free. **Effort:** Medium for a deep dive.

### Medium publications
- **Status:** [U]. Not researched. Paywall reduces reach for developer audiences. Low priority, and only with a cross-post canonical.

### Hugging Face Spaces
- **Status:** [U]. Hosting a live demo on Spaces is a plausible low-cost demo channel, but a GPUI app will not run in a browser. Use it only for a web-based companion demo if one exists.

### Product Hunt and others
- See section 1.

### Awesome-selfhosted
- **Fit:** Low. Not a self-hosted service.

## 7. Other high-leverage moves

1. **Talk at RustWeek 2027.** The CFP is open now. Submit a proposal for "streaming, validated UI from an LLM: a Rust DSL and a native renderer." This gives credibility and a recorded talk for future promotion.
2. **Technical deep-dive post.** A 2,000-word write-up on one hard problem (for example, the streaming parser with chunk equivalence, or patch streams into GPUI). Publish on your own site and cross-post. Link it from HN, Lobsters and TWiR. The post must be human-written, which is also a TWiR rule.
3. **A2UI community renderer.** The A2UI project lists community renderers. A GPUI renderer for A2UI-style payloads would make the project part of a bigger standard. Check the format compatibility first. Do not promise a fit that does not exist.
4. **Ask the GPUI-component author for feedback.** A short, specific message with a demo and one technical question is high leverage. Do not pitch.
5. **Cheap newsletter sponsorship.** Only after organic posts work. Check each newsletter's advertising page. TLDR and Console publish media kits [U/S].
6. **Demo GIF and 2-minute video.** Needed for every launch above. Record once, cut for each channel.
7. **Office hours in the Zed Discord.** Helping others with GPUI questions for a few weeks builds trust faster than any post.

## Top 15 for this product

1. **Show HN with a runnable demo.** Largest single spike for a systems-minded audience. Needs a tryable sample app.
2. **This Week in Rust (PR plus the "CFP for projects" issue route).** Free, reaches every Rust developer on the newsletter.
3. **Lobsters (rust and programming tags).** Deep technical audience. Requires invite and 70-day age.
4. **GPUI community (Zed Discord, gpui-component discussions, Jason Lee).** The niche that will actually adopt a GPUI renderer.
5. **RustWeek 2027 talk.** Credibility, and the talk recording keeps working after the launch. CFP closes Jan 10, 2027.
6. **users.rust-lang.org Announcements.** Searchable and durable. Crate announcements do well here.
7. **r/rust (with a real write-up).** High value, but only if the post meets the mod bar. Message mods first.
8. **Product Hunt (AI Agents or LLM Developer Tools category).** Free and with a long tail. Stagger it one day after HN so attention does not split.
9. **Changelog News submission.** Developer audience, free, and editors choose interesting items.
10. **Console.dev.** Weekly dev-tool newsletter with a large audience. Find the pitch route first.
11. **awesome-gpui PR.** Long-tail search from people looking for GPUI projects.
12. **Latent Space pitch.** AI-engineering reach. Pitch with a demo video.
13. **dev.to and Hashnode deep dive.** Permanent SEO content. Cross-post with canonical.
14. **r/LocalLLaMA or r/LLMDevs (self-promo rules respected).** Medium reach, but only with a local-LLM demo.
15. **A2UI and AG-UI ecosystem presence.** Adjacent standards communities. Show up as a native renderer. Lower volume, high relevance.

## Suggested launch sequence

Assumes a launch date L. Dependencies are noted inline.

### Pre-launch: about 10 weeks out (L-10w)
- Get a Lobsters invite. Engage in the chat. The 70-day clock starts when the account is created, so the account must exist about 10 weeks before L.
- Create a personal Product Hunt profile (not a company account).
- Start a Zed Discord presence. Answer GPUI questions.
- Read the gpui-component discussions and file one useful issue.
- Start the Reddit accounts and build comment karma. Comment in r/rust and r/LocalLLaMA without links. Several subreddits require karma before posting.
- Submit the RustWeek 2027 CFP (closes Jan 10, 2027).
- Check the live rules for every channel above and record them in the channel's notes.

### Pre-launch: about 6 weeks out (L-6w)
- Finish the README, demo GIF and docs (see checklist).
- Publish the crate to crates.io (v0.x is fine). Set categories, keywords and `repository`.
- Publish a runnable sample app with no sign-up. This is the Show HN artifact.
- Message Jason Lee (or the gpui-component discussion) with the demo and one question.
- Draft the Show HN post, the TWiR submission, the Lobsters post, the r/rust write-up, and the dev.to deep dive.
- Record the 2-minute demo video.

### Pre-launch: about 2 weeks out (L-2w)
- Message r/rust mods with a self-promo question (if the rule requires it).
- Warm up Mastodon and Bluesky with one post a day, mostly technical.
- Pitch Latent Space and Changelog News if their rules call for advance notice.
- Set up tracking: GitHub stars, referrers, sign-ups or downloads.

### Launch week (L)
- **Monday:** Peerlist Launchpad window opens (optional; low priority).
- **Tuesday (or the best day you have per your tracking):** Show HN at about 8 to 9 a.m. US Eastern. Stay all day. Launch Product Hunt at 12:01 a.m. Pacific on the same day only if you can handle both threads. Otherwise launch PH on Wednesday.
- **Wednesday:** Product Hunt (if not Tuesday). Post the Lobsters story if the account is over 70 days old and under 25% self-promo. Post the users.rust-lang.org announcement.
- **Thursday:** r/rust write-up (after mod check). r/LocalLLaMA or r/LLMDevs if the demo shows a local model. X, Bluesky and Mastodon posts with the GIF.
- **Friday:** dev.to and Hashnode deep dive with canonical URL. Submit to This Week in Rust for the next issue (TWiR publishes weekly, so the submission lands in the following week's issue).

### Post-launch: weeks 1 to 4
- Answer every GitHub issue and HN comment within a day.
- Submit to Changelog News and Console.dev (if still open).
- Open awesome-gpui PR.
- Submit AlternativeTo and LibHunt listings if not auto-added.
- Publish a follow-up: lessons learned, benchmarks, or a new feature. Each post can go to a fresh audience.
- Post a Rust talk proposal recap if RustWeek accepts.

### Post-launch: months 2 to 3
- Apply to AI Engineer events once the CFP is confirmed.
- Pitch the YouTube walkthrough to Rust creators.
- Consider a sponsored newsletter placement only if organic numbers justify the cost.
- Start the ratatui renderer track only after GPUI traction. Submit to awesome-ratatui then.

## Pre-launch checklist

- [ ] crates.io account, crate published, categories (up to 5), keywords (up to 5), `repository`, license file (MIT or Apache-2.0 recommended), docs.rs builds.
- [ ] README: one-paragraph pitch, "native not web" framing, install, 30-second demo GIF near the top, minimal example, link to docs, link to sample app, status (pre-1.0), license.
- [ ] Demo GIF (under 10 MB, a few seconds, looping) and a 2-minute video.
- [ ] Runnable sample host app with no sign-up. Show HN depends on it.
- [ ] Docs site (even a static site generated from the spec) with the DSL reference and the patch protocol.
- [ ] Spec and conformance fixtures public, so reviewers can check streaming equals batch.
- [ ] Example apps: at least one LLM-driven example (local model and one hosted model).
- [ ] Changelog file, versioning policy, and a contribution guide with issue labels for TWiR's "CFP for projects."
- [ ] CODE_OF_CONDUCT and SECURITY policy.
- [ ] Logo or wordmark, consistent name (placeholder until named).
- [ ] Social profiles: X or Bluesky handle, GitHub org, personal Product Hunt profile.
- [ ] Analytics or referrer tracking for the docs site.
- [ ] Human-written launch write-ups for HN, Lobsters, TWiR, r/rust and dev.to (no LLM-written text).
- [ ] Disclosure of AI assistance in the build, if relevant (Lobsters `vibecoding` tag rule).
- [ ] Rules checked on each channel's live page on launch day.
- [ ] Prepared answers for likely HN questions: how is it different from json-render, OpenUI, A2UI, MCP Apps, AG-UI; security model (no arbitrary code); performance; license; roadmap.

## Gaps and unverified items

- Reddit rules could not be fetched directly. All subreddit rules are [S] or [U].
- Rust Weekly submission process: fetch failed with a TLS error.
- Hachyderm and Fosstodon rules: fetch returned no content.
- Peerlist, DevHunt, BetaList, Uneed, Fazier, OpenAlternative, LibHunt, AlternativeTo: submission details come from third-party guides. Check each site.
- TLDR, Ben's Bites, The Rundown: no official submission route found.
- EuroRust 2027 and RustConf 2027 dates conflict across sources.
- AI Engineer events: no CFP found on the site.
- Best posting day: one analysis suggests Tuesday to Thursday for Show HN [S]. Not verified with HN data.
- GitHub Trending formula: not found.
- Star counts for json-render and OpenUI come from third-party trackers and are snapshots.

## Sources

Official and primary:
- https://news.ycombinator.com/showhn.html
- https://news.ycombinator.com/newsguidelines.html
- https://lobste.rs/about
- https://lobste.rs/t/rust
- https://www.producthunt.com/launch
- https://www.producthunt.com/categories
- https://github.com/rust-lang/this-week-in-rust
- https://this-week-in-rust.org/
- https://users.rust-lang.org/c/announcements/
- https://sessionize.com/rustweek-2027
- https://rustconf.com
- https://rustlab.it
- https://eurorust.eu
- https://www.ai.engineer/worldsfair
- https://www.ai.engineer/
- https://github.com/zed-industries/awesome-gpui
- https://github.com/ratatui/awesome-ratatui
- https://github.com/rust-unofficial/awesome-rust
- https://www.gpui.rs/
- https://zed.dev/blog/community-champion-jason-lee
- https://github.com/longbridge/gpui-component
- https://github.com/google/A2UI
- https://github.com/ag-ui-protocol/ag-ui
- https://github.com/CopilotKit/generative-ui
- https://blog.modelcontextprotocol.io/posts/2025-11-21-mcp-apps/
- https://github.com/vercel-labs/json-render
- https://github.com/thesysdev/openui
- https://dev.to/p/editor_guide
- https://dev.to/code-of-conduct
- https://console.dev
- https://changelog.com/submit
- https://www.latent.space/about
- https://alternativeto.net/faq
- https://lib.rs (tips reviewed through lib.rs dashboard pages)
- https://rustmagazine.org/topic/show-magazine
- https://rustacean-station.org (via search)
- https://www.podengine.ai/podcasts/rust-in-production (third-party listing)

Search snippets and third-party (treat as [S]):
- https://help.peerlist.io/individual/launchpad
- https://devhunt.org (via skills and guide pages)
- https://betalist.com/support
- https://saascity.io/directories/betalist
- https://uneed.best and https://fazier.com (via guides)
- https://signup.tldr.tech/ai
- https://reddit.com/r/rust (mod rules via mirror: https://axebps-redlib.hf.space/r/rust/wiki/rules, not loaded)
- https://reddit.com/r/LocalLLaMA (wiki mirror)
- https://feedbagel.com/post/optimal-posting-time-on-show-hn-revealed-through-data-analysis (Show HN timing analysis)
- https://www.ideaproof.io and https://saascity.io (AlternativeTo and directory claims)
- https://ossinsight.io and https://privacytools.io (trending mechanics)
