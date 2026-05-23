# Strata GTM Strategy v0.1

**Status:** Internal — the adoption strategy for Strata. Decision-shaped: defines which avenues we invest in, in what order, and which we refuse.
**Audience:** Founder, future GTM hires, future engineering hires who need to understand why product roadmap and GTM strategy intersect.
**Not for:** External audiences. The master GTM doc and the philosophical foundation are the externally-shareable artifacts; this one is internal.

---

## The structural insight

Strata's GTM shape is **open-source-substrate**, not SaaS.

The right reference pattern is SQLite, Postgres, DuckDB, Redis.
The wrong reference pattern is Supabase, Firebase, Neon.

This is not a stylistic preference. It's a structural fact about the product. Strata ships as one binary, embedded in the user's app, with no signup, no cloud account, no per-seat pricing, no MQL pipeline. The product *is* the distribution. The only real conversion event is "install and ship something with it."

That structural fact eliminates a class of GTM moves that don't apply: signup-funnel optimization, trial-to-paid conversion campaigns, PLG cohort analysis, paid acquisition with LTV/CAC math, sales-assisted enterprise pipeline. None of those fit the shape of how Strata actually reaches its users.

It also reveals a different class of moves that *do* fit: distribution via embedding, distribution via demonstration, distribution via the ecosystem the substrate fits inside.

The strategy below is built on this shape, not against it.

---

## Adoption topology — three concentric rings

Strata's adoption is sequenced through three rings, in order. Each ring's audience is roughly 10x the previous, each ring's investment is roughly 10x, and each ring's time horizon is roughly 2x.

### Ring 1 — the first 1,000 builders

**Who they are:** Indie hackers, AI-augmented founders, AI builder community early adopters. The ones who already follow developer Twitter, read Hacker News, and try new things because they're new. Most of them have already heard the founder's voice on something.

**Where they live:** GitHub, Hacker News, Twitter / X, builder Discords (Indie Hackers, Build in Public, AI builder communities), AI Engineer-adjacent communities, Cursor and Claude Code subcultures.

**What they need to see:** A working product. A founder voice. Demos that pass the Tesla bar. Examples they can clone in a minute. Real apps already shipped on Strata. Honest engineering content.

**What success looks like:** 1,000 active builders — defined as people who have installed Strata and have shipped or are working on something real with it. 50+ public apps using Strata. Unsolicited media coverage (HN front page, indie newsletters, mentions in podcasts). AI coding agents starting to mention Strata, occasionally, when devs ask about database options.

**Time horizon:** Now → V1 launch + 6 months.

### Ring 2 — the next 10,000 builders (AI ecosystem)

**Who they are:** The broader AI-app-building population. Devs scaffolding AI projects in Cursor, Claude Code, Bolt, Lovable, v0, Replit. AI engineers at startups. Developers who heard about Strata from Ring 1's case studies and the AI tools they use every day.

**Where they live:** AI coding tools and their default scaffolds. AI starter templates. AI Engineer Summit. AI builder YouTube channels. AI-focused podcasts. Partnered AI dev platforms.

**What they need to see:** Strata recommended by AI agents when they ask "what database should I use?" Strata as a scaffold option in their AI app builder. Templates and starter kits for the specific patterns they're shipping. Stack-pair demos with the tools they already use.

**What success looks like:** 10,000 active builders. 500+ public apps. Strata showing up as a default scaffold in at least one major AI app builder. AI agent recommendation rate above 25% when a dev asks for an AI-app database.

**Time horizon:** V1 launch + 6 months → V1 + 18 months.

### Ring 3 — the next 100M builders (the long arc)

**Who they are:** The schoolchild in India. The teacher in rural Africa. The electrician in China. App-generation tool users globally. Language-local builder communities. Students and makers in non-SF geographies. Vibecoders building Excel-replacers, internal tools, personal apps, neighborhood tools.

**Where they live:** App generation tools (the ones we don't have yet, that are coming). Schools. Regional builder communities. Language-local YouTube and TikTok. StrataHub as discovery surface. AI-builder culture as it spreads from coastal tech hubs to everywhere.

**What they need to see:** Strata as the default substrate of app-gen tools. Documentation in their language. Templates relevant to their context. StrataHub as a place to find and fork live databases relevant to their work. Strata Build Day events near them.

**What success looks like:** 100K+ builders en route to 100M. Strata Build Day tradition established in 5+ cities globally. StrataHub has 1,000+ shareable databases. Translated docs and templates in 10+ languages. Local communities running on Strata for local-context applications.

**Time horizon:** V1 + 18 months → indefinite. This is the destination, not the starting point.

### The sequencing rule

**Don't skip rings.** Ring 3 is the destination, but Ring 1 has to hum first.

Most early-stage GTM mistakes come from trying to fish in Ring 2 or Ring 3 before Ring 1 has compounded. Compounding Ring 1 produces the case studies, language, and momentum that Ring 2 needs. Compounding Ring 2 produces the ecosystem integration and category clarity that Ring 3 needs.

If we ever feel pulled to do Ring 3 outreach before Ring 1 hums (e.g., a partnership with a country's education ministry, a Lagos university outreach pitch), the answer is no. Not because the outreach is wrong, but because we don't yet have the case studies that make the outreach credible. Premature Ring 3 wastes goodwill and produces no compounding effect.

---

## The filter for which avenues count

Most dev-tools GTM advice is generic. The filter we apply to every proposed avenue:

> **Would Postgres, SQLite, DuckDB, or Redis have benefited from this avenue?**

If yes, it's table stakes. We do it, but it isn't strategy.

If no — if the avenue is **uniquely available to Strata** because of the substrate position (cheap MVCC branching, time-travel, embedded shape), the AI-native architecture (MCP, local inference, self-understanding), or the vision (next 100M, globally) — then it's a structural lever and the strategy concentrates investment on it.

The ten structural levers below all pass this filter. Standard channels — GitHub repo, founder Twitter, HN posts, conferences, demos — come after, as supporting infrastructure.

The strategic mistake to avoid: treating table-stakes channels as strategy. Every dev-tools company has a GitHub repo and a Twitter account. Those don't make the company. The structural levers make the company.

---

## The ten structural levers

### Lever 1 — StrataHub as GitHub-for-state

Not a marketplace of templates. Actual live, branchable, time-travelable databases as shareable artifacts.

Someone publishes "every BLS jobs report" or "Hacker News 2007–now" or "anonymized SaaS metrics from my last company" or "the 50 most common AI agent memory patterns, as live databases" as a Strata file. Others fork it, branch it, query it, build on it. The dataset *is* the artifact — not a description of it, not a download link, but a live operational database you can run locally in one binary.

**Why this is structural:** This is to *state* what GitHub was to *code*. Nobody else can do it: cheap MVCC branching at the substrate level is what makes shareable live databases practical, and only Strata has that. Postgres can't do it (forks are expensive). SQLite can't do it (no branching). Supabase can't do it (their architecture is cloud-tied). DuckDB is closest in spirit but doesn't have branching.

**Why it compounds:** Every shared database is its own discovery surface. Every fork creates more. The 100th dataset published is more valuable than the 99th because it's more likely to find an audience inside an ecosystem that's already discoverable. The network effect is structural, not bolted on.

**What it requires from product:** StrataHub V1 — already in roadmap. Pre-V1 we can prototype with hand-curated datasets.

### Lever 2 — Strata-built AI agents that ARE the demos

Not "AI agents recommend Strata." Actually ship a set of Strata-built AI agents that demonstrate the substrate by being immediately useful.

Examples:
- A **"convert your back-end" agent** that scans an existing project (Postgres + pgvector + Redis + auth + signed URLs) and produces a one-file Strata equivalent.
- A **"show me my data over time" agent** that takes any database and gives it temporal vision — diff this week vs last, show me when this changed.
- A **"scaffold this AI app" agent** that produces a working RAG/agent/multimodal starter with Strata as the substrate.
- A **"recover from an agent mistake" agent** that uses Strata's time travel to undo bad writes from another agent.

Each is immediately useful AND a Tesla bar demo. The product builds the salesforce.

**Why this is structural:** Most databases can't be safely operated by an AI agent — too much risk of irreversible damage. Strata can, because branching and time-travel make every write reversible. The same architecture that makes Strata good for AI builders also makes Strata good at *being* an AI-built tool. The fit is recursive.

**Why it compounds:** Each agent demo is shareable, embeddable, forkable. Each one is a content artifact (video + repo + tweet). Each one creates a relationship with the AI builder audience.

**What it requires from product:** Strata AI capabilities (already in roadmap). Probably one engineer dedicated to demo-agent creation post-V1.

### Lever 3 — Author the AI recommendation explicitly

Don't *hope* Claude, Cursor, or ChatGPT recommends Strata. Publish the canonical "how to use Strata, for AI coding agents" guide — designed to be ingested by them. Submit it to Anthropic, OpenAI, Cursor, Bolt, Lovable, v0, Replit docs sites and starter-template programs. Build the MCP server. Ship in-package agent docs. Make fix-suggesting errors that effectively *train* the agent on how to use Strata correctly.

Make the AI-recommendation channel a deliberate engineering project, not a hope.

**Why this is structural:** Strata is the only database designed AI-agent-first. The recommendation guide writes itself because the design choices already line up with what an AI agent needs. No other database can publish this guide credibly — they'd be making claims their architecture doesn't back up.

**Why it compounds:** AI agents are now a routing layer between developers and infrastructure choices. Every time a dev asks "what database should I use for an AI app?", the agent's answer is doing the work of a thousand SEO pages, conference talks, and ad campaigns. Compounds because each model iteration tends to lock in the existing recommendation pattern.

**What it requires from product:** MCP-native integration, in-package agent docs, fix-suggesting errors, starter templates. All in roadmap; needs deliberate engineering attention.

### Lever 4 — Education over promotion

Don't sell Strata. Teach the category.

Sample piece titles:
- "What an AI-era database actually needs"
- "Why pgvector isn't the answer"
- "What time-travel actually means for AI debugging"
- "Why your AI agent's data layer is the bottleneck you don't see yet"
- "Embedded vs cloud: the real cost of the infrastructure tax"
- "The three primitives every AI app needs but nobody calls primitives"

Each piece teaches the gap. Strata is the only thing in the gap. This bends category instead of trying to sell into the existing one.

**Why this is structural:** The dominant market category for "AI app database" is currently routed to BaaS (Supabase + pgvector + auth + edge functions). The educational content reroutes the category by teaching what's actually missing — and what's missing happens to be everything Strata is. Promotion against the current category is brittle; education that defines the next category is durable.

**Why it compounds:** Educational content has long tails. A well-written "Why pgvector isn't the answer" piece gets read for years. Each piece improves the SERP for category-defining queries. Each piece becomes shareable inside the AI builder community.

**What it requires:** Founder voice (or a writing partner who can carry it). One serious piece per month is the baseline.

### Lever 5 — Founder documentary — the Strata Diaries

The founder records short videos throughout the decade arc of building Strata. Build / decide / ship / reflect. Compounds into lore.

This is the Stripe Press / Patrick Collison tier media asset. Two-minute videos shot on whatever camera is available. Topics: a release ships, a hard architectural decision, a quiet day of debugging, a conversation about why we refused a feature, an essay-in-video about a vision moment. Released as a series — weekly initially, then biweekly, then whatever cadence holds.

**Why this is structural:** The only company this makes sense for is one with a decade-scale vision, and the only person who can make it is the founder. The vision provides the dramatic arc; the founder provides the authenticity; the company arc provides the material. Most dev-tools companies don't have any of the three.

**Why it compounds:** Year 1 has 50 videos. Year 5 has 250. Year 10 has 500. By year 5 the back-catalog itself is a body of work — investors watch it, hires watch it, journalists draw from it. The asset compounds because each video adds context to the others.

**What it requires:** A camera, a microphone, 30 minutes per recording, the founder's commitment to cadence. Edit-light or unedited. Authenticity over polish.

**Care:** This works only if it's not corporate. If it ever becomes a marketing channel produced by a marketing team, it dies. It has to be the founder's voice, the founder's camera, the founder's thinking-aloud. Imperfection is the feature.

### Lever 6 — University author-tour in the geographies of the vision

Not Stanford. Not MIT. India, Brazil, Nigeria, Kenya, Vietnam, Indonesia, Mexico, Philippines, Egypt, Bangladesh.

The schoolchild, the teacher, the electrician are real people in real places. Go meet them. Talk at universities, at AI clubs, at developer meetups in cities the SF tech industry rarely visits. Tier-2 and tier-3 cities specifically.

**Why this is structural:** Nobody else has a vision pointed at these geographies as the destination. SF dev-tools companies do SF and EU and maybe Bangalore once. Strata's vision *names* these places. Going there is vision authenticity in physical form. It's also long-arc seed for Ring 3 — the universities in these geographies will produce the next decade's app builders, and they'll remember who showed up.

**Why it compounds:** Each visit creates a local champion. Each champion creates a local builder community. Each community creates regional case studies. Ten years in, Strata has a global builder community no SF-only competitor can match.

**What it requires:** Time and travel budget. Probably one major tour per quarter once V1 ships. Local partnerships to organize venues.

**Care:** Don't go as a foreign vendor pitching a product. Go as a founder telling the vision and helping local builders. The visit is in service of the local community, not the other way around.

### Lever 7 — The launch is not a blog post

When V1 ships, the launch itself is a category-bending act, not a blog post.

Three candidate shapes (not mutually exclusive, but pick one as the spine):

**A. Synchronized global build event.** 24 hours, 10 cities, 100 builders shipping the same day on Strata. Live demos, public showcase, every builder gets featured. The event becomes the artifact.

**B. Single immersive long-form video.** 45 minutes, first-person camera, one vibecoder going from idea to shipped app on Strata, no edits, no narration. Released as one piece. Becomes the canonical Strata demo.

**C. Non-SF city tour.** Launch event in Austin or Lisbon or Lagos or Bangalore, not San Francisco. Press, demos, local builders. The geography itself signals where Strata is pointed.

**Why this is structural:** Every other dev-tools company launches with a blog post + HN post + tweet. That's a single news cycle. Strata's launch should be a memorable artifact people refer back to for years. The artifact compounds; the news cycle doesn't.

**Why it compounds:** Whichever shape we pick, the resulting media asset (video, event recording, photo essay) becomes part of the permanent founder + company story. Year 3 we still link to it. Year 10 it's foundational lore.

**What it requires:** Production effort, real timing alignment with V1 readiness, willingness to delay the launch if the artifact isn't ready.

**Care:** Don't pick a shape that doesn't fit the founder's voice. If the long-form video isn't authentic, it's worse than no video. If the city tour feels staged, it's worse than no tour. The shape has to feel inevitable, not orchestrated.

### Lever 8 — Stack-pair demos — "substrate goes with everything"

When a relevant AI company ships something — edge model release (Cerebras, Groq, Mistral), agent framework launch (LangGraph, CrewAI), MCP server announcement, embedding model drop, AI app-builder feature, open-source model release — Strata ships a small demo within 24–48 hours showing *that product + Strata*. Video + tweet + GitHub example. They get amplified context; we get exposure to their audience. They often retweet.

**Why this is structural:** Strata is *complementary*, never competitive, with these companies. Edge models need state. Agent frameworks need state. RAG tools need state. AI app builders need state. *"X + Strata"* makes structural sense in a way *"X + Supabase"* doesn't, because Supabase competes for the same backend slot and Strata is the substrate underneath.

Strata's substrate features — branching, time-travel, local inference, MCP-native — map directly to demo gold for whatever these companies are announcing. Agent eval state. Model swap. Recovery from agent mistakes. Air-gapped edge inference.

**Why it compounds:**
- The AI release calendar produces a steady rhythm — something ships every week — so we never run out of demo material.
- After enough demos, companies start coming to us for collab content. The relationship flywheel turns inward.
- Each demo lives on as a permanent GitHub example, a permanent YouTube video, a permanent tweet thread. Year 1 produces 50 examples; Year 2 produces 100. The catalog itself becomes a discovery surface.

**What it requires:** A small team (or solo founder + one engineer) on rapid demo duty. Standard demo template: idea → Strata + Other → 90-second video → GitHub repo → tweet thread, done within 24–48 hours of the original release. Watch list of likely-to-ship companies with alerts.

**Care:**
- Don't pile onto controversy or companies in conflict.
- Don't make it look opportunistic. Each demo has to be genuinely useful on its own.
- Don't demo direct substrate competitors (rare, since substrate competitors are mostly Postgres/SQLite/DuckDB and pairing isn't natural).

### Lever 9 — Hackathon-led builder onboarding

In-person presence at AI builder population concentration points. Hackathons are the flagship; AI Engineer Summit, partner-hosted developer days, builder meetups in major cities are the supporting cast.

Why hackathons specifically fit Strata harder than they fit most dev tools:

- **The product's value proposition IS the hackathon timeline.** Strata's one-binary, no-config, no-cloud-setup is most visible against the 48-hour pressure of a hackathon. Postgres + pgvector + Redis + Inngest + Auth + signed URLs = real overhead in those 48 hours. Strata = zero overhead. The hackathon is the demo.
- **AI hackathons are population concentrations of the ICP.** Every major AI hackathon is hundreds of AI builders, indie hackers, AI-augmented founders in one room — exactly Strata's Ring 1 and Ring 2 audience.
- **Hackathon outputs are content gold.** Real apps shipped in 48 hours with Strata = case studies, video clips, social proof for months.

**What makes the Strata version different from logo-on-a-banner sponsorship:**

1. **Send engineers, not booths.** Strata team members at the hackathon as deep technical helpers. When a team says "we need vector search" or "the agent is making bad writes," a Strata engineer is there to solve it in five minutes. The substrate *feels* magical because someone's there to make it work. Trust at the hackathon converts to advocacy after.

2. **Pre-loaded starter kits.** Walk in with templates for the five highest-frequency AI app patterns. RAG chatbot. Agent with memory. Branching agent experiments. Time-aware app. Multimodal app. Each is 30 seconds to clone, 80% of the way to working. Time pressure makes this 10x more valuable than docs.

3. **Prizes designed to teach the category.** Not "best app using Strata" — that's generic. *"Best demo of agent branching."* *"Best use of time-travel for AI recovery."* *"Best edge inference + database combo."* The prize categories themselves teach what Strata is and frame Tesla-bar moments judges and audiences remember.

4. **Record everything.** Every team using Strata → 60-second clip → tweet, YouTube short, GitHub example. Each hackathon yields 10–20 amplifiable artifacts.

5. **The incentive structure has to fit a substrate, not a SaaS.** Cash prizes for "using our tool" land weird when the tool is free and open. The real incentives:
   - **Compute credits for Strata AI inference** (the part that has cost)
   - **Early access to features not yet released**
   - **Publication to StrataHub as a permanent shareable database** — their hackathon project becomes a forkable artifact that lives on (ties to Lever 1)
   - **A "Built at [Hackathon] with Strata" featured video series** — distribution, not swag
   - **Recruiter pipeline access** — companies watching Strata's hackathon showcase get first look at builders; builders know it

6. **Reverse hackathons. Strata Build Days in tier-2 cities.** Once there's enough community, host Strata's own events — Austin, Lagos, Bangalore, São Paulo, Lisbon. Cheaper than SF/NYC, hits builder populations the rest of the industry under-serves, vision-aligned with Ring 3 geographies.

**Why this is structural:** A booth-and-swag sponsorship is what every dev-tools company does. Engineers-in-the-room + starter-kits-pre-loaded + category-teaching-prizes + StrataHub-publication + tier-2-cities is a coherent program that builds compounding community and reaches the exact ICP at the exact moment they're trying to ship something with the product's promised shape.

**Care:**
- Don't sponsor without showing up. Logo-only sponsorships are wasted budget.
- Don't attend if V1 isn't ready enough for a 48-hour build to succeed. Failure at a hackathon is worse than absence.
- Don't go to every hackathon — pick high-leverage events. AI Engineer Hackathon, MLH-tier AI events, corp-run hackathons (Anthropic, OpenAI, Cerebras, Cursor). Skip the random ones.

### Lever 10 — The investor coalition as distribution channel

Don't just raise money. Engineer the cap table so every check is also a distribution channel.

The literal act of raising VC funding is table stakes — every startup does it. The structural version is **constructing a VC + operator coalition with intent**, treating cap-table composition as a product decision with the same care as a feature.

**Why this is structural for Strata specifically:**

- **Substrate companies have decade arcs.** The wrong VCs force premature SaaS-shape pivots — push for signup funnels, per-seat pricing, MQL pipelines that don't apply to substrate shape. The right ones hold the line on patient capital. Picking them is a deliberate filter, not just a fundraising step.
- **Category-bending requires investor pattern recognition.** Most VCs route "database" to BaaS. The few who see substrate position (Casado at a16z, Jerry Chen at Greylock, Lonsdale at 8VC, Sarah Guo at Conviction, Mike Volpi at Index) do category education for free by virtue of who they are. Their belief in Strata signals the category to the rest of the market.
- **Founder-operator angels become integration partners.** If Vercel founders, Replit founders, Cursor founders, Modal founders, Linear founders, Pinecone/Weaviate/Chroma founders, Datadog founders, Hashicorp / Docker founders, and AI-era operator angels (Elad Gil, Lachy Groom, Aaron Levie, Tobi Lütke, Solomon Hykes, Naval, etc.) write checks into Strata, that's both validation AND access. They integrate. They recommend. They route inbound. The cap table becomes a distribution map.
- **The Stripe pattern.** Stripe's early cap table — Sequoia + Thiel + Founders Fund + Y Combinator + 50 operator angels (Musk, Levchin, Brin, Levie, etc.) — wasn't just capital. It was a coalition that did distribution, hiring, and category education for years. Strata can replicate this pattern; the founder's existing network and the substrate position both support it.

**The strategic principle:**

Every check has a purpose: capital, distribution, signal, pattern recognition, partnership, hiring access. A single Sequoia check is worth less than 50 well-chosen operator angel checks if the goal is distribution. Usually we want both — institutional lead for capital + governance, operator syndicate for distribution + signal.

**Operational shape:**

1. **Pre-seed / seed:** heavy on founder + operator angels. 30–50 small checks from people whose companies are adjacent to Strata's ICP. Each angel's company is a potential integration, case study, or amplifier.
2. **Series A:** lead institutional VC chosen for substrate pattern recognition, plus the operator-angel coalition expanded — not diluted by passive capital.
3. **Subsequent rounds:** maintain operator-coalition density. Resist passive capital that dilutes the distribution role of the cap table.

**Target coalition shape (illustrative, not locked):**

- **Lead VC:** one of Sequoia / Benchmark / a16z (Casado) / Greylock (Chen) / Founders Fund / Index (Volpi). Substrate pattern recognition required.
- **Operator angels from companies Strata serves:** Vercel, Replit, Bolt/StackBlitz, Lovable, Cursor, Linear, Modal, Pinecone, Weaviate, Chroma, LangChain, LlamaIndex, Inkeep, Liveblocks. These are integration partners disguised as cap-table entries.
- **Founder angels from substrate / infra companies:** Solomon Hykes (Docker), Mitchell Hashimoto (Hashicorp), Edo Liberty (Pinecone), Olivier Pomel (Datadog), Guillaume Pousaz (Checkout.com), the Collisons (Stripe). Pattern-recognition angels who've built substrate before.
- **AI-era operator angels:** Elad Gil, Lachy Groom, Naval, Tobi Lütke, Aaron Levie, early Anthropic/OpenAI alumni who angel-invest.
- **Specialized infra / AI VCs:** Conviction (Sarah Guo), 8VC (Lonsdale), Radical Ventures, General Catalyst.

**Specific moves:**

- **Founder writes essays VCs route to each other.** Lever 4 (education) and Lever 5 (diaries) feed this — the writing is how the right investors find Strata before Strata pitches them.
- **Founder runs cap-table construction like a product.** Each candidate has a role: distribution, partnership, signal, hiring, governance. No purely-passive checks.
- **Founder is patient.** The wrong check accepted is worse than no check. Wrong-fit money locks in wrong-fit metrics and wrong-fit expectations.
- **Founder shows up at the dinners where these people already meet.** Selective; not socialite. The point is depth of relationship before the term sheet, not after.

**Why it compounds:**

- Each angel's company becomes a potential integration, case study, or recommendation channel.
- The investor coalition itself becomes a market signal that bends the category — when 30 well-known operators are on the cap table, the question "is this a real company?" answers itself.
- Inbound from partners, hires, journalists, and customers routes through investor introductions; the coalition becomes the company's social graph for everything downstream.

**Care:**

- Don't take money from VCs who push SaaS-shape on substrate-shape products. The Tesla bar test works on VCs too — if a candidate doesn't get the substrate position from a single demo, they'll push wrong metrics for years.
- Don't take from investors holding competitors in the same round (cap-table conflict).
- Don't optimize for valuation at the cost of investor quality. The right investor at a lower valuation is worth more than the wrong one at a higher one.
- Don't sprawl the operator coalition past usefulness. 30–50 well-chosen angels is more powerful than 200 random ones. Density matters; the angels have to know each other.
- Don't treat fundraising as one-time. Cap table construction is continuous — every round, every option-pool refresh, every secondary is a chance to refine the coalition.

**Where it slots in sequencing:** This is a **pre-V1 / now** lever. Cap-table construction starts before V1 ships and continues through every round. The right time to start conversations with the right investors is now — the founder essays and diaries already in motion are the right introduction surface.

---

## Standard channels (table stakes, not strategy)

These all run continuously in the background. None of them differentiates us, but each is required infrastructure.

- **GitHub repo.** README, examples, demo gifs, "install in 30 seconds" path, contribution guidelines, issue templates. Every other channel terminates here.
- **Founder Twitter / X.** Daily-to-weekly presence. Build-in-public posture. Replies to AI builder community.
- **Hacker News.** Launch posts at major moments. Founder-authored when appropriate. No paid amplification.
- **Demo videos on YouTube.** Short (90s) and long (10 min) cuts of the canonical demos.
- **Blog posts on the company site.** Long-form education and announcements.
- **Conference talks.** AI Engineer Summit first. Then AI builder conferences. Eventually database conferences.

These are necessary. They are not strategy. They support the nine structural levers.

---

## Avenues we refuse

- **Paid ads.** Waste at this stage. Ineffective for substrate-shaped distribution.
- **Enterprise sales.** Wrong category for V1. Reconsider post-V1 if the inbound is strong.
- **Comparison content vs Supabase / Firebase / Neon.** Violates the no-competitor framing and cements the BaaS routing we're trying to bend.
- **"Best database for X" SEO.** Routes into the BaaS category. The category we're bending is the one we shouldn't optimize for.
- **Foundry / native macOS app.** On ice for V1.
- **Influencer marketing.** Out of register for substrate.
- **Tradeshows we don't show up to in person.** Logo on a banner is wasted money.
- **Hackathons we don't staff with engineers.** Same.
- **General-purpose dev marketing newsletters.** Wrong shape; substrate moves through community, not newsletter list.
- **Premature partnerships.** Don't pursue Tier-1 AI dev platform partnerships before Ring 1 is humming — we won't have the leverage to negotiate good terms, and the integration will underperform.

---

## Sequencing

### Now (pre-V1)

- **Lever 3** — author the AI recommendation. Start the guide. Build the MCP server. Get docs in package.
- **Lever 4** — education content stream. One piece per month minimum.
- **Lever 5** — start recording the Strata Diaries. Don't wait for V1.
- **Lever 8** — modest stack-pair demo cadence with what V1 can already do. One per month, building rhythm.
- **Lever 9** — scout hackathons quietly. Attend without sponsoring. Meet builders. Gather feedback on what they actually need from a substrate.
- **Lever 10** — begin investor coalition construction. Map the target cap table (lead VC candidates + operator angel list). Open relationship conversations with the highest-signal operators first via essays and intros. Treat as a 12-month relationship project, not a 6-week raise.

### V1 launch

- **Lever 7** — execute the non-blog-post launch. Pick one of the three shapes early enough to produce well.
- **Lever 1** — first public StrataHub showcase. A few hand-curated live databases as launch artifacts.
- **Lever 9** — first sponsored, engineer-staffed appearance at a major AI hackathon. Treat as launch beat.
- **Lever 8** — cadence steps up to weekly stack-pair demos.
- **Lever 10** — leverage the launch artifact as the formal investor moment. The launch is the demo a VC has to see; structure intros and conversations around it.

### V1 + 90 days

- **Lever 2** — first Strata-built AI agents shipped as demos. The "convert your back-end" agent first.
- **Lever 9** — regular cadence. One major hackathon per month, engineer-staffed.
- **Lever 4** + **Lever 5** continue at established rhythms.
- **Lever 10** — close the seed / Series A coalition. Operator angels first; lead institutional last (the operator coalition increases lead-investor leverage).

### V1 + 6 months

- **First Strata Build Day** in a tier-2 city.
- **Lever 1** — StrataHub officially launched as GitHub-for-state with public publishing.
- Ring 1 → Ring 2 transition signals visible (AI agents starting to recommend Strata unsolicited; AI dev platforms starting to integrate).
- **Lever 10** — first round of operator-angel-driven integration partnerships activated (whichever portfolio companies are on the cap table become the first integration surfaces).

### V1 + 12 months

- **Lever 6** — university author-tour begins. One major geography per quarter.
- All levers running at sustained cadence.
- First major AI dev platform partnership (Replit / Bolt / Lovable / v0 / Vercel-style).

### V1 + 18 months and beyond

- Strata Build Day series running in 5+ cities.
- StrataHub catalog has hundreds of shareable databases.
- Documentation translated into 5+ languages.
- Ring 2 → Ring 3 transition signals appearing.
- **Lever 10** — Series B coalition expansion if growth justifies it. Maintain operator-coalition density across rounds; don't dilute the distribution role of the cap table.

---

## Metrics that fit substrate-shaped GTM

Substrate adoption doesn't fit SaaS funnel metrics. The metrics that matter:

### Ring 1

- **Active builders.** Defined as: people who installed Strata and shipped or are working on something real with it. Estimated through public app visibility, GitHub example forks, community engagement, voluntary check-ins.
- **Public apps in the wild.** Counted manually for the first 100; through StrataHub and community surface after.
- **GitHub metrics.** Stars (vanity baseline), forks (more meaningful), contributors (deepest signal).
- **Unsolicited media mentions.** Manual tracking of blog posts, podcasts, conf talks that mention Strata without us asking.
- **Tesla-bar moments observed.** Founder + team note when they hear "wait, how does that work?" or "why doesn't every database do this?" reactions in public. Qualitative signal but the most important one.

### Ring 2

- **AI-agent recommendation rate.** Measured through periodic three-AI testing (Claude / ChatGPT / Cursor / Bolt scaffold prompts). Track quarterly.
- **AI-tool ecosystem integration count.** How many AI dev platforms / agent frameworks / RAG tools have Strata as a first-class option.
- **Stack-pair demo amplification.** Retweet rate, view rate, GitHub example fork rate.
- **Hackathon ROI.** Builders converted per event (defined as installs that ship something real within 30 days).

### Ring 3

- **Geographic distribution of installs.** Country-level breakdown. Goal: at least 30% of installs from outside the US + Western Europe by V1 + 18 months.
- **StrataHub published databases.** Count, fork rate, fork depth.
- **Translated docs / templates count.** Both first-party and community-contributed.
- **Strata Build Day count.** Per year, per region.
- **Language-local community signal.** Existence of Discord / Telegram / WhatsApp groups in non-English languages discussing Strata.

### What we don't measure

- Signup funnel (none exists)
- Trial-to-paid conversion (substrate is free)
- MQLs / SQLs / sales pipeline (no enterprise motion)
- Per-seat pricing analytics (no per-seat pricing)
- Customer acquisition cost / LTV (substrate distribution doesn't fit this math)

If we ever find ourselves reaching for one of these metrics, it's a sign we've drifted toward SaaS shape and need to reorient.

---

## Risks and signals to watch

### Risk 1 — V1 ships before the Tesla bar is real

The strategy depends on demos that produce "wait, how does that work?" reactions. If V1 ships and demos don't pass the Tesla bar, the structural levers don't fire. Worse: launching against a weak Tesla bar can durably damage the brand.

**Signal to watch:** Tesla-bar moment count per demo session in private testing. If we're not consistently producing the reaction in a small audience, we're not ready to ship.

**Response:** Delay the launch over weakening the bar.

### Risk 2 — BaaS routing wins anyway

The market's current category for "AI app database" is BaaS. We're trying to bend that category. The three-AI test (Claude + ChatGPT + Grok / equivalent rotation) is the periodic measurement of whether the bending is working.

**Signal to watch:** Annual three-AI test. Is the routing improving? Are AI agents starting to mention Strata when devs ask for an AI database?

**Response:** If the routing isn't improving over 18 months post-V1, double down on Lever 3 (author the AI recommendation) and Lever 4 (education). Consider hiring a category-bending specialist.

### Risk 3 — Open-source distribution doesn't take off

Substrate distribution depends on installs and embeds. If those don't compound, the strategy doesn't work.

**Signal to watch:** Install rate, fork rate, contributor rate over time. Compounding pattern, not absolute numbers.

**Response:** If installs flatline, audit the GitHub front door first. Then audit Lever 3 (AI recommendation) and Lever 8 (stack-pair demos). The substrate has to be findable AND obviously useful to compound.

### Risk 4 — Founder voice cadence slips

Levers 4 (education), 5 (Strata Diaries), and most of the long-arc levers depend on sustained founder voice. The decade-arc only works if the cadence holds.

**Signal to watch:** Cadence of essays, diaries, talks. Quarterly review.

**Response:** If cadence slips, audit calendar and reallocate time. Hire a writing partner before hiring a marketing team. The voice can't be outsourced; the production assistance can be.

### Risk 5 — Hackathon presence dilutes

Levers 9 depends on quality not quantity. Going to too many hackathons or under-staffing them degrades the value of each appearance.

**Signal to watch:** Per-event ROI (builders converted, content artifacts produced, Tesla-bar moments observed).

**Response:** Refuse low-leverage events. Maintain quality bar over coverage bar.

### Risk 6 — Stack-pair demos look opportunistic

Lever 8 depends on relationships with third-party AI companies. If demos look like opportunistic ride-ons, those relationships break.

**Signal to watch:** Retweet rate from third parties. Relationship velocity (which companies are coming to us for collab demos).

**Response:** Audit demo quality. Each demo has to be genuinely useful on its own merit before it can be a relationship asset.

### Risk 7 — Premature Ring 2 or Ring 3 outreach

Easy mistake. A Vercel-style partnership offer, a country's education ministry interest, an AI hackathon invitation in a major venue. Any of these can pull us into Ring 2 or Ring 3 before Ring 1 is humming.

**Signal to watch:** Are we doing Ring 2/3 outreach without Ring 1 case studies? If yes, we're skipping.

**Response:** Refuse premature outreach. Track the inbound; pursue it when Ring 1 has compounded enough to make the outreach credible.

### Risk 8 — Wrong-fit investor capital

The single most consequential strategic mistake available to us. Wrong-fit investors push SaaS-shape metrics on substrate-shape products, force premature monetization, push for category-conforming positioning, and force pivots that erase the structural moves. One wrong board member can undo two years of strategic alignment.

**Signal to watch:** During fundraising conversations, listen for what investors talk about — if they're focused on signup funnels, MQL pipelines, per-seat ACV, "competitive landscape against Supabase," they don't see the substrate position. If they're focused on adoption velocity, ecosystem position, decade-arc compounding, they do.

**Response:** Refuse wrong-fit term sheets even at the cost of valuation or runway. The right investor at a lower valuation is worth more than the wrong one at a higher one. If the round can't close with the right investors, narrow the round, extend the runway, and continue building. The cap table is forever.

---

## Open questions

These don't have answers yet, but they're load-bearing for execution:

- **What is the open-source license?** AGPL vs MIT vs custom. Affects everything downstream — community contribution, commercial use, partnership terms.
- **What is the commercial model?** StrataHub paid tiers, Strata AI compute hosting, enterprise support, sponsorship — multiple paths plausible; pick the one that fits substrate shape (not SaaS).
- **Who is the second hire?** Engineer? GTM? Both critical; sequencing matters.
- **What is the launch readiness threshold?** When is V1 "ready" — feature complete? Tesla-bar passing in 80% of demos? First-100 builders successful?
- **What partnerships do we pursue first?** Replit, Bolt, Lovable, v0, Vercel, Cursor, Anthropic, OpenAI — which conversations do we initiate first?
- **What does the StrataHub launch look like?** Closed beta with curated datasets, then open? Or open from day one?
- **When does the seed / Series A round close?** Pre-V1 launch (build leverage from the launch artifact), or post-launch (let the launch produce inbound and close on stronger terms)? The launch-as-investor-moment framing argues for post-launch; the runway and hiring needs may argue for pre-launch.
- **Who is the lead institutional VC?** A small number of candidates pass the substrate pattern recognition test; depth of relationship with each needs to be built before the round opens.

---

## Related artifacts

- `strata-master-gtm-v0.1.md` — the master GTM doc; what Strata is, vision, tagline, features, positioning, FAQ.
- `strata-philosophical-foundation.md` — the depth under the vision.
- `why-we-started-strata.md` — the founder essay.
- `strata-vision-v0.2.md` — the canonical vision document.
- `strata-gtm-playbook-v0.1.md` — the execution layer; specific demos, scripts, surface-by-surface playbook. (To be drafted.)
- `memory/project_company_vision.md` — the committed vision.
- `memory/project_committed_tagline.md` — the committed tagline.
- `memory/project_tesla_bar_operational.md` — the Tesla bar criterion.
- `memory/project_company_scope_not_database_company.md` — the company-scope framing.

---

## How to use this document

- **For the founder:** the working strategy doc. Update as the strategy evolves. The structural levers should remain stable for at least a year; the sequencing and metrics adjust as we learn.
- **For future GTM hires:** the explanation of what we're doing and what we refuse. The filter (would Postgres/SQLite have benefited?) is the most important inheritance from this doc.
- **For future engineering hires:** the explanation of why some product roadmap items (StrataHub, Strata AI agents, MCP, in-package docs) are GTM-critical, not just product-critical.
- **For internal alignment moments:** when a new opportunity arises, route it through the rings + levers + refuse list. Most opportunities answer themselves once routed.

What this document is **not**:

- External marketing copy
- A pitch deck
- A press release
- A description of what we've already done

It is the internal map of where we're going and how. The external artifacts are downstream.
