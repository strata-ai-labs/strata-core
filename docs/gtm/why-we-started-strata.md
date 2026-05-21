# Why I started Strata

When I started Strata, the goal was simple: build the database the next wave of builders actually needs.

Databases are everywhere. Every app you use, every AI system you talk to, every business running on software depends on one. But for the next wave of builders — indie hackers, AI-augmented founders, internal teams, solo creators — what's available isn't what they need.

The defaults force a choice nobody should have to make. Either ship Postgres-with-pgvector and hope it holds. Or stitch together six cloud services the project never asked for. Either oversimplify and outgrow your stack by week eight, or overcomplicate and spend more on infrastructure than on the product. Either send your data to someone else's cloud, or build the storage layer yourself.

The current databases are built for one operator: human teams at corporate scale, making careful, deliberate writes against a linear timeline. That operator has changed. AI is rewriting who builds software, how fast, and what kinds of state they need. The data layer hasn't caught up.

At IBM, I led the launch of Db2 Genius Hub — the autonomous database for the agentic AI era. Agentic reasoning, multi-agent automation, supervised autonomy, air-gapped deployment. Genius Hub brought that future to enterprise: banks, regulated industries, DBAs running mission-critical workloads at scale.

But the same thesis applies just as hard at the other end of the market, and nothing there fits. These builders don't have DBAs. They can't justify the cloud overhead. They need a database as intelligent and autonomous as anything running in an enterprise data center — that also fits in one binary on their laptop, with no signup and no infrastructure tax.

That database didn't exist. So I built it.

Strata is one binary. It runs on a Raspberry Pi and it runs on a Xeon. Inside that binary: structured records, documents, events, vectors, and graph relationships, in one query layer. Git-style branching for agent writes. Millisecond time-travel for recovery. Local inference, so your data never leaves the machine unless you want it to.

Builders ship real applications on Strata without operating a stack. Their data stays with their app. Their agents can work freely because nothing they do is permanent. The substrate goes wherever they do.

When more people can build real things — without infrastructure tax or operational drag — more real problems in the world get solved. That's why this matters.
