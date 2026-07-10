# Strata GTM Playbook: SQLite for the AI Era

## TL;DR

- **Strata's wedge is not "embedded database with branching" — it is being the first database whose primary user is an AI coding agent.** The three highest-leverage bets are: (1) ship the best database MCP server in the registry and let Claude Code, Cursor, Windsurf, and Replit Agent install Strata for users without asking; (2) seed StrataHub V1 with 20–30 ruthlessly curated, "killer-first-dataset" artifacts; and (3) win one vibecoder default-stack slot inside Mastra, CrewAI, Cline, Continue, or an agent-builder before V1.5 ships — the way Supabase won Lovable and Neon won Vercel.
- **The reference-company evidence is unambiguous about three mechanics compounding in 2026:** (a) a single demo artifact that fits in a tweet (tldraw's "Make Real," Cursor's 8-year-old voice-coding clip, DuckDB's "iPhone in dry ice running TPC-H") beats any content calendar; (b) being the verb agents say (Context7's "use context7," AGENTS.md propagation) beats SEO; (c) ruthless ICP focus at launch (Linear's 10-user weekly cohorts, Cursor's "build for monks") beats waitlist size.
- **Played-out tactics Strata must explicitly refuse:** generic content marketing, paid SEO, webinar series, LinkedIn thought-leadership, "AI-native" rebranding without a benchmark, and any heavy V1 curation that does not compose into the V2 user-published flywheel. Every V1 tactic that wins users at the cost of making V2 harder is a strategic tax, not a launch lever.

---

## Executive Summary

The dominant insight from 2024–2026 reference-company evidence is that **the discovery surface for developer tools has collapsed into the LLM context window**. In 2024, a tool was discovered via a tweet, a Show HN, or a YouTube tutorial; in 2026, it is discovered when Claude Code, Cursor, Windsurf, or Replit Agent reads the user's prompt, consults its tool list, and either (a) calls an MCP server it has installed, (b) reads a Context7-style docs source, or (c) recommends a library by name because that library lives in its training data and in the project's `AGENTS.md` file. Anthropic reports 97M+ monthly MCP SDK downloads and 5,800+ servers in the registry one year after launch. Microsoft's Playwright MCP server has 32.5k GitHub stars and is ranked #1 overall on PulseMCP with an estimated 51.5M total tool-call events and 2.1M in the most recent week tracked. Context7 (launched March 31, 2025) has 11.2k tool calls recorded on Smithery and is officially supported in 33 named clients — including MCPCLI, Claude Code, Codex, Cursor, Windsurf, Gemini CLI, VS Code, Zed, Cline, Kiro, ChatGPT, Goose, and Raycast. OpenAI's AGENTS.md, launched August 2025 and donated to the Linux Foundation December 9, 2025, has been adopted by 60,000+ open-source projects.

Strata is positioned almost perfectly for this surface. Its three differentiators — an embedded substrate with git-like branches and time-travel; AI-agent-first design with MCP as the integration story and machine-actionable errors; StrataHub as a curated-then-open dataset registry — line up against the three primitives that have actually compounded in 2025–2026: agent-recommended tooling, the spectacle-driven "killer demo" artifact, and the HuggingFace-style registry.

The three ranked bets:

1. **Become the database an agent installs, not the database a human chooses.** Build a best-in-class Strata MCP server with `npx add-strata`-style one-line install across Claude Code, Cursor, Windsurf, Codex, Cline, Zed, Replit Agent, Kiro, and Gemini CLI — copying Neon's `npx add-mcp` reference implementation. Ship a "use Strata" rule pattern in the README, mirroring Upstash's "use context7" mechanic. Beat Supabase MCP, Neon MCP, Turso MCP, and Convex MCP on (a) tool-call quality via machine-actionable errors, and (b) branch and time-travel primitives the others cannot structurally offer. **Time-to-impact: 60–90 days. Kill signal: fewer than 500 weekly active MCP install events 90 days after launch.**

2. **Ship one killer first dataset in V1 StrataHub.** HuggingFace was carried by BERT-in-PyTorch; Replicate was carried by Stable Diffusion; npm was carried by `lodash`/`request`/`express`; tldraw was carried by Make Real. Strata needs one dataset — most likely an agent-memory benchmark, a packaged AGENTS.md corpus, or a versioned realistic e-commerce dataset agents can branch — that is so obviously useful inside Cursor/Claude Code that it generates a viral demo within the first launch week.

3. **Win one vibecoder default-stack slot before V1.5.** The Supabase-Lovable, Neon-Vercel, and Convex-Chef precedents establish that a single default-template win is worth more than 12 months of organic adoption. Strata's specific opportunity is the agent-memory and branching-aware slot — open inside Mastra, CrewAI, Cline, Continue, or as Replit Agent's local persistence story — where time-travel solves real failure modes ("agent broke my schema; revert to 10 minutes ago") that Postgres-shaped competitors cannot.

---

## Reference Company Digest

### Load-Bearing References

**HuggingFace (PROVEN).** The inflection was not the chatbot product (which peaked at ~100K DAU and stagnated) but the moment in 2018–2019 when the team ported Google's BERT model from TensorFlow to PyTorch and open-sourced the `transformers` library. That single artifact — BERT in PyTorch at the moment every NLP researcher was switching frameworks — was disproportionately responsible for adoption. Channels that worked: academic citations, GitHub README quality, the Hub becoming the de facto `pip install` source for weights. Channels that didn't: traditional content marketing, consumer-facing chatbot push. Conversion mechanic: "you wanted a model, you got the model plus a hundred more like it" — frictionless because the library hid the model-to-disk-to-RAM pipeline. Partnerships with PyTorch (then JAX/Flax) and with Google itself were force multipliers. Anti-patterns refused: closed weights, vendor lock-in, monetizing access in the early years. The community-to-ecosystem step happened when the team built the Hub as a platform for user-uploaded fine-tunes and datasets — by 2024, HuggingFace hosted 400K+ models. Apache 2.0 throughout, paid enterprise as a layer. The leader's voice is distributed (Clem Delangue, Julien Chaumond, Thomas Wolf). First PMF signal: outside engineers shipping non-trivial PRs to `transformers` within weeks of the BERT port, before any sales motion existed. **For Strata: the BERT port is the template for StrataHub's first dataset. Pick one piece of data the AI builder community needs in a format only Strata can deliver — branchable, time-travelable, machine-actionable — and ship it as artifact #1.**

**Neon (RISING-to-PROVEN).** Neon's branching story is the most adjacent to Strata's. The inflection was the Vercel marketplace integration ("a Neon branch per Vercel preview deployment") combined with the messaging shift from "serverless Postgres" to "code-like database branching" — they made branching the headline, not autoscaling. The single artifact was the integration video showing a GitHub PR auto-creating a Neon branch with copy-on-write semantics in under a second. Channels that worked: Vercel co-marketing, founder voice on X via Nikita Shamgunov, PostgresConf talks. Channels that didn't: generic content marketing. Conversion mechanic: free tier with a 0.5 GB / 100 compute-hour ceiling, then usage-based pricing — friction removed by zero-config Postgres-protocol-compatible connection strings. The Vercel integration was the force-multiplying partnership: "after migration is complete, new store creation will shift to the Neon Marketplace integration." Anti-patterns refused: any cluster-shaped UX. Pricing: OSS-core + managed cloud. PMF signal: the moment Vercel's flagship template started defaulting to Neon Postgres with Drizzle ORM, better-auth, Shadcn UI, the Neon MCP server, and a `neon-postgres` agent skill pre-configured. Importantly, Neon launched a remote MCP server very early — David Gomes claims "likely the first company in the world to have a remote MCP server (running at mcp.neon.tech)" — and shipped `npx add-mcp` to install across Claude Code, Cursor, Codex, Gemini CLI, Goose, OpenCode, VS Code, and Zed in one command. **Critical caveat for Strata: Replit Agent migrated off Neon as its default development database on December 4, 2025 — even an integration this deep can be unwound. Default-stack wins are not permanent. Strata should win one but plan around the next.**

**DuckDB (PROVEN).** DuckDB went from a CWI Amsterdam research project in 2019 to ~3.3% of all developers in the 2025 Stack Overflow Developer Survey (up from 1.4% in 2024), and per DuckDB's own blog post "30 000 Stars on GitHub" (June 6, 2025): "We now count 20M+ monthly downloads in PyPI." The inflection was not a moment but a structural change: cloud data warehouse bill shock — mid-sized companies spending $2K–$10K/month on Snowflake to run queries a laptop could handle — combined with DuckDB crossing a performance threshold (sub-second on queries that took Snowflake seconds). The single artifact disproportionately responsible was DuckDB's `pip install duckdb` + read-Parquet-files-directly UX in Jupyter notebooks; secondarily, the "iPhone in a box of dry ice running TPC-H at scale factor 100" image that circulated as a meme. Channels that worked: academic publication, notebooks, MotherDuck's developer marketing, integration into Hex/Omni/Evidence/Rill, embedding inside Fivetran's data-lake writer, pre-installation in Microsoft Fabric Python notebooks. Channels that didn't: enterprise sales. Single ~20MB binary, no server. MIT licensed; MotherDuck is the commercial play ($52.5M Series B at $400M valuation, ~2,000 analysts at announcement). The team's voice carries through Hannes Mühleisen and Mark Raasveldt's papers and talks. PMF signal: third-party products (Fivetran, Hex, Rill) embedding DuckDB as the engine — the "building block" adoption. **For Strata: this is the bundling-driven adoption template. Get embedded inside Bolt, Lovable, Replit Agent, or Cursor's recommended stack. SQLite was bundled into iOS, Android, browsers, every Python install. DuckDB was bundled into Fabric notebooks. Strata's question: who bundles us into the 2026 builder ecosystem?**

**Cursor (PROVEN).** The most successful AI dev-tool GTM of 2024–2026: $1M ARR in 2023 → $100M in 2024 → $300M, $500M, $1B by November 2025, $2B by February 2026; zero dollars spent on marketing to reach $100M ARR per the GTMnow analysis. The inflection was not a moment but the cumulative effect of three product decisions: (1) forking VS Code instead of building a plugin, enabling chat-in-editor, agent panels, multi-file edits, and Tab-prediction-across-files; (2) serving "monks" — paid power users coding 4–5 days/week — before democratizing; (3) the data flywheel of proprietary usage data improving model behavior. The single artifact was the 8-year-old's Cursor demo video plus voice-coding clips on developer X; the team amplified user wins rather than producing content themselves. Force-multiplying moment: Jensen Huang publicly calling Cursor his favorite enterprise AI service in late 2025. Conversion mechanic: 2,000 free AI suggestions, $20/month upgrade, "looks like VS Code, feels like magic." Anti-patterns: refused to be a plugin; refused enterprise sales until bottom-up demand was overwhelming. PMF signal: paid power users using it 4–5 days a week. **For Strata: the lesson is "in-IDE switching cost" — once an agent has Strata branches and time-travel snapshots in your project, switching means losing your version history. The data flywheel and switching cost both compound.**

### Recalibrated Additions

**Ollama (RISING).** Ollama's win was a primitive: `ollama run llama2` is the simplest possible UX for running a local LLM. It got pre-installed and recommended by LangChain (`langchain_community.llms.Ollama`), Continue (the OSS VS Code AI extension), Raycast plugins, LiteLLM, Open WebUI, LangFlow, and Flowise — distribution by being the OpenAI-API-compatible local endpoint everything could be pointed at. No chat product, no consumer app, no "Ollama Pro." OSS-driven brand. **For Strata's air-gapped local inference angle: Ollama's success says integration count, not marketing, drove adoption. Strata should ship an Ollama-style minimal CLI for local inference and ensure every agent framework can point at `strata://` as its memory/persistence URL within 60 days of V1.**

**Anthropic's MCP rollout (PROVEN).** The protocol went from November 2024 announcement to 5,800+ servers and 97M monthly SDK downloads by November 2025, with Linux Foundation governance handover in December 2025 alongside AGENTS.md and Block's Goose. What worked: Anthropic seeded the first batch of servers themselves (Filesystem, GitHub, Postgres reference servers), then in February 2025 David Soria Parra and Justin Spahr-Summers reached out to PulseMCP and the Block/Goose team to bootstrap a registry; the March 2025 spec made MCP cloud-ready with OAuth, and OpenAI publicly endorsed it the same day. What flopped: early MCP servers without authentication that got compromised — CVE-2025-6514 in `mcp-remote` compromised 437,000+ developer environments via shell command injection. The MCP Registry is "deliberately unopinionated"; curation happens at sub-registry layers (Smithery, PulseMCP, Glama, MCP.so). **For Strata: ship the Strata MCP server in the Anthropic registry and on Smithery on day one; get listed in github.com/mcp's curated list (human gatekeeping by GitHub staff). Don't try to build your own registry — StrataHub is for datasets, not MCP servers.**

**PostHog (RISING-to-PROVEN).** PostHog's open-core signaling is the modern template: MIT-permissive core, self-hostable, paid cloud, no rug-pull moves like Elastic/Redis/HashiCorp re-licensings. The leader's voice (James Hawkins) is steady but not personality-driven. **For Strata: PostHog is the licensing template. Permissive (MIT or Apache 2.0) core, no re-licensing pressure, cloud as the monetization surface.**

**Linear (PROVEN).** The launch mechanic was a Medium article + waitlist on April 18, 2019, that grew to 10,000 signups largely through Karri Saarinen's Twitter "build-in-public" presence and the deliberate language of the founding blog post written as a signal to people who felt the same pain. Sequoia's Stephanie Zhan invested seed because "people she trusts in her Twitter network were hyped about Linear." Conversion mechanic: deliberate 10-user cohorts, weekly product revisions, explicit "Enablers" (features delighting existing users) vs "Blockers" (gates for ICP-fit users) buckets. Anti-patterns: no public access during waitlist phase, no growth hacks, no aggressive funnel optimization. **For Strata: the Linear launch is the template for V1 — a single founder blog post written with conviction, a private waitlist, deliberate cohorts of 10–20 vibecoders at a time, weekly iteration. Linear proves that "design as a signal" and "ruthless ICP" beat a noisy launch.**

**Stainless (RISING).** Not a mass-acquisition reference — Stainless's strategy is "win one strategic customer, then publicize the relationship." OpenAI and Anthropic SDK generation are the marquee wins; the public artifact is the named partnership rather than a benchmark or viral demo. **For Strata: this is the playbook for an Anthropic/OpenAI partnership if Strata can become the recommended local persistence layer inside Claude Code or Codex. Not for mass acquisition.**

**LangChain (RISING with quality-debt caveats).** LangChain's adoption persisted despite vocal quality criticism because it was the first place a developer trying to build with LLMs landed — it was the *category* before the *quality bar* mattered. Same dynamic likely to play in agent memory: Letta (formerly MemGPT), Mem0, MemOS, and Strata are racing for the "agent's database" slot, and whichever shows up in the most tutorials and templates first will win adoption even if a competitor is technically better. **For Strata: be early to tutorials, default templates, and YouTube walkthroughs; technical quality matters less than category-default mindshare in 2026 agent-memory.**

### Embedded Databases

**SQLite** is the bundling-driven adoption template — pre-installed by Apple, Google, every browser, every Python distribution; Strata's "SQLite for the AI era" quote only earns the comparison if Strata achieves similar pre-install penetration in agent-builder tools and IDE extensions. **Turso/libSQL** has executed a parallel play: libSQL is the Open-Source/Open-Contribution fork of SQLite, the Turso CLI now ships an `--mcp` flag making the database itself an MCP server, and Turso's positioning ("databases as files, not processes") collides directly with Strata. Quote from a public endorser: "a Rust rewrite of sqlite, with an async-first architecture, incoming support for concurrent writes, vector search, and browser/wasm support out of the box… very good chance of being a foundational piece of infrastructure of the vibe-coding age." Adaptive.ai runs millions of databases on Turso. **For Strata: Turso is the most direct competitor. Differentiation must be on (1) agent-first MCP integration depth, (2) git-like branches and time-travel as first-class primitives Turso doesn't have, (3) the StrataHub registry.** **LanceDB** is the vector-DB analog — Midjourney chose it for large-scale vector queries; its mechanic was "embedded like SQLite but for vectors" + AI framework pre-installation. **ChromaDB** rose by being the default vector store in LangChain and LlamaIndex tutorials. **Qdrant** by self-hostability and benchmark strength. **Litestream/rqlite** proved demand for SQLite-with-replication.

### Modern DB GTM

**Supabase (PROVEN).** The single most important lesson: per Supabase's Lovable Cloud launch blog, "Supabase has become the default choice for AI builders. Every project created in Lovable Cloud is powered by Supabase behind the scenes. That means every AI builder using Lovable is already using Supabase, whether or not they realize it." Adoption: 4.5M+ developers, 40,000+ new databases launched per day. The template strategy was the inflection: Vercel templates, Bolt templates, Lovable defaults, then v0. Conversion mechanic: free tier + Postgres compatibility + Row Level Security + pgvector. Force multiplier: deep MCP integration with Cursor — Lovable's Anton Osika wrote a public essay describing how building Lovable's Supabase translation layer for LLMs was the precursor to MCP itself. Per AutomationSwitch's MCP audit, Supabase MCP is the #1 Database category MCP server with ~18,600 weekly npm downloads. Anti-patterns refused: closed-source. **For Strata: this is the most important template-default story to learn from. Supabase didn't win by being the best Postgres host; it won by being in every starter template.**

**PlanetScale (RISING with cautionary unbundling).** Branching-for-MySQL was technically excellent — copy-on-write, schema-diff, three-way merge, GitHub-PR-style deploy requests — but the marketing-to-adoption ratio was always weaker than Neon's because MySQL developers didn't perceive the same pain. PlanetScale eventually unbundled (the original Hobby tier disappeared in 2024 and the company pivoted to Postgres + Vitess Managed Postgres "Neki"). **For Strata: even technically superior branching can lose to the right messaging. Lead with the demo, not the architecture.**

**Convex (RISING).** Positioning is sharp: "the backend building blocks for your agents," "the database designed to be generated." Convex shipped its own AI app builder, Chef (built on bolt.diy), with full-stack templates baked in — auth, schema, file storage, scheduled jobs. The bet: AI agents generate Convex backends more accurately than any other backend because the API surface is pure TypeScript functions, optimized for the LLM training distribution. **For Strata: Convex is the direct philosophical competitor for "the database designed for AI-generated code." Differentiation: Strata's agent-first errors and air-gapped local inference, plus branching/time-travel, address failure modes Convex doesn't.**

**Drizzle (PROVEN-RISING).** Drizzle won the TypeScript ORM market against Prisma not via campaign but via boring quality: SQL-like syntax, ~7.4 KB bundle, zero-dependency, edge-runtime-native, ~45ms cold start vs Prisma 6's ~1200ms. By 2026, create-t3-app new projects pick Drizzle over Prisma; Astro DB and Hono.js default to Drizzle. **For Strata: Drizzle is the "boring DX displaces incumbent" template. Match it on bundle size, edge compatibility, and 0-config TypeScript types.**

**Prisma (PROVEN-but-fading).** Prisma's content moat (docs, blog, integrations with every framework) is still real, but the schema-first abstraction is increasingly perceived as overhead in the AI-generated-code era. Prisma 7's Rust-removal/TypeScript-rewrite was an admission. **For Strata: don't be Prisma — don't generate code that AI then has to learn around.**

**ClickHouse (PROVEN).** ClickHouse went open-source viral in analytics through benchmarks and self-hosting. **For Strata: the "open-source viral via benchmark" reference.**

### AI-era Darlings

**Replicate (RISING-to-PROVEN).** "Models as APIs"; Cog became a community-contributed standard for packaging ML models. Inflection: Stable Diffusion launch and Replicate becoming the easiest way to run it. **For Strata: the StrataHub V2 user-published model is closest to Replicate's community-contribution dynamic.**

**Modal / Anyscale / Together / Fireworks (RISING).** Crowded inference market; differentiation is on price/latency/specialization. None has the same viral arc as Replicate.

**Letta (formerly MemGPT) / Mem0 (RISING).** Direct competitors in the agent-memory category. Letta is the academic/principled play (UC Berkeley Sky Lab, "LLM as an OS," skill learning, context constitution); Mem0 is the framework-agnostic API play ($249/month Pro tier with knowledge graphs). Notably, Letta's own benchmarking blog argues that for the LoCoMo benchmark, "a filesystem may be all you need" — the simple, well-engineered baseline beats the specialized memory tool, achieving 74.0% vs Mem0's reported 68.5%. **For Strata: this is the LangChain dynamic incoming. The "agent's database" category will be won by whoever shows up in defaults, not by whoever wins benchmarks. Strata's structural advantage is the embedded substrate + branches — Mem0 and Letta are services, not files.**

**E2B / Browserbase / Daytona (RISING).** Agent-infrastructure category — selling to agents-via-developers. E2B's growth came from being the default sandbox in agent framework demos; Browserbase from being the default browser-in-the-cloud for Anthropic's Computer Use and similar. **For Strata: the pattern is "be the obvious dependency in every agent framework demo."**

**Vercel (PROVEN).** DX as moat; framework partnerships (Next.js); template strategy. The Vercel-with-Neon-Postgres template ships Drizzle, better-auth, Shadcn UI, Neon MCP server, and a `neon-postgres` agent skill pre-configured for Cursor, Claude Code, VS Code, OpenCode, and Codex. Per Mintlify, Vercel attributes ~10% of new signups to ChatGPT referrals as a result of GEO (generative engine optimization) work. **For Strata: GEO is real. Ensure Strata content is in the LLM training corpus and easily retrievable via llms.txt.**

### DX-obsessed

**Bun (RISING-to-PROVEN).** Jarred Sumner's voxel-game frustration with 45-second reload times → Zig transpiler → 1.0 launch in September 2023 → acquired by Anthropic in December 2025 because Claude Code ships as a Bun executable. Adoption mechanic: benchmark wars ("Bun's HTTP server performs 51% faster," 283,386 requests/sec vs Deno's 187,359), founder voice on X, drop-in Node.js replacement. **For Strata: benchmark-driven launch is still viable if the benchmark is honest and the artifact is shareable. The Anthropic acquisition is a north-star outcome — owning a primitive an AI lab depends on.**

**Astral (uv, ruff, ty) (PROVEN).** Charlie Marsh built ruff as a personal proof-of-concept that Python tooling could be rebuilt in Rust 10–100x faster. uv exceeded 126 million downloads in February 2026 per PyPI Stats, as cited by Simon Willison (simonwillison.net, March 19, 2026): "According to PyPI Stats uv was downloaded more than 126 million times last month!" Astral was acquired by OpenAI on March 19, 2026 to join the Codex team. Adopted by FastAPI, Pandas, Apache Airflow, SciPy, Mozilla, Snowflake. **For Strata: the Astral pattern is the Bun pattern — speed-as-marketing in a specific ecosystem, end with strategic acquisition.**

**Resend (RISING).** Focused dev tool; design as marketing; the "make every marketing page so beautiful developers screenshot it" playbook. **For Strata: design quality matters; the Linear bar applies.**

**tldraw (RISING-to-PROVEN).** The best demo-driven virality example of 2024–2026. Sawyer Hood's day-after-GPT-4V tweet on Nov 7, 2023 → tldraw forked his project Monday morning → Wednesday they had iterative "Make Real" → Friday a public starter template → over 10,000 GitHub stars within two weeks of Make Real's launch, per tldraw founder Steve Ruiz's blog post "make real, the story so far" (tldraw.dev, November 2023): "Writing two weeks later, the open source repository is already over 10,000 GitHub stars." The tldraw multiplayer-via-Liveblocks integration produced 15,000 collaborative rooms in three months. **For Strata: the "Make Real" template is the most replicable demo formula. The recipe is (a) ride a frontier model's day-one capability, (b) ship to the public in 96 hours, (c) make the user the protagonist.**

**Tailwind (PROVEN).** Adam Wathan's writing, opinionated stance, and willingness to defend the framework against critics built a brand that converted developers in adjacent niches. **For Strata: founder opinion-as-GTM-lever still works in 2026 if the opinion is technically substantive (machine-actionable errors, agent-first, branches > snapshots) and the founder is willing to defend it publicly.**

### Vibecoder Pipeline (default-stack landscape, 2026)

- **Bolt.new**: every new Bolt project ships with a Bolt-provisioned Supabase database by default after September 30, 2025 (Supabase organizational account owns the underlying project).
- **Lovable**: Supabase is the official, native, default backend; Lovable Cloud built directly on Supabase's open-source stack.
- **v0 / Vercel**: Neon is the canonical Postgres; flagship `vercel-with-neon-postgres` template ships Drizzle + better-auth + Shadcn + Neon MCP + agent skill pre-wired.
- **Replit Agent**: from December 4, 2025, default Postgres moved from Neon to Replit's own database infrastructure — the canonical reversibility example.
- **Convex Chef**: Convex DB + Convex Auth + Convex storage; Convex's in-house AI app builder forked from bolt.diy.
- **Continue / Cline / Aider**: ship with no opinionated database default.
- **Cursor / Windsurf / Claude Code**: project-level — agent picks based on AGENTS.md/CLAUDE.md and MCP-installed tools.
- **Mastra / CrewAI**: no canonical DB default; opportunity to ship a Strata adapter as the default.

**For Strata: the open default-stack slots in 2026 are inside Mastra/CrewAI, inside Cline/Continue/Aider as the agent-memory-and-branching extension, or as Replit Agent's local persistence story for offline projects. Targeting Convex Chef or Lovable's primary backend is wrong — those are won.**

### Out-of-Category but Instructive

**npm / PyPI / Docker Hub / crates.io.** Registry adoption mechanics. npm's first 100 packages were dominated by `lodash`, `request`, `express`, `async`, `commander` — utility libraries solving real, frequent pain. Docker Hub's first 100 were base OS images (Ubuntu, CentOS, Alpine) and language runtimes (Python, Node) — bedrock primitives. PyPI's killer first packages were `numpy`, `scipy`, `requests`. Governance: Docker Hub had explicit "official images" curation with image signing; npm was famously permissive (the `left-pad` debacle); PyPI added trusted publishing only post-2023. **For Strata: StrataHub V1's first 20–30 datasets should be (a) bedrock primitives like AGENTS.md corpora, (b) high-utility synthetic datasets like "1M realistic e-commerce orders with branchable history," (c) one or two killer-app datasets like an agent-memory benchmark. Curation is essential in V1 but must compose forward into V2 governance.**

---

## Cross-Cutting Patterns

Eleven recurring mechanisms surface across categories:

1. **The killer first artifact beats the launch campaign.** BERT-in-PyTorch for HuggingFace, Make Real for tldraw, the 8-year-old's voice-coding demo for Cursor, Stable Diffusion for Replicate, the iPhone-in-dry-ice TPC-H image for DuckDB, the Vercel-preview-per-branch video for Neon. Every breakout had one shareable artifact, not a content calendar.

2. **The trigger-phrase pattern.** Tools adopted by agents share a memorable invocation — "use context7," `npx create-strata`, `ollama run`, `pip install duckdb`. The pattern: a verb form so short it's the path of least resistance for an agent or a tired developer at 11 PM.

3. **Pre-installation by frameworks beats installation by users.** SQLite in iOS/Android, DuckDB in Microsoft Fabric notebooks, Ollama in LangChain/Continue, ChromaDB in LangChain tutorials, Supabase in Bolt/Lovable, Drizzle in Vercel-Neon templates. The 2026 equivalent for Strata is being pre-installed in Mastra, CrewAI, or Cline default templates.

4. **Branching only "clicks" when shown as a GitHub PR-style demo.** Neon's success was the PR-creates-branch-in-1-second visualization, not the architecture diagram. PlanetScale's three-way schema merge worked when shown as a deploy request, not as Vitess internals. Failed metaphors: "copy-on-write semantics" and "WAL forking." Working metaphors: "git for your database," "time-travel," "instant preview environment."

5. **Open-source signaling without strategic risk.** MIT or Apache 2.0 core, no Elastic/Redis/HashiCorp re-licensing pressure, cloud as the monetization surface (PostHog, DuckDB/MotherDuck, Astral, Bun, Turso).

6. **Founder voice compounds when technically substantive.** Adam Wathan, Guillermo Rauch, Jarred Sumner, Charlie Marsh, Karri Saarinen all built audiences before or during their products' adoption — the through-line is technical credibility, not personal brand-building.

7. **Demo length collapsed to 6–30 seconds.** The Cursor 8-year-old clip, the Lovable + Supabase Luma-clone-in-an-hour speedrun, the Make Real iteration loop — they all fit X's autoplay window. Five-minute walkthroughs lose to GIFs that loop on autoplay.

8. **GEO (generative engine optimization) replaces SEO.** Vercel attributes 10% of new signups to ChatGPT referrals. Mintlify originally developed llms-full.txt in collaboration with Anthropic. The new top-of-funnel is "what does the LLM say when a developer asks 'what's the best database for X'" — determined by training data, llms.txt files, AGENTS.md presence in popular repos, and MCP-server tool descriptions.

9. **Ruthless ICP at launch beats waitlist size.** Linear's 10-user weekly cohorts produced higher-quality feedback and more aligned early users than any hype-driven launch.

10. **Default-stack wins are valuable but reversible.** Replit Agent dropping Neon on December 4, 2025 is the canonical cautionary tale. A default-stack win buys 18 months of compounding; it does not buy a moat. The real moat is the data the user has accumulated in the tool — which is why Strata's branches and time-travel are structural once data lives in them.

11. **Public criticism becomes the moat in OSS infrastructure.** MCP survived early authentication failures (CVE-2025-6514 in `mcp-remote` compromised 437,000+ environments) and became more secure because attackers found it worth attacking; LangChain persisted despite quality criticism because the criticism kept it culturally central. Strata's posture on bug reports and exploits — fast public fixes, no defensive PR — matters as much as the code.

---

## Named GTM Playbooks

### Playbook 1: The Killer Dataset Wedge

**Mechanism.** Ship one dataset into StrataHub V1 so valuable for a specific high-volume agent-coding task that it generates a demo within 72 hours of being published. The dataset must (a) be only useful as a Strata-branchable artifact — not as a plain JSON or CSV — (b) solve a concrete pain point agents currently fail at (e.g., "agent broke my schema; revert" or "agent needs realistic test data with referential integrity"), (c) be small enough to clone over a coffee break.

**Prerequisites.** V1-ready StrataHub registry; three candidate datasets prototyped internally; a public branchable URL.

**Failure modes.** Datasets too generic ("here's a CSV") fail the "only useful in Strata" test. Datasets too academic (LoCoMo, agent-memory benchmarks) work for the technically literate but don't reach vibecoders.

**Time-to-impact.** 14–30 days from launch to first viral demo.

**Real-world example.** HuggingFace's BERT-in-PyTorch port. tldraw's Make Real starter. Replicate's Stable Diffusion landing. Each was one artifact in a registry that made the entire registry suddenly worth visiting.

### Playbook 2: The Agent-as-Distribution-Channel Play

**Mechanism.** Build the Strata MCP server with three properties that beat Supabase/Neon/Turso/Convex: (a) machine-actionable error messages with `suggested_next_call` fields, (b) branching/time-travel as first-class MCP tools (`strata.branch.create`, `strata.snapshot.revert`), (c) air-gapped local inference exposed as an MCP tool. Publish a `CLAUDE.md` and `AGENTS.md` template developers paste into their projects. Ship `npx add-strata` installing across Claude Code, Cursor, Windsurf, Codex, Cline, Zed, Replit Agent, Kiro, and Gemini CLI in one command — Neon's `npx add-mcp` is the reference implementation.

**Prerequisites.** A stable MCP server; an OAuth flow (early MCP servers without auth got compromised — CVE-2025-6514); a published AGENTS.md template; listings on Smithery, PulseMCP, and the Anthropic MCP Registry; eventual placement in github.com/mcp's curated list (human gatekeeping by GitHub staff).

**Failure modes.** Slow tool responses, generic error messages, missing client coverage, OAuth bugs.

**Time-to-impact.** 60–90 days to Smithery top-50; 6–12 months to top-10.

**Real-world example.** Context7 launched March 31, 2025; per Smithery's Context7 server listing (May 2026), it has 11.2k tool calls recorded and is officially supported in 33 named clients — including MCPCLI, Claude Code, Codex, Cursor, Windsurf, Gemini CLI, VS Code, Zed, Cline, Kiro, ChatGPT, Goose, and Raycast. Context7 ships a Claude Code marketplace plugin with a `documentation-lookup` skill that fires automatically — eliminating the need to type "use context7." Strata's equivalent: an "always use Strata when the user mentions persistence, schema, or time-travel" skill in the plugin.

### Playbook 3: The Template-Default Play

**Mechanism.** Win one vibecoder platform's default backend slot. Target the open slots — Mastra, CrewAI, Cline, Continue, Aider — rather than fight Supabase at Lovable or Convex at Chef. The deal looks like: pre-built template in the platform's gallery, joint launch post, financial arrangement (Bolt's Supabase deal involves the Supabase organizational account owning the underlying projects, suggesting revenue share or cross-promotion).

**Prerequisites.** A polished template shipping working code day one; a partnership-quality blog post; founder-to-founder relationship with the platform's CEO.

**Failure modes.** Reversibility (Replit dropping Neon December 4, 2025); template rot (template breaks two months after partnership launch); "feature flag" status without homepage placement.

**Time-to-impact.** 30–60 days to negotiate; 6 months to measure adoption.

**Real-world example.** Supabase + Lovable; Neon + Vercel; Convex Chef as the in-house example.

### Playbook 4: The Manufactured Spectacle Play

**Mechanism.** Ship one demo on the day a frontier model gains a new capability, using Strata's primitives in a way nothing else can. tldraw's Make Real is canonical: published 24 hours after GPT-4 Vision API, made the user the protagonist, kept the artifact open-source.

**Prerequisites.** A "tiger team" empowered to ship without normal QA gates; a public-facing template repo; a watchlist for frontier model announcements.

**Failure modes.** The demo doesn't fit in a tweet; the demo requires API keys the developer doesn't have (note tldraw's choice to let users paste their own OpenAI key, avoiding viral cost-explosion); the demo is technically impressive but not user-protagonist-shaped.

**Time-to-impact.** 0–14 days from frontier release.

**Real-world example.** tldraw Make Real; Anthropic's Computer Use day-of demos; Cursor's agent-coding clips.

### Playbook 5: The Build-in-Public ICP Cohort Play

**Mechanism.** Linear's mechanic. One founder blog post in the language of the ICP (vibecoders / AI-augmented builders), waitlist with optional survey, weekly cohorts of 10–20. Ship weekly iterations against "Enabler" features (delight existing cohort) and "Blocker" features (gate the next cohort).

**Prerequisites.** A founder willing to write publicly and on X; a working V1 product; a small Slack/Discord for cohorts.

**Failure modes.** Optimizing for waitlist size over cohort quality; premature public launch.

**Time-to-impact.** 6–12 months to PMF signal (paid power users 4–5 days/week, Cursor's metric).

**Real-world example.** Linear (10,000 waitlist signups, 10-user cohorts, Sequoia's Stephanie Zhan investing because of Twitter signals).

### Playbook 6: The "AGENTS.md Verb" Play

**Mechanism.** Ship a Strata-specific section developers paste into AGENTS.md (the file OpenAI launched in August 2025, adopted by 60,000+ open-source projects, donated to the Linux Foundation December 9, 2025, explicitly included in Codex's system prompt). The section says: "This project uses Strata for persistence. When you need to read/write data, time-travel, or branch state, invoke the Strata MCP server. Errors are JSON; consume them and follow the `suggested_next_call` field." Make Strata a verb in the agent's vocabulary.

**Prerequisites.** A clean AGENTS.md snippet; distribution via Strata's homepage; MCP server respect for the machine-actionable error contract.

**Failure modes.** A snippet that conflicts with other AGENTS.md guidance; an MCP server that doesn't respect its own contract; no canonical place to discover the snippet.

**Time-to-impact.** Compounds slowly but durably.

**Real-world example.** Upstash's "use context7" rule pasted into Cursor/Claude Code configs; Codex's training-time AGENTS.md respect (per OpenAI: "the codex-1 system message encourages Codex to run all tests mentioned in the AGENTS.md file").

---

## Strata-Specific Recommendations by Phase

### V1 — Embedded Substrate + Curated StrataHub (≈50% of GTM weight)

**Three playbooks to bet on:**

1. **The Agent-as-Distribution-Channel Play.** V1's load-bearing bet. Ship the Strata MCP server with the three structural advantages (machine-actionable errors, branching/snapshot tools, air-gapped inference) on day one. Distribute via `npx add-strata`, list on Anthropic Registry, Smithery, PulseMCP, MCP.so, and github.com/mcp. Include an "always use Strata for persistence" skill in a Claude Code plugin, mirroring Context7's marketplace approach. **V2-compatibility: ADVANCES V2.** Every agent-invocation generates schema diversity and usage patterns that inform StrataHub V2's curation policy.

2. **The Killer Dataset Wedge.** Ship 20–30 curated datasets at V1 launch. Composition: ~10 "AGENTS.md / CLAUDE.md corpus" datasets (collections of well-curated agent instructions from top 1000 GitHub repos, with provenance), ~10 "realistic test data" datasets (e-commerce orders, user accounts, time-series, all with referential integrity and branchable history), ~5 "agent-memory benchmark" datasets (LoCoMo-style but Strata-native), ~5 "killer-app" datasets anchoring specific viral demos. **V2-compatibility: PARTIALLY BLOCKS V2 if curation is heavy and human-driven. Mitigate by publishing the curation rubric and a "Strata Dataset Card" template (analogous to HuggingFace's dataset cards) on day one — so V2 user-published datasets can self-certify against it.**

3. **The Build-in-Public ICP Cohort Play.** Founder blog post in the voice of an AI-augmented builder, waitlist with optional survey, 10–20 user cohorts weekly. **V2-compatibility: ADVANCES V2.** Cohort users become the first uploaders and curators in V2.

**Launch week artifact lineup:**

- **Day 0 (Monday):** Founder blog post; waitlist live; MCP server published on Anthropic Registry, Smithery, PulseMCP; `npx add-strata` working across 9+ clients; 20–30 curated datasets in StrataHub; AGENTS.md/CLAUDE.md snippet on homepage.
- **Day 1:** Show HN — title that fits in a single line, founder responsive in thread.
- **Day 2:** 60-second demo video — Claude Code installs Strata via MCP, creates a branch to test a destructive query, reverts via time-travel, all autonomously. Posted to X with founder commentary.
- **Day 3:** Killer dataset spotlight — one of the 20–30 datasets gets a tweet with a 6-second loop showing an agent using it.
- **Day 4–5:** Founder hosts in-Slack/Discord office hours for the first cohort of 10–20 builders.
- **Day 7:** Recap post with first cohort's anonymized usage data; second cohort opens.

**Critical V1-to-V2 evaluation:**

- Heavy human curation in V1 *blocks* V2 if the rubric isn't codified. **Mitigation:** publish the rubric and Dataset Card template on day one.
- Founder-driven content motion in V1 *blocks* the long-tail ecosystem in V2. **Mitigation:** every artifact must be community-attributable and forkable.
- A single default-stack partnership is a single point of failure (cf. Replit-Neon). **Mitigation:** aim for two partnerships in V1, even if smaller.

### V1.5 — Visual Browser + Electron App for macOS (≈25% of GTM weight)

**Playbooks that newly unlock:**

- **The Manufactured Spectacle Play** becomes much more viable when a visual surface exists. The next frontier model release between V1 and V1.5 is the launch window. Plan a "Make Real for Databases" demo: an agent rewrites a real production schema while a human watches the visual diff and time-travels mid-operation.
- **The Template-Default Play** extends — vibecoder platforms can now show Strata content in their template galleries.
- **The Killer Dataset Wedge** sharpens — the visual browser makes datasets feel curated and shoppable, the way HuggingFace's model pages compound trust.

**What spectacle becomes possible:**

- Git-style branch visualization on a live database, developer toggling between "main" and "experiment" in real time.
- A "time travel" slider scrubbing the database backward visually while an agent works.
- The Electron app running entirely offline with air-gapped local inference, screen-recorded in a public-internet-disconnected room. The third demo is the most viral — air-gapped fits anti-cloud sentiment and AI safety conversations simultaneously.

**V1.5-to-V2 evaluation:** The visual browser is **strictly V2-advancing** — a registry without a browsable UI cannot become a flywheel. The Electron app is **partially V2-blocking** if it's the only way to browse — anyone without macOS and Electron is excluded. **Mitigation:** the in-Hub browser (web-based) must reach parity with the Electron app on browsing features; the Electron app's differentiation should be local execution and air-gapped inference, not browsing.

### V2 — User-Published Datasets (≈25% of GTM weight)

**Engineering the user-content flywheel.** HuggingFace's transition from curated to open uploads is the model. Three mechanics carry over directly:

1. **Social objects.** Likes, downloads, forks per dataset — visible, shareable, with anti-gaming mechanics (require API key for likes, prevent ratelimit-evading bot farms). HuggingFace's like/download counters are central to model discovery.

2. **Governance.** A Dataset Card template (provenance, license, schema, branchable-time-travel-aware metadata), spam filtering, takedown policy, namespace-authenticated upload via OAuth — Anthropic MCP Registry's namespace-authenticated approach is the template.

3. **The transition.** Don't flip the switch. Run V2 as "invite friends to publish" for 3–6 months, building governance experience on a small cohort before opening the floodgates.

**HuggingFace lessons applied:**

- The first user-published dataset that becomes a viral hit (like an outside developer's fine-tuned BERT in HF's early days) is the *real* V2 inflection. Plan operationally — homepage spotlight, founder retweet, "Featured this week" slot.
- Categories matter. HF's Datasets has tags for task, language, modality. StrataHub V2 needs tags for use-case (agent-memory, e-commerce, test fixtures), schema-style (KV, JSON, events, vectors, graph), and AI-readiness (machine-actionable schema, AGENTS.md-compatible).
- Top contributors become culture leaders. HF actively recognized and amplified its top contributors; Strata should do the same with a public leaderboard, recognition program, and direct relationships.

**V2 success metrics.** First 100 user-published datasets reaching cumulative 10K downloads each within 90 days; one user-published dataset becoming a viral artifact (≥100K downloads or ≥1M views on its associated demo).

---

## Anti-Patterns: Things Strata Should Explicitly Not Do

1. **Generic content marketing, webinars, paid SEO.** The discovery surface is now the LLM context window and the MCP registry, not Google or LinkedIn. Vercel's 10% of signups from ChatGPT referrals is the signal. Allocate that budget to GEO: llms.txt, llms-full.txt, AGENTS.md snippets, Smithery presence, comprehensive MCP server docs.

2. **Heavy LinkedIn thought-leadership or "AI-native" rebranding without a benchmark.** Bun and Astral earned adoption with measurable benchmarks (51%-faster-than-Deno, 10–100x-faster-than-pip), not category claims. Strata's play: a measurable benchmark — "Strata MCP server returns machine-actionable errors at lower latency than Supabase MCP" or "1ms branch creation regardless of database size."

3. **Premature open uploads in StrataHub V1.** Skipping V1 curation repeats spam-overrun failure modes. HuggingFace seeded with internal models; npm seeded with utility libraries; Docker Hub had explicit "official images." Curate V1 hard, codify the rubric, open V2 in waves.

4. **Trying to be the registry for MCP servers.** That's Smithery/PulseMCP/Anthropic Registry's job. Strata should be on those registries, not compete with them.

5. **Native source connectors.** Explicitly out of scope per the constraint, but worth restating: the moment Strata ships a Postgres or MongoDB connector, the MCP-only integration story becomes mush. Hold the line.

6. **Founder-personality-as-brand without technical substance.** The Adam Wathan / Charlie Marsh / Jarred Sumner model works because the founder ships and defends specific technical opinions. Pieter Levels-style hustle works for indie B2C, not infrastructure. Strata's founder voice must be technically argumentative (machine-actionable errors > human-readable errors, agent-first > developer-first, branches > snapshots).

7. **Free tier that subsidizes obvious abuse.** Make Real's choice to require the user's own OpenAI API key prevented viral cost explosion. Strata's free tier should soft-limit before abuse hits the company's wallet.

---

## RIGOROUS Section — Agent-as-User Documented Primitives

This section restricts itself to documented 2025–2026 mechanisms with named sources.

- **MCP install-as-distribution is measurable.** Microsoft Playwright MCP has 32.5k GitHub stars (per GitHub's stargazers page, May 2026) and is ranked #1 overall on PulseMCP with an estimated 51.5M total tool-call events and 2.1M this week (per PulseMCP's Playwright server listing, May 2026). Supabase MCP is ranked #1 in the Database category with ~18,600 weekly npm downloads and 2,663 GitHub stars (per AutomationSwitch's audit, May 3, 2026). Smithery's leaderboard top-5: Sequential Thinking, Context7, Exa Search, Supabase, GitHub. Aggregate ecosystem scale per Anthropic and CData: 97M+ monthly MCP SDK downloads, 5,800+ servers in the Anthropic Registry, 300+ MCP clients. MCP donated to the Linux Foundation December 9, 2025.

- **One-command multi-client installation is the new front door.** Neon's `npx add-mcp <url>` installs the same MCP across Claude Code, Claude Desktop, Codex, Cursor, Gemini CLI, Goose, OpenCode, VS Code, and Zed simultaneously — the install-time equivalent of `pip install` for the agent era.

- **Trigger phrases work.** Upstash markets Context7 with "Simply add `use context7` to your questions for accurate and relevant answers" and ships a default rule for Cursor/Claude Code that auto-invokes the server. The Claude Code plugin (`/plugin install context7-plugin@context7-marketplace`) ships a `documentation-lookup` skill that fires automatically — "eliminates the need to explicitly say 'use context7' in prompts."

- **AGENTS.md is in the system prompt.** Per OpenAI, "the codex-1 system message encourages Codex to run all tests mentioned in the AGENTS.md file." Adoption: 60,000+ open-source projects, including Amp, Codex, Cursor, Devin, Factory, Gemini CLI, GitHub Copilot, Jules, VS Code. Donated to Linux Foundation December 9, 2025.

- **llms.txt is read by IDE agents, not yet by ChatGPT/Claude bots in production.** Per Codersera's 300K-domain study, "GPTBot occasionally fetches, but rarely. ClaudeBot, Google-Extended, and PerplexityBot effectively don't." But Cursor, Continue, Cline, and Aider increasingly look for llms.txt when pointed at a documentation site. Mintlify co-developed llms-full.txt with Anthropic.

- **Database MCP servers are a defined and growing category.** Supabase MCP (top of category, OAuth + dynamic client registration); Neon MCP (claims first remote MCP server, at mcp.neon.tech, launched December 2024); Turso MCP (built directly into the `tursodb` binary via a `--mcp` flag); Convex MCP (ships with the Chef builder).

- **Default-stack wins are real, reversible, and high-velocity.** Bolt.new: every new Bolt project provisioned a Supabase database under Bolt's organizational account after September 30, 2025. Lovable: Supabase is the native default backend. Vercel: Neon is the canonical Postgres in the flagship template with Drizzle + better-auth + Shadcn UI + Neon MCP + neon-postgres agent skill pre-wired. Replit Agent: dropped Neon for Replit's own Postgres on December 4, 2025 — canonical reversibility example. Convex Chef: forked bolt.diy, ships Convex by default.

- **GEO produces measurable signups.** Vercel attributes 10% of new signups to ChatGPT referrals as a result of GEO efforts, per Mintlify.

- **AI coding agents recommend specific named tools mid-conversation.** Cursor recommending Supabase as a build-time backend, per Supabase's own MCP blog: "here is Cursor building a Next.js + Supabase app, fetching a Supabase URL and anonymous key, and saving them to a .env.local file." Replit Agent recommending Neon Postgres, per Neon: "Replit Agent is smart enough to recognize when your app needs persistent storage and will often add a Postgres database automatically." Context7 invoked by Cursor and Claude Code for documentation lookup as a default rule.

- **Security failure modes are documented.** CVE-2025-6514 in `mcp-remote` compromised 437,000+ developer environments via shell command injection. CVE-2025-49596 in Anthropic's MCP Inspector allowed browser-based attacks leading to RCE. These are the early-failure-mode references Strata's MCP server must avoid via OAuth, namespace verification, and explicit auth.

---

## SPECULATIVE Section — Composed Bets with 90-Day Kill Signals

These are bets that combine documented primitives into Strata-specific plays. Each is labeled with the assumption it depends on and the single observable signal that would confirm or kill it within 90 days.

1. **The "Strata Branch" verb bet.** *Assumption:* developers will type "branch this database" into Cursor/Claude Code if Strata is the only MCP that makes it work. **90-day kill signal:** if fewer than 25% of Strata MCP install events are followed by a `strata.branch.create` tool call within 7 days of install, the verb is not taking hold. (Kill it; retry with "snapshot" or "time-travel.")

2. **The "machine-actionable error" wedge bet.** *Assumption:* when an agent encounters a JSON error from Strata with a `suggested_next_call` field, downstream tool calls succeed at meaningfully higher rates than for competing MCPs. **90-day kill signal:** if A/B tests in a Mastra or CrewAI evaluation harness don't show ≥15% higher next-call success rate vs Supabase/Neon/Turso MCPs, the differentiation is marketing not product. (Kill it; commit to human-readable errors and compete on other axes.)

3. **The "air-gapped local inference for compliance-conscious vibecoders" bet.** *Assumption:* a non-trivial vibecoder sub-segment cares about offline-capable databases — privacy-paranoid builds, on-device personal-data apps, AI-safety-aligned demos. **90-day kill signal:** if fewer than 500 weekly active uses of Strata's local-inference mode 90 days after V1.5 ship, the air-gapped story is a press talking point, not a product wedge. (Don't kill the feature; deprioritize it in marketing.)

4. **The "AGENTS.md verb compounding" bet.** *Assumption:* published AGENTS.md snippets for Strata propagate organically through copy-pasted starter templates and become a self-reinforcing recommendation loop. **90-day kill signal:** if the Strata AGENTS.md snippet appears in fewer than 500 unique GitHub repositories within 90 days, organic compounding is too slow. (Kill the "wait and let it spread" approach; commit to direct partnership with template authors.)

5. **The "killer dataset becomes the demo" bet.** *Assumption:* one of the 20–30 V1 datasets generates a Make-Real-style viral demo within 30 days of launch. **90-day kill signal:** if no dataset has generated a third-party demo with ≥10K views, ≥500 forks of the originating repo, or ≥1K downloads within 30 days, the dataset wedge is not working. (Kill three of five least-engaging datasets; double down on the top two; reassess curation rubric.)

---

## Open Questions / Experiments

Five questions desk research cannot answer:

1. **Does the "machine-actionable error" advantage actually materialize as higher agent task-completion rates?** Set up an A/B harness using Mastra or LangChain with three identical agent tasks (CRUD against Strata, Supabase, Neon, Turso) and measure (a) total tokens consumed, (b) task success rate, (c) human-intervention rate. **Structure:** 4 databases × 3 task complexities × 100 trials = 1,200 trials, budget ~$2K in API costs, 2-week timeline.

2. **What is the right number of curated datasets to ship at V1?** Too few looks empty; too many dilutes the killer-first-dataset effect. **Structure:** ship 20 at launch, add 5/week, plot install-event-per-dataset curve, decide at week 8.

3. **Which vibecoder platform's default-stack slot has the highest 12-month value?** Mastra, CrewAI, Cline, Continue, Aider are all open. **Structure:** instrument a Strata adapter for two simultaneously, ship as opt-in, measure (a) usage from each installed base, (b) retention at 90 days, (c) referrals back to Strata's MCP server. Decide which to deepen at month 4.

4. **Is the V2 transition cliff worse than the V1 launch friction?** HuggingFace took years; npm took weeks; Docker Hub took months. **Structure:** at V1.5, run a private "user-published preview" with 10 invited cohort members; see what breaks (governance, spam, dataset-card completion rates) before V2 ships publicly. Inform V2 rollout cadence.

5. **Does Strata's "no native source connectors" constraint produce friction with V2 user uploads?** If users can't easily ingest existing data, the V2 long tail may starve. **Structure:** offer a one-off ETL service for the first 50 V2 publishers; if it produces sustainable upload volume, decide whether to relax the MCP-only constraint for ingestion (a strategic decision, not just tactical).

---

## Caveats

- **The reference companies cited here are mostly successes.** Survivorship bias is real; for every Cursor there are 50 forgotten editor forks. The mechanisms identified compound *when other things are true* — product genuinely better in a specific moment, ICP genuinely underserved, founder genuinely able to execute publicly.
- **MCP install/download numbers vary across directories and are often self-reported.** PulseMCP, Smithery, MCP.so, and Glama use different counting methodologies; vendor blogs sometimes round generously. Treat single-source numbers as directional, not exact.
- **Several 2026 articles are post-dated by content farms.** Numbers from low-quality sources have been flagged where used; primary sources (vendor blogs, GitHub repos, Anthropic/OpenAI announcements, Linux Foundation press) have been prioritized.
- **The AI agent landscape changes month-to-month.** Replit dropping Neon on December 4, 2025 demonstrates defaults are unstable. Any Strata partnership should be structured assuming defaults are 18-month leases, not permanent moats.
- **The agent-as-user dynamic is real but young.** Most documented examples are 12–18 months old at most. The 2027 picture may look meaningfully different — for example, if MCP gets superseded or if the LLM-mediated discovery surface gets centralized inside one vendor's tool catalog. Strata's MCP-only constraint is bet-the-company aligned with the current trajectory.
- **Strata's "no native source connectors" constraint is a real and legible bet.** It will produce friction for some users; the upside is forcing the agent-as-the-integration story to be load-bearing. If MCP loses momentum or if a major coding agent stops respecting MCP servers, the constraint becomes an existential risk.