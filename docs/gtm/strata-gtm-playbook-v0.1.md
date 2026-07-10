# Strata GTM Playbook

**Version:** 0.1
**Status:** Committed direction, ready for execution
**Audience:** Anyone writing external Strata copy, building demos, doing partnerships, or having conversations about what Strata is.

---

## 0. How to use this document

This playbook is the execution-level artifact for how Strata talks about itself to the world. It is downstream of two upstream artifacts:

- **The founder mission** (`memory/project_founder_mission.md`) — *Strata is the founder's foot forward in the AI democratization cycle, through the data layer.*
- **The vision document** (`docs/gtm/strata-vision-v0.2.md`) — *the internal source-of-truth on category, ICP, primitives, design principles.*

The vision document answers *what is Strata* and *why does it exist*. This playbook answers *what do we say, what do we show, what do we cut*.

The playbook is committed direction. Sections 1, 2, and 6 are not up for relitigation. Other sections will evolve as the product ships, but the spine — tagline, category-correction, Tesla bar — stays put.

---

## 1. The committed tagline

> **Database for the next 100 million builders.**

This is the external tagline for Strata. Use it on:
- Homepage headline
- Launch posts (Show HN, X threads, blog)
- Pitch decks (investor, hire, partnership)
- Talks and conference presentations
- Social bios
- Sales conversation openers
- Founder casual-conversation answer ("what are you working on?")

**Why this tagline:**
- Encompasses democratization (the founder mission)
- Signals ambition (the magnitude claim is itself a market position)
- Names the audience ("builders" — the people AI is unlocking)
- Future-oriented ("next")
- Covers the breadth of apps these builders will create

**Known cost (and why it's accepted):**

The three-AI test (Grok + ChatGPT + Claude) confirmed this tagline routes listeners to the BaaS/Supabase/Firebase category. Every AI extrapolated it as "managed Postgres-with-AI for indie builders." That is not what Strata is.

The cost is real and accepted. The product clears the Tesla bar (Section 6), which means once users experience Strata, the mis-routing self-corrects. The job between the tagline and that experience is what Sections 2 through 5 specify.

---

## 2. The first-line-after-tagline (category-correction)

The very next thing a listener sees, reads, or hears after the tagline must break the BaaS inference. Without this, listeners default to "Supabase/Firebase with AI features." With this, they pivot to "wait, this is something else."

**Approved first-line patterns:**

1. *"A complete database that runs anywhere — from a Raspberry Pi to a Xeon, with no signup, no connection string, no cloud overhead."*
2. *"Not a cloud backend. A complete data layer that ships with your app — from a solo project to a small SaaS, all on the same binary."*
3. *"Most apps don't need a cloud database. They need a complete one that lives with them."*
4. *"A database that ships with your app — not behind a connection string to someone else's cloud."*

Each one names what Strata IS (complete, embedded, ships-with-your-app) and what it ISN'T (cloud backend, behind a connection string, requires signup). Pick the one that fits the surface — homepage gets #1 or #2; a punchier social bio gets #3 or #4.

**The vocabulary that does this work:**

| Use repeatedly | Avoid |
| --- | --- |
| Complete | AI-native database |
| Embedded | Postgres for AI |
| Ships with your app | Backend-as-a-Service |
| Same binary | Cloud-native database |
| Anywhere | Serverless database |
| Lives with your app | Database platform |
| Not a cloud backend | The agent's database |
| No signup, no connection string | For agents |
| Most apps don't need... | Just like Supabase but… |
| One binary, every data shape | We compete with X |

The vocabulary is doing positioning work. Every external piece of copy should pass a quick check: does it use words from the left column? Does it avoid words from the right column?

---

## 3. The four canonical demo cases

These are the four use cases Strata's demos and customer stories should center on. Each one is a specific person/team that current data infrastructure mis-serves — and each one breaks the BaaS expectation when shown.

### 3.1 The AI agent startup at 50–100 users

**Who:** Founders or small engineering teams building an AI product at sub-Pinecone scale. Today they stitch together Postgres + a vector index + Redis + an event store + custom glue code.

**Pain shown:** *"I can't justify the cost or operational time of running a stack. But I need events, KV, vectors, and graph relationships. So I'm using only Postgres-with-pgvector and shipping something fragile."*

**Reaction-moment:** Multi-form unified — *"I stored events, KV, vectors, and graph relationships in one database, in one query plan, with one binary. Why doesn't every database do this?"*

### 3.2 The internal analytics dashboard from many sources

**Who:** Internal data engineering or BI teams pulling from many SaaS sources (Salesforce, GitHub, Stripe, Slack) into a unified queryable surface.

**Pain shown:** *"Each source has a different data shape. I either build a normalized warehouse (slow, expensive, lossy) or I leave them separate (no joins, no relationships, no unified queries)."*

**Reaction-moment:** Multi-shape natively + time travel — *"I queried across JSON, events, transactional, and conversational data in one query, and I time-traveled to see what last quarter's Stripe data looked like. Why doesn't every database do this?"*

### 3.3 The vibecoder building a calculator for their Excel sheet

**Who:** Solo indie hackers, citizen developers, AI-assisted builders replacing spreadsheet workflows with real applications.

**Pain shown:** *"I want my Excel sheet to be a real tool. But I don't want to host a cloud database, learn migrations, manage a stack, or pay for infrastructure on a side project that might never have a user."*

**Reaction-moment:** Embedded + complete — *"I built a calculator with a real database, it ships as one binary, runs on my laptop with no signup. Why doesn't every database do this?"*

### 3.4 The CEO's private mental model on her laptop

**Who:** Solo executives, privacy-conscious analysts, regulated environments, anyone whose data must not leave the device.

**Pain shown:** *"I want intelligence about my business — notes, relationships, what-if scenarios, AI-assisted analysis. But I will not send my business data to any cloud API."*

**Reaction-moment:** Native local inference + multi-form — *"I have notes, relationships, and what-if analyses with AI assistance — all running locally, completely offline, on my laptop. Why doesn't every database do this?"*

These four are the bank of canonical stories. Every external surface — homepage hero demo, launch post, talk, sales deck — should be built around one or more of them. Together they cover the breadth of the "100 million builders" claim while breaking the BaaS expectation cleanly.

---

## 4. The seven reaction-moments

Every Strata demo, in 60 seconds or less, must produce at least one of these reactions in the user:

1. **Branching.** *"I made changes on a branch, tested them, merged the good ones, threw away the bad ones. Why doesn't every database let me do this?"*
2. **Time travel.** *"I rolled the database back to ten minutes ago with one command. Why doesn't every database do this?"*
3. **Multi-form unified.** *"I stored a KV cache, documents, events, vectors, and graph relationships in one database, in one query plan, in one binary. Why doesn't every database do this?"*
4. **Embedded everywhere.** *"It runs on my laptop. Same binary runs on a Pi or on a Xeon. No signup. No connection string. Why doesn't every database do this?"*
5. **Native local inference.** *"I ran semantic search against my data with a model I downloaded from Hugging Face, completely offline. Why doesn't every database do this?"*
6. **Self-understanding.** *"The database told me it found three patterns I hadn't noticed in my data. Why doesn't every database do this?"*
7. **StrataHub clone.** *"I cloned a real dataset, branched it, ran my analysis on the branch, and the original is still pristine. Why doesn't every database do this?"*

Each moment is a single experience that produces the Tesla-feeling reaction. Demos should be designed to put users in front of one or two of these in the first 60 seconds — ideally first 10.

---

## 5. The Tesla bar (operational quality criterion)

> **"People will not have seen a database like this before. They will have to think: 'Why doesn't every database work like this?' That's the bar."**

This is the operational quality criterion for the product, the demos, and every external interaction.

**Concrete application:**

- **Every feature in V1** must have a corresponding reaction-moment, or it's supporting infrastructure (acceptable) rather than a headline feature.
- **Every demo** must produce one of the seven reactions in 60 seconds. If it doesn't, redesign the demo.
- **Every external surface** (homepage, talks, posts, sales calls) must contain at least one reaction-moment, shown — not described.
- **The first 60 seconds** of a new user using Strata is the highest-leverage interaction Strata will ever have with them. Optimize for that window.

If something is "good but not category-bending," it doesn't clear the bar. A feature that's marginally better than the alternative sustains the existing category; it doesn't expand it. Strata's whole position depends on category expansion — the Tesla bar protects that position.

---

## 6. Customer education through experience (the disruption-window reality)

The three-AI test confirmed: the market currently routes "database" to "cloud BaaS." This is not a failure of positioning — it is proof of disruption. The current category understanding cannot see Strata, because Strata is a different shape of thing than what "database" means in 2026.

**This means:**

- **The category education has to happen, and it can't happen in copy alone.** Copy can prepare users for the reaction; only the product can produce it.
- **Word of mouth IS the education.** Once a user has had the *"why doesn't every database work like this"* reaction, they explain Strata to their colleagues in their own words. That explanation carries the new category understanding with it — in the voice of someone who just experienced it, which lands far harder than marketing copy.
- **The job is slow.** The three-AI test gives you a measure of how far the category has bent. Today: 0%. Six months from now, with enough adoption and content: some percentage. The mis-routing diminishes as Strata accumulates real-world examples that the AIs eventually train on.
- **Patience is structural, not optional.** The disruption posture means slower category-formation in exchange for category-leadership at the end. Don't try to short-circuit this by softening the positioning into something the BaaS category accepts.

---

## 7. The three-AI test (validation methodology)

Before committing any new tagline, headline, or major positioning phrase, run it through three AIs as a cheap, fast validation:

1. **Grok** — pulls from current web context, good for catching saturation and current category routing.
2. **ChatGPT** — pulls from mainstream-developer intuition, good for catching generic-platform inferences.
3. **Claude** — pulls from positioning-aware analysis, good for catching category lineage and sibling-product comparisons.

Ask each: *"What comes to mind when you hear this?"*

**Interpretation:**

- All three route to SQLite / DuckDB / embedded / substrate territory → phrase is working.
- Any of the three routes to Supabase / Firebase / BaaS territory → phrase is mis-routing.
- All three call the phrase "vague" or "generic" → phrase has no positioning content.

Run this test for the homepage subhead, the launch-post opening, and any pitch-deck title. It's cheaper than focus groups and more reliable than internal opinion.

---

## 8. Surface-by-surface playbook

### Homepage

- **Headline:** *Database for the next 100 million builders.*
- **Subhead:** A first-line-after pattern from Section 2.
- **Hero:** One of the four canonical demos, autoplaying or interactive. First-load priority is to produce a reaction-moment in 10 seconds.
- **Below the fold:** The seven reaction-moments as labeled clips, each ~10 seconds. A visitor can scroll and see each moment land.
- **Footer:** Links to docs, GitHub, StrataHub.

### Launch post (Show HN / X thread / blog)

- **Opening line:** the tagline.
- **Second paragraph:** category-correction first-line.
- **Third paragraph:** founder mission framing ("AI is the next democratization cycle, like printing press / electricity / computers, and the data layer has to keep up").
- **Body:** walk through one or two of the four canonical demos, showing the reaction-moment in each.
- **Close:** *"Most apps don't need a cloud database. They need a complete one. That's what I built."*

### Pitch deck (investor / hire / partnership)

- **Slide 1:** Founder mission. *"AI is the next printing press. I know databases. I'm building the data layer for what's coming."* (Or: *"Every big technology wave needs new infrastructure. The printing press needed paper. Electricity needed grids. Computers needed chips. AI needs a data layer that keeps up. That's Strata."*)
- **Slide 2:** Tagline.
- **Slide 3:** Category correction — what Strata IS, what it ISN'T.
- **Slides 4–7:** The four canonical use cases as proof of breadth.
- **Slide 8:** The seven reaction-moments as proof of depth.
- **Slide 9:** The Tesla bar and how V1 feature decisions are made.
- **Slide 10:** Where we are, where we're going (mission applied to roadmap).

### Sales conversation / demo

- **Open:** the tagline.
- **Pivot:** to whichever of the four canonical use cases the listener relates to.
- **Walk through:** one or two reaction-moments live. Don't describe — show.
- **Close:** *"Try it. The first 60 seconds will tell you whether this matters to you."*

### Social bio / quick description

- Full: *"Database for the next 100 million builders. Complete, embedded, runs anywhere."*
- Shorter: *"Database for the next 100 million builders."*
- Founder bio: *"Building Strata — the database for the next 100 million builders. AI is going to change who gets to build things; the data layer has to keep up."*

### Founder casual answer ("what are you working on?")

- *"I'm building a database for the next 100 million builders. AI is going to let way more people build things — and most of them don't need a cloud database, they need a complete one that lives with their app. I know databases, so I'm building it."*

This is the dinner-party version. It carries the tagline, the mission, the category correction, and the personal stake — all in conversational form.

---

## 9. Anti-patterns

Things to avoid in any external Strata communication:

1. **Don't position as "AI database" or "AI-native database"** without immediately differentiating. These phrases route to BaaS-with-AI-features.
2. **Don't lead with feature lists** (multi-shape, branches, time travel, MCP) in first contact. Lead with the audience-and-tagline; feature lists belong after the reaction-moment is set up.
3. **Don't relitigate the tagline.** The phrase is committed. Energy on alternative taglines is energy not spent on the supporting material that has to do the category-correction work.
4. **Don't try to teach the category through copy alone.** Copy can prepare; only the product can teach. The Tesla bar is the actual teacher.
5. **Don't compare to Supabase / Firebase / Neon / Convex in external copy.** Active enemy positioning attracts a fight you don't want and signals that you're in their category, not above it.
6. **Don't ship demos that don't produce a reaction in 60 seconds.** A demo that explains is a missed opportunity; a demo that produces a reaction is a category-bending moment.
7. **Don't ship a feature that doesn't have a reaction-moment.** Polish, ergonomics, and quality-of-life features support the reaction-moments; they aren't substitutes for them.
8. **Don't soften the positioning to fit the BaaS category.** The discomfort of the mis-routing is the cost of being in the disruption window. Softening makes the cost permanent.
9. **Don't use "for agents" or "for vibecoders" in primary positioning.** "Agents" is saturated. "Vibecoders" is too narrow and pulls Strata into the application-layer category.

---

## 10. Open questions / next iterations

Things the playbook does not yet specify and that will need decisions as V1 ships:

1. **Pricing positioning.** The playbook doesn't address free/open-source/cloud-pricing yet. Should be added in v0.2 once decisions are made.
2. **StrataHub launch sequencing.** When does StrataHub enter the external story? V1 launch or post-V1?
3. **Comparative content.** At some point users will ask "how is this different from Postgres?" or "from Supabase?" The playbook currently says don't lead with comparisons in copy — but there should be a doc page that answers these questions for users who ask. Worth specifying.
4. **Developer relations cadence.** Talks, podcast appearances, hackathons — what's the cadence and which surfaces matter most? Open.
5. **Community surface.** Discord, GitHub Discussions, forum — which one is canonical? Open.

---

## Cross-references

- **Vision (internal):** `docs/gtm/strata-vision-v0.2.md` (Section 18's canonical wording predates the committed tagline; treat the playbook as the more current external surface)
- **Founder mission:** `memory/project_founder_mission.md`
- **Core thesis:** `memory/project_core_thesis_database_rethink.md`
- **Substrate positioning (internal):** `memory/project_strata_is_substrate_positioning.md`
- **Self-understanding architecture (hidden depth):** `docs/intelligence/self-understanding-architecture.md`
- **Disruption window (strategic context):** `memory/project_disruption_window.md`
- **Tesla bar (operational criterion):** `memory/project_tesla_bar_operational.md`
- **Committed tagline (this playbook's source):** `memory/project_committed_tagline.md`

When something in this playbook conflicts with the vision document, the playbook wins for *external* communication and the vision document wins for *internal* alignment. They are different artifacts for different purposes.
