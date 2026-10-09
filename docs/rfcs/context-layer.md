# RFC: the context layer

Status: proposed on 2026-10-08 from the research in [context-layer-research.md](../research/context-layer-research.md). Delivers promises 10 and 11 in [concepts.md](../concepts.md). Acceptance criteria CTXL-00 to CTXL-56 are at the end. The checkboxes go to the root README under Ready for Go once the owner approves CTXL-00. The product RFC is [mote](mote.md). The pack model this document feeds is [reuse/context.md](../reuse/context.md). The memory graph it links to is [reuse/memory.md](../reuse/memory.md).

## Key points

1. One context layer in mote-cloud reaches every source the owner lives in. Connectors push or poll into one normalized inbox, one ingest pipeline writes one index, and every lane reads through one retrieval planner with its own latency and token budget.
2. The coverage map is a first-class store, computed by code, and attached to every answer. It names what is connected, stale, denied, missing a scope, or reachable only while the Mac client is connected. Research shows a model will not abstain reliably on its own, so the daemon states the gaps.
3. Every connection has a registry row with its identity, tenant and realm (personal, work, a named client). Every indexed row carries its realm. A pack never mixes realms unless the owner asks.
4. The data model is a property graph with time. The engine is SQLite with FTS5, sqlite-vec and bounded petgraph walks. A 2-hop query at 1M edges measured 0.05 ms on the owner's Mac. LadybugDB is the runner-up and is measured in a spike before the choice is final.
5. Local stores (files, Apple Mail, Notes, Calendar, Messages, browser profiles, Spotlight, Overseer) are capabilities the Mac client lends to mote-cloud behind owner switches. The Pi stores nothing.
6. Talk gets full text plus vectors over a hot partition in 40 ms. Think adds the graph walk, filters and a search tool in 250 ms. Results fuse with reciprocal rank fusion, decay by age, and dedupe by document and simhash.
7. Evidence enters packs as cited parts with source pointers in cache-stable order, raw by default, summarized only when long, and the daemon maps every citation marker to a source.
8. Nothing in the index, inbox, registry or token store leaves the machine except redacted pack parts in a model call. Deletions at the source propagate within one ingest cycle.

## Status and scope

In scope: connectors, the event inbox, the connection registry and realms, the index, retrieval, consumption into packs, the coverage map, the storage engine decision, efficiency targets, and privacy. Out of scope: screen or audio capture, the relay that would let webhooks reach the Mac, and any cloud service. Those stay deferred behind their own criteria.

Dependencies: the event log (LOG-NN), redaction (CRATE-02), the pack model (CTX-NN), the memory graph (MEM-NN), the always-on watchers (ALWAYS-NN), and the think lane's `moted mcp` server (THINK-07). This document amends three of them: CTX-11's Slack limit is an upper bound for Marketplace apps, MEM-01 gains a `realm` column on nodes and edges, and ALWAYS-07 and ALWAYS-08 become inbox producers.

Topology, as set by the owner on 2026-10-08. mote has four parts. mote-cloud is the engine and holds all state, on the Mac today and on a server later. mote-client is the chat app on the Mac and the phone. When it runs on the Mac it lends local capabilities (signed-in browser sessions, files, Overseer) to mote-cloud behind owner switches. mote-ui is a stateless renderer. mote-talk is voice. The index, the registry and the coverage map live only in mote-cloud.

## Why

The owner's words:

- "Another important thing is accessing ALL the data efficiently. We need a layer of infrastructure for context retrieval and consumption."
- "Huge thing is its ability to recognize what it does not know / can't access."
- "I don't know about SQLite. Shouldn't we be using some graph based storage of context or something?"
- "how can we easily recognize which connection is which, and which ones we have, and how to connect them on separate layers of personal and work."

The first sentence asks for one layer, not one connector per lane. The second asks for a record of gaps that the model cannot invent. The third asks for an honest engine comparison. The fourth asks for a registry with realms.

## Architecture

```text
 mote-client (Mac)                         mote-client (phone)            mote-ui
 lends: files, Spotlight, Apple Mail,      chat only                      renders cards, the sources strip,
 Notes, Calendar, Messages, browser                                       the Connections screen
 profiles, browser session, Overseer
        |  inbox rows and bodies over the gateway (Unix socket today)
        v
 +------------------------------- mote-cloud (moted) ----------------------------------------+
 |                                                                                            |
 |  API connectors         inbox (one schema)        ingest pipeline       index.sqlite       |
 |  Slack, GitHub, IMAP,   push and reconcile,       redact, exclude,      doc, chunk,        |
 |  CalDAV, Linear,   -->  dedupe, priority     -->  dedupe, chunk,   -->  chunk_fts,         |
 |  Notion, web                  |                   embed, link           chunk_vec, link,   |
 |                               | priority 3 and 4                        mention, inbox,    |
 |                               v                                         stream             |
 |                        router (utterance)                                     |            |
 |                                                                               v            |
 |  registry (mote.sqlite)  <-->  coverage map  <---------------------  retrieval planner    |
 |  connection, realm,            states, freshness,                    talk: fts + vec hot  |
 |  capability, stream            windows, query log                    think: + walk + tool |
 |                                     |                                         |            |
 |                                     v                                         v            |
 |                        coverage part and sources strip  -->  context packs  -->  ledger    |
 +--------------------------------------------------------------------------------------------+
```

Where each lane reads:

| Lane | Reads | Budget | Deadline |
| --- | --- | --- | --- |
| Talk | FTS5 and vec0 over the hot partition of the thread's realm, the working set, the coverage summary | 8 items, 600 tokens | 40 ms p50, 80 ms p95 |
| Router | Nothing from the index. The turn text and one line of thread state. | 0 | 0 |
| Think | Everything talk reads, plus entity seeds, the bounded graph walk, the full vector partition, explicit filters, the `context_search` and `coverage` tools | 20 items, 4,000 tokens per turn | 250 ms p95 per retrieval |
| Work | Nothing directly. The delegate prompt carries the think lane's evidence pointers. | 0 | 0 |
| Watchers | Produce inbox rows. Read nothing. | 0 | 0 |

Three crates. `mote-connectors` owns the registry, realms, the inbox, the connectors and the stream cursors. `mote-index` owns `index.sqlite`, the ingest pipeline, retrieval, the graph walk and the coverage report. `mote-context` (from reuse/context.md) gains `evidence` and `coverage` part kinds. The Mac client's lent connectors are the same `mote-connectors` modules compiled into the client and talking to mote-cloud over the gateway.

## Proven systems to copy

The verdict column says whether mote embeds the system, runs it as a sidecar, copies its design, or skips it. Facts are in research sections 3, 5, 7 and 8.

| System | What it proves | What mote copies | What mote skips | Verdict |
| --- | --- | --- | --- | --- |
| Microsoft Graph delta queries and change notifications | Push tells you something changed, delta tells you what. Lifecycle events (`reauthorizationRequired`, `missed`, `subscriptionRemoved`) are first-class. `410 Gone` means full resync. | The two-channel shape (notification plus delta cursor), the lifecycle states as registry states, the resync rule | Graph itself until a Microsoft connector exists | copy design |
| Gmail `history.list` with `historyId`, `users.watch` through Pub/Sub | A monotonic history id with a short retention window, and a 404 that means "start over" | The history-id cursor shape and the 404 rule for the Gmail API option | Pub/Sub push, which needs a public endpoint and a GCP project | copy design |
| Google Drive `changes.list` page tokens, Calendar sync tokens | "The page token doesn't expire" for Drive. Calendar `syncToken` with deletes always included and `410` for a full resync. | The sync-token cursor and the 410 rule for the Google Calendar option | Drive until the owner asks for it | copy design |
| Slack Events API and Socket Mode, `conversations.history` cursors | Globally unique `event_id`, acknowledge within 3 s, three retries, a disconnect warning before refresh, `ts` as the message id | All of it, through slack-morphism, plus `message_changed` and `message_deleted` as update and tombstone rows | The HTTP Events API, which needs a public URL | embed |
| GitHub webhooks, the notifications endpoint with `X-Poll-Interval`, GraphQL | `X-GitHub-Delivery` is the dedupe key, 304 is free, `since` and `sort=updated` give incremental lists, GraphQL has a points budget | Polling through `gh api` with conditional requests, `since` cursors, delivery-id dedupe once a relay exists | Webhooks until the relay | copy design, `gh` as a subprocess |
| Linear and Notion webhooks | Signed payloads with ids only, retries over hours, out-of-order delivery, both require a public HTTPS endpoint | `updatedAt` and `last_edited_time` cursors, "fetch current data on event" | Webhooks until the relay | copy design |
| IMAP IDLE and CONDSTORE/QRESYNC, CalDAV sync-collection | IDLE re-issued every 29 minutes, UIDVALIDITY as the resync signal, MODSEQ for changes, `sync-token` with a precondition error for full resync | All of it through async-imap and libdav or mote's own REPORT | Nothing | embed |
| macOS FSEvents, Spotlight `mdfind`, Apple Mail, Notes and Messages stores | Event ids replay with `sinceWhen` and a volume UUID check. `mdfind` answers content queries in under a second. Apple's stores are SQLite but TCC-protected and undocumented. | FSEvents through notify with stored ids, `mdfind` for discovery, reading a copy of each store read-only from the Mac client with Full Disk Access | Reading the stores from mote-cloud, Spotlight as the index | embed (notify), sidecar (`mdfind`), lent capability (stores) |
| Airbyte | Sync modes, per-stream state committed only after the destination writes, at-least-once with primary-key dedupe, deletes only when the source emits them | The state rule, the dedupe rule, the mode names | The platform | copy design |
| Nango | A connection is one user's credentials for one integration, with end-user tags, typed `auth` and `sync` errors, and checkpoints | The registry row shape and the error types | The product (Elastic License, server) | copy design |
| Composio | Connected accounts with `ACTIVE`, `INACTIVE`, `REVOKED`, `EXPIRED` and a reason. Triggers are realtime or polling. | The status set with reasons, the realtime-or-polling split per source | The product | copy design |
| Merge | Linked accounts with `COMPLETE`, `INCOMPLETE`, `RELINK_NEEDED`, automatic webhooks with polling as the fallback | `RELINK_NEEDED` as `needs_reauth`, polling fallback | The product | copy design |
| Glean | Permissions mirrored per document and evaluated at query time. Three access modes: indexed, live, hybrid. Four crawl types: content, incremental, activity, identity. | Realm and identity on every row, the three access modes per source, the identity crawl as the registry's identity refresh | Hiding gaps. Glean hides what you cannot see. mote reports it. | copy design |
| Rewind and Limitless | Local-only capture with app and site exclusions and a disk budget. The product is gone, which proves the value of owning the store. | Exclusions per app, site and private window, the disk budget | Screen and audio capture | copy design |
| Khoj | Hash-based incremental indexing with deletion of missing hashes, a cross-encoder only when more than one hit exists | Both rules | The product (Python, AGPL) | copy design |
| Tantivy | Embeddable BM25 with phrase and fuzzy queries | Nothing in v0. FTS5 is already compiled in. | Tantivy until a ranking feature FTS5 lacks is needed | skip for now |
| Meilisearch | A good engine whose `milli` crate is unpublished and heavy | Nothing | The engine and the sidecar | skip |
| LanceDB | IVF partitions plus quantization, hybrid search with RRF | The quantize-then-rescore idea for large vector sets | The crate (Arrow and DataFusion) | copy design |
| sqlite-vec | Vectors inside the SQLite file with partition keys and metadata filters | All of it | Nothing | embed |
| Debezium CDC | Snapshot then stream, offsets committed with the write, at-least-once, tombstones, incremental snapshots in key-ordered chunks with watermarks | The reconciliation design in "Being aware of everything" | The platform | copy design |
| CloudEvents and Activity Streams | `source` plus `id` is the uniqueness key. `actor`, `object`, `target`, `published` name the shape of an activity. | The inbox id rule and the `kind` vocabulary | The envelopes themselves | copy design |
| Dust connectors | Webhook workflow id equals the event id so redeliveries are no-ops. Per-thread debounce of 10 s. GC after a full sync. | All three rules | TypeScript and Temporal | copy design |
| LlamaIndex and Haystack | Docstore hash upsert strategies, duplicate policies, RRF with k 60 | The upsert rules | The frameworks | copy design |
| Overseer account governance | Accounts not harnesses, a one-way fingerprint, a "follows app" login the daemon does not own | The account row, the fingerprint, "follows app" for `gh` and `claude` | Nothing | copy design |

## Connection registry and realms

The owner asked how to recognize which connection is which, which ones exist, and how to keep personal and work apart. The answer is one registry row per connection and one realm on every row the layer stores.

A realm is a named boundary. Default realms: `personal`, `work`, and one per named client. The owner can add more. Every connection belongs to exactly one realm. Profile facts about the owner belong to `shared`.

```sql
CREATE TABLE realm (
  id TEXT PRIMARY KEY,            -- 'personal', 'work', 'client:acme'
  name TEXT NOT NULL,
  kind TEXT NOT NULL,             -- 'personal' | 'work' | 'client'
  created_ms INTEGER NOT NULL
);

CREATE TABLE connection (
  id TEXT PRIMARY KEY,            -- 'slack:T024BE7LD', 'imap:fastmail:bilal', 'fs:projects'
  provider TEXT NOT NULL,         -- 'slack', 'github', 'imap', 'caldav', 'google', 'linear', 'notion', 'fs', 'apple_mail', 'apple_notes', 'apple_calendar', 'messages', 'browser', 'web', 'transcripts'
  kind TEXT NOT NULL,             -- 'api' | 'local' | 'lent'
  realm TEXT NOT NULL REFERENCES realm(id),
  identity_id TEXT NOT NULL,      -- Google sub, Microsoft oid, Slack user_id, GitHub login, mail address
  identity_display TEXT NOT NULL, -- what the Connections screen shows
  identity_fingerprint TEXT NOT NULL, -- sha256(provider + identity_id), first 16 hex. The only identity that enters events and the ledger.
  tenant_id TEXT,                 -- Slack team_id, Google hd, Microsoft tid, GitHub org, Notion workspace_id, Linear organization id
  tenant_display TEXT,
  scopes_granted TEXT NOT NULL,   -- JSON array as the provider reported it
  scopes_needed TEXT NOT NULL,    -- JSON array mote wants for full coverage
  token_ref TEXT,                 -- keychain item name, never the token. NULL for 'follows app' logins (gh, claude) and local stores.
  follows_app TEXT,               -- 'gh' | 'claude' when the login belongs to another tool
  lender_client TEXT,             -- client id for kind 'lent'
  state TEXT NOT NULL,            -- see the coverage map
  state_reason TEXT,
  last_ok_ms INTEGER,
  last_error TEXT,
  created_ms INTEGER NOT NULL,
  removed_ms INTEGER
);
```

Identity recognition at connect time. mote proposes a realm from signals the provider signs or returns, and the owner confirms or changes it before the first sync.

| Provider | Identity key | Tenant | Realm proposal |
| --- | --- | --- | --- |
| Google (OAuth) | ID token `sub`. Never `email`. | `hd` claim when present | `hd` present: work realm named by the domain. Absent: personal. The `hd` claim is trusted because it is inside the signed token. |
| Microsoft (later) | `oid` plus `tid` | `tid` | `tid` equals `9188040d-6c67-4c5b-b112-36a304b66dad`: personal. Otherwise work, named from `GET /organization`. |
| Slack | `auth.test` `user_id` | `team_id`, `enterprise_id` | One realm per workspace. The owner names it. |
| GitHub | `gh api user` `login` per host | `gh api user/memberships/orgs` | Repositories owned by an org the owner belongs to map to that org's realm. Personal repositories map to personal. |
| IMAP and CalDAV | user name and host | mail domain | Domain equals the work domain: work. Else personal. |
| Linear | `viewer { id organization { id urlKey } }` | organization id | work, named by `urlKey` |
| Notion | token exchange `owner` | `workspace_id`, `workspace_name` | named by workspace |
| Apple Mail, Calendar | account rows inside the local store | account address domain | per account, same rule as IMAP |
| Browser profiles | the profile directory | profile display name from `Local State` | per profile. Chrome keeps work and personal profiles apart already. |
| Files | path rule | none | per rule. Default: `~/projects/testbox/**` work, else personal. The owner edits the rules. |
| Transcripts | the project directory | none | the realm of the project path |
| Messages, Notes | the store | none | personal by default |

Every row the layer stores carries its realm: `doc.realm`, `chunk` through its doc, `chunk_vec.realm` as the vec0 partition key, `inbox.realm`, `stream.realm`, `mem_node.realm` and `mem_edge.realm` (added to MEM-01, default `shared`), and `query_log.realm`.

Retrieval filters by realm. A thread has a `realm_scope`. Default: the realm of the connection that produced the triggering inbox row, else the owner's active realm selection, else `personal`. The planner passes the scope to every lane as a partition constraint and a join filter. A pack never holds evidence, memory nodes or coverage lines from a realm outside the scope. The owner can set a thread to `all`, which the window shows as a badge and the event log records. The same person in two realms is two entity nodes until the owner merges them, and the merge is a reversible event.

Outward actions default to the thread's realm. The approval card names the realm and the acting identity ("post as @bilal in TestBox Slack"). A target in another realm needs an explicit owner choice on the card. A watcher can never choose.

The coverage map reports per realm. "Work: Slack fresh, GitHub fresh, mail 40 minutes stale. Personal: calendar not connected."

Removing a connection deletes its docs, chunks, vectors, inbox rows and keychain item, and keeps the registry row with `removed_ms` and the counts. Other realms are untouched.

Prior art for the design: Chrome profiles keep history and cookies apart per profile and enterprise policy can enforce a separate managed profile. Android's work profile has separate storage, blocks most cross-profile intents, badges work apps, and makes work data unreachable when the profile is off. Apple Focus filters let Mail show only chosen accounts and Calendar only chosen calendars per Focus. Nango keys a connection by integration plus connection id with end-user tags. Merge keys a linked account by `end_user_origin_id` plus category. Composio keys a connected account by user id plus auth config. Glean mirrors ACLs per document and evaluates them at query time. Microsoft Purview stamps one sensitivity label per item and inherits the highest label from attachments to mail. Overseer's account governance stores a provider, a named account, a one-way fingerprint and a status, and marks a login it does not own as "follows <app>". mote's registry is that row plus a realm.

## Connectors

Sync column: push means the source tells mote, poll means mote asks on a schedule, local means the Mac client reads a store. Every push source also has a reconciliation poll. Rate budgets are the research numbers for a single-workspace personal app. Where it runs: cloud means mote-cloud, client means the Mac client lends it.

| Source | Sync | Incremental cursor | Rate budget | Store locally versus fetch live | Freshness target | Token or permission | Runs in |
| --- | --- | --- | --- | --- | --- | --- | --- |
| Slack (messages, threads, files metadata) | Push through Socket Mode for `message` and `app_mention` in joined channels. Poll `conversations.history` per channel for backfill and reconciliation. `search.messages` live for think-lane queries. | `ts` per channel, `latest` cursor, `next_cursor` | Tier 3, 50 per minute per method. Internal apps keep Tier 3. | Store messages, threads, users, channel names. Fetch files live. | Push under 5 s. Reconcile every 15 min per active channel, 6 h per quiet channel. | Bot `xoxb-` with `channels:history`, `groups:history`, `im:history`, `mpim:history`. App-level `xapp-` with `connections:write`. User `xoxp-` with `search:read` and the history scopes for channels the bot is not in. | cloud |
| GitHub (issues, PRs, reviews, comments, notifications) | Poll notifications with `If-Modified-Since` and `X-Poll-Interval`. Poll issues and PRs with `since` and `sort=updated` per watched repository. | `Last-Modified` for notifications, `updated_at` high-water mark per repo | 5,000 per hour. A 304 is free. Search 30 per minute. | Store issues, PRs, comments, review state. Fetch diffs live. | Notifications at the poll interval (60 s). Repos every 5 min when active. | `gh` login, never read by mote. `gh auth status` must exit 0. | cloud, `gh` subprocess |
| GitHub code | Local clones through the file connector. `gh` live for files not cloned. | FSEvents ids per clone | none | Store chunks of tracked files. | 5 s after a write | Full Disk Access is not needed for `~/projects`. | client |
| Email (IMAP) | Push through IDLE on INBOX. Poll other folders every 15 min. QRESYNC when advertised. | `UIDVALIDITY`, `UIDNEXT`, `HIGHESTMODSEQ` per folder | Gmail allows 15 clients. One connection per account. | Store headers and bodies as text. Attachments metadata only in v0. | Push under 5 s. Folders 15 min. | App password or XOAUTH2 token in the keychain. | cloud |
| Email (Gmail API, optional) | Poll `history.list` every 60 s. No push (Pub/Sub needs a public endpoint). | `historyId` | 6,000 units per minute per user. `history.list` costs 2. | Same as IMAP plus labels. | 60 s | `gmail.readonly` on an OAuth client the owner creates. Personal gmail.com clients in Testing expire tokens after 7 days, so IMAP is the default. | cloud |
| Email (Apple Mail store) | Local. FSEvents on `~/Library/Mail/V10` plus a read of `Envelope Index` and `.emlx` files. | `messages.ROWID` high-water mark | none | Store headers and bodies. | 30 s | Full Disk Access on the Mac client. | client |
| Calendar (CalDAV: iCloud, Fastmail) | Poll `sync-collection` every 5 min. `ctag` fallback. | `sync-token` per calendar | none published | Store events with start, end, attendees, status. | 5 min | App-specific password in the keychain. | cloud |
| Calendar (Google API, work account) | Poll `events.list` with `syncToken` every 5 min. Push channels skipped (public HTTPS). | `syncToken` per calendar | 600 per minute per user | Same | 5 min | `calendar.readonly` on an Internal OAuth client of the Workspace org. | cloud |
| Calendar (Apple store) | Local. Read `Calendar.sqlitedb` from a copy. | row modification dates | none | Same | 5 min | Full Disk Access on the Mac client. | client |
| Files and notes (Markdown, text, PDF text, code) | Local. FSEvents through notify. Spotlight `mdfind -onlyin` for discovery on first run and after a UUID change. | FSEvents event id plus volume UUID | none | Store chunks. Binary files skipped. | 5 s | Full Disk Access only for paths outside the home folders macOS protects. | client |
| Apple Notes | Local. Read `NoteStore.sqlite` from a copy, decode `ZICNOTEDATA.ZDATA`. | `ZMODIFIEDDATE1` | none | Store notes as text. Locked notes skipped. | 60 s | Full Disk Access on the Mac client. | client |
| Browser history and bookmarks (Chrome, Brave, Safari) | Local. Read a copy of `History` and `Bookmarks` per profile every 5 min. | `visits.id` high-water mark per profile. Safari `history_visits.id`. | none | Store URL, title, visit times. No page content. | 5 min | Chrome and Brave: none. Safari: Full Disk Access. | client |
| Messages (opt-in, off by default) | Local. Read a copy of `chat.db`. | `message.ROWID` | none | Store text per chat. | 60 s | Full Disk Access on the Mac client, plus the owner's explicit switch. | client |
| Linear | Poll issues with `updatedAt: { gt }` and `orderBy: updatedAt` every 2 min. Webhooks later. | `updatedAt` high-water mark | 2,500 requests per hour, 3,000,000 points. | Store issues, comments, projects. | 2 min | Personal API key in the keychain, or OAuth `read`. | cloud |
| Notion | Poll `search` sorted by `last_edited_time` every 5 min, then block children for changed pages. Webhooks later. | `last_edited_time` high-water mark | 180 requests per minute | Store page text and properties for pages shared with the integration. | 5 min | Internal integration token in the keychain. | cloud |
| Web pages | On demand. Fetch bookmarks and links found in indexed documents of the same realm, and owner requests. | `fetched_ms` plus ETag | 1 request per second per host, robots honoured | Store extracted main text with a TTL. | On request, refresh after 7 days | None. A lent browser session only for allowlisted domains behind a switch. | cloud, client for lent sessions |
| Agent transcripts (Claude Code, Codex, Overseer event log) | Local. FSEvents on `~/.claude/projects` and `~/.codex`. Overseer over its socket. | file offsets per JSONL, Overseer cursor | none | Store user and assistant text only. Tool outputs excluded. | 30 s | None | client (files), cloud (Overseer) |
| Microsoft 365 (later) | Graph change notifications plus delta queries | `deltaLink` | per Graph | Same shapes as mail and calendar | per resource | OAuth with `Mail.Read`, `Calendars.Read` | cloud |

Rules that apply to every connector:

- Reads only. No connector holds a write-capable client. Outward actions go through the approval gate (ALWAYS-09).
- Every API call that fetches content writes a snapshot row with `fetched_ms`, validator and TTL (CTX-11).
- Backfill runs newest first in windows: 7 days, 30 days, 90 days, then everything, one stream at a time per connection, and yields to turns.
- A connector that fails restarts with backoff from 1 s to 60 s and emits `watcher_restart` (ALWAYS-13). After 3 consecutive auth failures the registry state becomes `needs_reauth`.
- A lent connector reports through the client. When no Mac client is connected, every lent connection is `client_offline` and its freshness clock keeps running.

## Being aware of everything

Every connector emits events of one schema into one inbox. The inbox is the only way data enters the index and the only way a source wakes the router.

```sql
CREATE TABLE inbox (
  id TEXT PRIMARY KEY,            -- '<connection_id>/<source event id>'. CloudEvents' source plus id rule.
  connection_id TEXT NOT NULL REFERENCES connection(id),
  realm TEXT NOT NULL,
  account TEXT NOT NULL,          -- identity fingerprint
  kind TEXT NOT NULL,             -- 'message.created', 'message.updated', 'issue.updated', 'mail.received', 'event.changed', 'file.changed', 'page.updated', 'visit.recorded', 'tombstone'
  stream TEXT NOT NULL,           -- 'slack:C024BE91L', 'imap:INBOX', 'fs:/Users/bilal/projects', 'gh:TestBoxLab/monarch'
  cursor TEXT,                    -- the source cursor after this event
  event_ms INTEGER NOT NULL,      -- when it happened at the source
  observed_ms INTEGER NOT NULL,   -- when mote saw it
  pointer TEXT NOT NULL,          -- source-native locator: permalink, UID, path, URL, ROWID
  priority INTEGER NOT NULL,      -- 0 bulk, 1 watched, 2 assigned or thread I am in, 3 direct message, 4 mention of me
  origin TEXT NOT NULL,           -- 'push' | 'poll' | 'reconcile' | 'backfill'
  payload_hash TEXT NOT NULL,     -- sha256 of the redacted payload
  state TEXT NOT NULL             -- 'new' | 'indexed' | 'skipped' | 'failed'
);
CREATE INDEX inbox_state ON inbox(state, priority DESC, observed_ms);
CREATE INDEX inbox_stream ON inbox(stream, event_ms);
```

Push where the source offers it, reconciliation polls where it does not:

| Source | Push | Reconciliation | What reconciliation catches |
| --- | --- | --- | --- |
| Slack | Socket Mode envelopes, acknowledged by `envelope_id` within 3 s | `conversations.history` per channel from the stored `ts` | Messages sent while the socket was down, edits, deletions |
| GitHub | none until the relay | notifications poll, `since` lists | Everything |
| IMAP | IDLE on INBOX | `UID FETCH` since `UIDNEXT`, `VANISHED` or a UID set diff per folder | Mail in other folders, expunges |
| CalDAV and Google Calendar | none | `sync-collection`, `syncToken` | Everything, including cancellations |
| Files | FSEvents | a key-ordered walk per root every 7 days, and on `MustScanSubDirs` or a UUID change | Dropped events, moves, deletions |
| Apple stores, browsers | none | a copy-and-diff every 60 s to 5 min | Everything |
| Linear, Notion | none until the relay | `updatedAt`, `last_edited_time` polls, plus a full listing every 24 h | Deletions, which polls never show |
| Web | none | TTL refresh | Changed pages |

Reconciliation copies Debezium. The incremental cursor is the stream. The full listing is the snapshot. A full listing walks the remote collection in key order with resumable progress and runs while incremental polling continues. A newer incremental row for the same id wins over the listing row. After a listing completes, the stream diffs remote ids against local ids and emits `tombstone` rows for ids that vanished. Every write is an upsert on `(connection_id, external_id)` because every input is at least once.

Dedupe. The primary key is the source's own event id where it has one (Slack `event_id`, GitHub `X-GitHub-Delivery`, Linear `Linear-Delivery`, Notion `id`), else a deterministic id from the pointer plus the source change marker (`ts`, UID plus MODSEQ, FSEvents id, `visits.id`). A redelivered event updates `observed_ms` and nothing else. An edit is a new row with kind `*.updated` and the same pointer. A deletion is a `tombstone` row.

Priority. Priority 4 is a mention of the owner. Priority 3 is a direct message or an assignment to the owner. Priority 2 is activity in a thread, issue or PR the owner took part in. Priority 1 is a watched channel, repository, folder or calendar. Priority 0 is bulk backfill and browser history.

How the router consumes it. Rows at priority 3 and 4 become one `utterance` event with source `watcher:<connection_id>` within 1 s, as ALWAYS-07 and ALWAYS-08 already describe. Rows at priority 2 batch into one digest card per hour per realm. Rows at 0 and 1 only index. Wakes are capped at 10 per minute per realm and overflow merges into the digest. The attention detector (MOTE-22) still scores every wake. Nothing from the inbox acts outward.

Prior art. Microsoft Graph separates the notification (a resource changed) from the delta query (what changed) and adds lifecycle events for missed notifications and expired authorizations. Slack's `event_id` is "globally unique across all workspaces", retries carry `x-slack-retry-num`, and Socket Mode asks for an acknowledgement per envelope. GitHub keeps the same `X-GitHub-Delivery` on redelivery so it is the dedupe key. Dust makes the webhook workflow id equal to the event id so redeliveries are no-ops. mote's inbox id does the same.

## Index

One logical index over everything, in one file, `index.sqlite`, beside `mote.sqlite`. The reason for a second file: an ingest transaction can take seconds, and the event log's writer must append in under 1 ms (LOG-09). Two files mean two writer connections and no contention. Cross-file consistency does not need atomicity because every index write is an idempotent upsert keyed by the inbox row, and a nightly reconciler repairs dangling references.

```sql
CREATE TABLE doc (
  id INTEGER PRIMARY KEY,
  connection_id TEXT NOT NULL,
  realm TEXT NOT NULL,
  kind TEXT NOT NULL,             -- 'message', 'thread', 'mail', 'event', 'issue', 'pr', 'file', 'note', 'page', 'visit', 'transcript'
  stream TEXT NOT NULL,
  external_id TEXT NOT NULL,
  title TEXT,
  author TEXT,                    -- identity fingerprint or display handle, redacted
  pointer TEXT NOT NULL,          -- opens the source: permalink, file path, mailto with UID, URL
  event_ms INTEGER NOT NULL,      -- source time
  updated_ms INTEGER NOT NULL,    -- last source change
  observed_ms INTEGER NOT NULL,   -- when mote indexed it
  content_hash TEXT NOT NULL,     -- sha256 of redacted text
  simhash INTEGER NOT NULL,       -- 64-bit, near-duplicate detection
  bytes INTEGER NOT NULL,
  tombstone_ms INTEGER,
  inbox_id TEXT NOT NULL,         -- the row that produced this version
  UNIQUE(connection_id, external_id)
);
CREATE INDEX doc_time ON doc(realm, kind, event_ms);

CREATE TABLE chunk (
  id INTEGER PRIMARY KEY,
  doc_id INTEGER NOT NULL REFERENCES doc(id),
  position INTEGER NOT NULL,
  text TEXT NOT NULL,             -- redacted, with the situating header
  tokens_est INTEGER NOT NULL,
  start_char INTEGER NOT NULL,
  end_char INTEGER NOT NULL,
  hash TEXT NOT NULL
);

CREATE VIRTUAL TABLE chunk_fts USING fts5(
  title, text, content='chunk_fts_view', content_rowid='id', tokenize='porter unicode61'
);

CREATE VIRTUAL TABLE chunk_vec USING vec0(
  chunk_id INTEGER PRIMARY KEY,
  realm TEXT PARTITION KEY,
  tier TEXT PARTITION KEY,        -- 'hot' (last 60 days or pinned) | 'cold'
  kind TEXT,                      -- metadata column, 12 characters or fewer
  event_ms INTEGER,               -- metadata column, time filters
  embedding int8[256] distance_metric=cosine
);

CREATE TABLE link (src_doc INTEGER NOT NULL, dst_doc INTEGER NOT NULL, rel TEXT NOT NULL,
  PRIMARY KEY (src_doc, dst_doc, rel));            -- 'reply_to', 'in_thread', 'pr_for', 'attachment_of', 'visited', 'duplicate_of'
CREATE TABLE mention (chunk_id INTEGER NOT NULL, node_id INTEGER NOT NULL, start_char INTEGER, end_char INTEGER,
  how TEXT NOT NULL, PRIMARY KEY (chunk_id, node_id));  -- node_id is mem_node.id in mote.sqlite. how: 'handle', 'email', 'repo', 'channel', 'reflect'
CREATE TABLE stream (id TEXT PRIMARY KEY, connection_id TEXT NOT NULL, realm TEXT NOT NULL, mode TEXT NOT NULL,
  cursor TEXT, covered_from_ms INTEGER, covered_to_ms INTEGER, last_push_ms INTEGER, last_poll_ms INTEGER,
  last_ok_ms INTEGER, last_error TEXT, backfill_state TEXT, items INTEGER NOT NULL DEFAULT 0);
```

Entities and relations stay in the memory graph (`mem_node`, `mem_edge` in `mote.sqlite`, MEM-01) so that one graph serves memories and documents. `mention` links chunks to entity nodes. Document-to-document structure lives in `link`. Time lives on every doc and on every bi-temporal edge.

Ingestion, per inbox row, in order:

1. Fetch the body. A push payload carries it, or the connector fetches by pointer with a snapshot row.
2. Normalize into a `Doc` with the fields above. Convert source epochs (Apple 2001 seconds or nanoseconds, WebKit microseconds since 1601, IMAP dates) to unix milliseconds.
3. Redact with `mote-redact` and the owner's extra patterns. Hash the redacted text.
4. Exclude. Opt-out lists per connection, stream, path glob, domain and kind. Secret-bearing file names. Binary files. Documents over 2 MB of text are cut to 2 MB with a `truncated` flag.
5. Dedupe. An equal `content_hash` for the same `(connection_id, external_id)` is a no-op. A different hash replaces the document's chunks and vectors in one transaction. A simhash within Hamming distance 3 of another document in the same realm links `duplicate_of` and indexes only the canonical document.
6. Chunk. Recursive splitting at 400 tokens with no overlap, following Chroma's measurement. Each chunk gets a situating header with no model call: title, author, source and date, which is contextual retrieval without the LLM. Messages shorter than 40 tokens join their thread neighbours into windows up to 400 tokens. Calendar events, visits and short notes are one chunk each. Code splits on blank-line boundaries at 400 tokens in v0.
7. Embed with model2vec `potion-base-8M` into `int8[256]`, in batches of 256. The model is the one pinned by hash in MEM-10. No network.
8. Write in one transaction per batch of at most 256 chunks on the index writer.
9. Link entities by canonical key only: handles, email addresses, repository names, channel names, calendar attendees, file paths under a project root. Missing entities are created as `entity` nodes with the document's realm. No LLM extraction here. The reflect job (memory doc open decision 4) may add semantic edges later.
10. Mark the inbox row `indexed`, advance the stream cursor in the same transaction as the last batch of that inbox row, and emit one `indexed` event per 100 documents with counts only.

Ingest yields to turns. While a talk turn runs, the ingest loop pauses within 50 ms and resumes when the turn ends. Backfill uses one core at most.

Where ingestion runs. On the Mac today, inside moted, with the lent connectors running in the Mac client process and streaming inbox rows and bodies over the Unix socket. When mote-cloud moves to a server, the same client modules stream over the gateway. The Pi runs a client and stores nothing.

## Retrieval

The planner takes a query, a lane, a thread (which gives the realm scope and the working set), optional explicit filters, and a deadline, and returns ranked evidence plus a coverage report.

| Lane | Candidates | Fusion and ranking | Cut | Deadline |
| --- | --- | --- | --- | --- |
| Talk | FTS5 BM25 top 40 (title weight 2.0), vec0 KNN top 40 over partitions `(realm in scope, tier = hot)`, working-set documents | RRF with k 60, times recency decay, times 1.5 for working-set members | 8 items, 600 tokens, at most 2 chunks per document | 40 ms p50, 80 ms p95 |
| Think | Talk's candidates over both tiers, entity seeds from the working set and canonical-key matches, a bounded graph walk, explicit filters | RRF across up to 4 lists, decay, working-set boost, then an optional cross-encoder rerank when the `rerank` feature is on and 60 ms remain | 20 items, 4,000 tokens, at most 3 chunks per document | 250 ms p95 |
| `context_search` tool | The think planner with the tool's filters: `realm`, `sources`, `kinds`, `since`, `until`, `entity`, `k` | same | as requested, under 4,000 tokens | 250 ms per call |

Recency decay. Score times `exp(ln(0.5) * age / half_life)` with `age` from `event_ms`, following Elasticsearch's `exp` function with `decay` 0.5. Half-lives per kind: message 7 days, mail 14 days, issue and PR 14 days, file and code 30 days, note and page 90 days, transcript 14 days. Calendar events use a window instead: full score within 7 days before or after now, then the same decay.

Graph walk, think lane only. Seeds are entity nodes from the working set plus canonical-key matches in the query filters. Load edges within 2 hops of the seeds that are valid now, at most 20 edges per node ordered by recency times weight, at most 200 edges in total, with four indexed lookups per direction and never a materialized union view. Run Personalized PageRank for 20 iterations in petgraph over that neighbourhood. Return the top documents mentioned by the top-ranked nodes as a fourth list. The measurements behind the bound: a bounded 2-hop load takes 0.03 to 0.07 ms at 1M edges on the owner's Mac, and an unbounded 3-hop from a 4,000-edge hub takes 310 ms, which would eat the whole budget.

Dedupe. One result per document unless chunks are non-adjacent, then at most 2 (talk) or 3 (think). Documents linked `duplicate_of` collapse to the canonical one. Two results whose simhash is within Hamming 3 collapse to the higher-scored one.

Source and time filters. The think lane and the tool take explicit filters only. The planner never classifies natural-language text with regexes. The owner's query filters (`in:slack`, `since:2026-09`) are parsed as structured tokens the UI also offers as chips.

Hit quality, computed by code for the coverage report. Pre-retrieval: `MaxIDF` over the query terms from `fts5vocab`. Post-retrieval: NQC over the top 10 BM25 scores and the top cosine similarity. Verdict `strong` when the top cosine is at least the strong threshold or NQC is above its threshold, `weak` when candidates exist below those lines, `none` when no candidate passes the floor. The thresholds are learned on the LongMemEval replay, whose abstention subset has questions the history cannot answer. The verdict never comes from a model.

Deadline. A lane that misses its deadline returns what is ready and marks the pack `partial` (MEM-07).

Vector scale. The talk lane searches the hot partition only, which stays under 100,000 vectors through the 60-day tier rule. The think lane scans both tiers. Above 250,000 vectors the think lane switches to a binary-first scan (`vec_quantize_binary`, about 10 times faster) with an int8 rescore of the top 200, or enables the `ann` feature from the memory doc, whichever the bench shows keeps the deadline.

## Consumption

Retrieved context enters the pack of `reuse/context.md` as parts. Two new kinds: `evidence` and `coverage`. Order in the prefix: `system`, `profile`, `tool_defs`, `memory`, `snapshot`, `summary`, `evidence`, `working_set`, `coverage`, `turn`. `coverage` is volatile and sits last before the turn so a changed source state never breaks the cached prefix above it.

An evidence part:

```json
{"kind": "evidence", "cite": "c7", "realm": "work", "source": "slack:T024BE7LD",
 "kind_of": "message", "pointer": "https://testbox.slack.com/archives/C024BE91L/p1759900000000100",
 "title": "#eng-monarch", "author": "fp:3a9f...", "event_ms": 1759900000000, "age": "3h",
 "tokens_est": 84, "text": "[#eng-monarch, 2026-10-08 14:20] the probe test flakes on retry ..."}
```

Ordering is cache-stable. The budget cut uses scores. After the cut, parts are sorted by `(realm, source, event_ms, position)`, so two consecutive packs that retrieved the same evidence are byte-identical in that region (CTX-02). New evidence changes the suffix only when it sorts later.

Raw versus summary. Raw redacted chunk text is the default. A summary replaces raw text only for documents over 1,500 tokens, produced by the local model from a structured verdict `{"summary": "...", "covers": [start, end]}` parsed fail-closed, cached by `(content_hash, model)`. A malformed verdict leaves the raw head in place. The talk lane never waits for a summary.

What the model sees so it can cite. The system part states the rule once: "Evidence parts carry `[c<n>]` ids. Cite with the id. Say when the coverage part lists a source as unavailable or stale. Never claim a source you were not given." The model writes `[c7]` in its text. The daemon maps each marker to the evidence pointer, renders a citation chip on the card that opens the source, drops unknown markers and counts them in the ledger row. The shape follows Anthropic's citations API (a document index plus a span), which mote cannot use through `claude -p`, so the marker protocol stands in.

"What did you know" (CTX-10) lists evidence parts with kind, source, realm, age and tokens, and the coverage part as it was at call time.

## Coverage map

This is the "what I do not know" layer and the owner's highest priority here. It is a store, a per-turn report, three renderings and a rule.

The store, in `mote.sqlite` beside the registry:

```sql
CREATE TABLE capability (
  id TEXT PRIMARY KEY,            -- 'files', 'spotlight', 'apple_mail', 'apple_notes', 'apple_calendar', 'messages', 'browser:chrome', 'browser_session', 'overseer'
  lender_client TEXT,             -- the Mac client that lends it, NULL when none is connected
  state TEXT NOT NULL,            -- 'connected' | 'client_offline' | 'denied' | 'disabled'
  since_ms INTEGER NOT NULL
);

CREATE TABLE query_log (
  id INTEGER PRIMARY KEY,
  ts_ms INTEGER NOT NULL,
  lane TEXT NOT NULL,
  thread_id TEXT,
  realm_scope TEXT NOT NULL,
  query_hash TEXT NOT NULL,       -- sha256 of the redacted query
  sources_consulted TEXT NOT NULL,-- JSON: per source, hits and top score
  verdict TEXT NOT NULL,          -- 'strong' | 'weak' | 'none'
  unavailable TEXT NOT NULL,      -- JSON: sources in scope that were not searchable, with state and fix
  latency_ms INTEGER NOT NULL,
  partial INTEGER NOT NULL,
  pack_id TEXT
);
```

Connection and capability states, with the fix the card shows:

| State | Meaning | Fix shown |
| --- | --- | --- |
| `connected` | Last sync succeeded within the expected interval | none |
| `syncing` | Backfill or a full listing is running, with the covered window so far | none |
| `stale` | `now - covered_to` is more than 3 times the expected interval | "Slack is 3 hours stale. Last sync failed: rate limited." |
| `needs_reauth` | The provider rejected the credential (`invalid_grant`, `invalid_auth`, `gh auth status` non-zero) | "Sign in to GitHub again" |
| `missing_scope` | A scope mote needs is not granted | "Slack search needs a user token with search:read" |
| `denied` | macOS TCC refused a lent store | "Grant Full Disk Access to mote in System Settings" |
| `disabled` | The owner turned it off | "Turn on Messages in Connections" |
| `never_synced` | Connected, no data yet | "First sync pending" |
| `client_offline` | A lent capability with no Mac client connected | "Reachable when the Mac client is connected" |
| `rate_limited` | Waiting for a budget window | "GitHub resumes at 14:35" |
| `error` | Anything else, with the last error | the error |
| `removed` | The owner removed the connection | none |

Expected intervals are the freshness targets in the connectors table. Freshness is `now - covered_to_ms` per stream, rolled up to the connection as the oldest stream the realm scope needs.

The per-turn report. Before a pack is built, the planner computes a `CoverageReport` in code:

```text
CoverageReport {
  realm_scope, computed_ms,
  sources: [ { connection, realm, state, fresh_as_of, covered_from, covered_to, searched: bool, hits, top_score, fix } ],
  capabilities: [ { id, state } ],
  verdict: strong | weak | none,
  partial: bool
}
```

It is stored in `query_log`, attached to the pack as the `coverage` part, and kept with the ledger row.

Three renderings:

1. The `coverage` pack part. For talk, at most 60 tokens: only sources in scope that are unavailable or stale, and the verdict. For think, at most 300 tokens: the full list with windows. The model is told these facts. It is not asked to compute them.
2. The sources strip on every card, rendered by the daemon from the report, not from the model's text. Each source shows state, freshness and hits. An unavailable source shows its fix as an action.
3. The Connections screen in mote-ui: connections grouped by realm, each with identity, state, freshness, covered window, scopes and a fix action. Lent capabilities show "reachable while the Mac client is connected" and flip to `client_offline` when it disconnects. A "What I could not find" list shows `query_log` rows with verdict `none`, their realm and the sources consulted.

The rule. When the verdict is `none`, or when a source in scope is not `connected`, the daemon attaches the coverage line to the card whatever the model wrote. The think lane's `coverage` tool returns the same report on demand, and the `context_search` tool returns per-source hit counts with every result, so a tool-using turn sees the gaps as data.

Why code and not the model. The Sufficient Context study found that large models answer rather than abstain when context is insufficient (Gemini 1.5 Pro hallucinated on 40.4% of insufficient cases), that an "I don't know" instruction was already in the prompt, and that fine-tuning on "I don't know" made Mistral 7B worse. AbstentionBench found a careful system prompt raises abstention without fixing the cause. Enterprise search products hide what the user cannot access and report nothing. mote reports the gap as data and renders it itself, and the model's job is to phrase it.

What a model turn does with it. Talk: the local model sees the 60-token part and can say "I cannot see your calendar, it is not connected" in its own words, and the card shows the strip anyway. Think: Claude sees the 300-token part, can call `coverage` for detail, and can call `context_search` with a different realm or source when the owner allows. Router: nothing. The router does not see the index.

The reflect job may read `query_log` rows with verdict `none` and propose a connection as a card. It never connects one.

## Storage engine decision

The owner's question deserves a plain answer. mote's data model is a graph with time. The engine does not have to be a graph database, because at mote's scale the query shape decides latency, not the engine.

Data model, independent of the engine: nodes (entities, documents, chunks, summaries, procedures) with realm and time, typed edges with four timestamps (MEM-03), document links, full text over chunks, vectors over chunks, and a cursor per stream. Any engine below must hold all of it or pair with another store.

Comparison. Embed means the engine links into the Rust daemon with no server. Multi-hop numbers are the author's measurements on the owner's Mac (research 1.1) and vendor numbers where marked. Binary size is the delta a Rust daemon pays.

| Engine | Embeds in a Rust daemon, aarch64 Linux included | Write path cost | 2-hop and 3-hop at 100k and 1M edges | Vectors | Full text | Transactions | Binary size | Maturity | License | Last release |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| SQLite graph tables plus petgraph walks (memory doc) | Yes. rusqlite `bundled`, pure C, builds everywhere mote builds. | One insert per edge in a batch transaction. 1M random edges plus two indexes built in 0.76 s. | Measured: 2-hop 0.05 ms, 3-hop 0.4 to 0.5 ms at both sizes for ordinary nodes. Hub with 4,000 edges: 2-hop 28 ms, 3-hop 310 ms unbounded, 0.03 ms bounded to 200 edges. | sqlite-vec 0.1.9, brute force, int8 and binary, partition keys | FTS5, BM25, porter, trigram | ACID, WAL, one writer per file | under 900 KiB for SQLite, small for the extension | SQLite is the most deployed database. sqlite-vec is pre-v1. | Public domain, MIT or Apache-2.0 | SQLite 3.53 bundled, sqlite-vec 2026-03-31 |
| LadybugDB (Kuzu successor), crate `lbug` | C++ core. `build.rs` downloads a prebuilt static library per platform (linux-aarch64 8.8 MB compressed) or runs CMake. docs.rs cannot build it. Cross-building with `cargo zigbuild` is unverified. | Not measured. Columnar CSR storage is built for bulk loads. One writer at a time. | Not measured on this graph. The Kuzu report shows wins on LDBC SF100 (hundreds of millions of edges), 2-hop from 0.33 s. The NeuG post shows losses on SF1. Neither is mote's scale. | On-disk HNSW extension, cosine, filtered search through projected graphs. Incremental maintenance after `CREATE_VECTOR_INDEX` is undocumented. | BM25 extension on node string properties. Incremental update undocumented. | Serializable, one writer, no multi-process story documented | about 9 MB static library plus libstdc++ | Kuzu archived 2025-10-10. Ladybug forked, 30 crate releases, 1.8k stars. | MIT | 0.21.2, 2026-10-01 |
| SurrealDB embedded (`kv-surrealkv`) | Yes, pure Rust with SurrealKV. Multi-minute builds. | Vendor: creates 34,830 per second on SurrealKV in 3.0 (8.25 ms mean latency per create in their harness) | Vendor: depth-2 3.29 ms, depth-3 8.30 ms on SurrealKV on a Threadripper. About 60 times the SQLite join measurement. | HNSW listed on the roadmap for 3.x. Embedded support unverified. | Full-text indexes listed. Embedded support unverified. | ACID, MVCC | Unknown for embedded. The full server package is 76 MB installed. | 3.3.2, active | Business Source License 1.1, Apache-2.0 on 2030-01-01 | 2026-10-08 |
| HelixDB | The docs describe `Client::open` embedded mode, but the published `helix-db` 3.0.0 crate has zero feature flags and no `embedded` feature. | Unknown | Unknown. No benchmark published. | ANN prefiltered by traversal (docs) | BM25 (docs) | Unknown for embedded | Unknown | 3.5.1 server releases on 2026-10-08. Embedded not on crates.io. | Apache-2.0 | crate 3.0.0, 2026-08-04 |
| oxigraph | Yes, with RocksDB (C++) by default. No pure-Rust persistent backend listed. | RocksDB writes | SPARQL property paths. Not measured. | none | none | ACID through RocksDB | RocksDB build, tens of MB | 0.5.11, active | MIT or Apache-2.0 | 2026-09-02 |
| cozo | Yes, pure Rust with the SQLite or memory backend | Datalog transactions | Datalog recursion with built-in PageRank. Not measured. | HNSW since 0.6 | since 0.7 | ACID | moderate | No release since 2023-12-11. 43 open issues. Unmaintained. | MPL-2.0 | 2023-12-11 |
| Hybrid: SQLite as source of truth, whole graph resident in petgraph | Yes | Same as SQLite plus one in-memory edge insert | Walks run in memory with no SQL per hop. A 1M-edge graph is about 16 to 40 MB resident and rebuilds from SQLite at start in about one second (estimate, measured in the spike). | as SQLite | as SQLite | SQLite's | as SQLite | petgraph 0.8.3 | MIT or Apache-2.0 | as SQLite |

Recommendation. SQLite with FTS5 and sqlite-vec as the source of truth, graph tables in the memory doc's shape with a `realm` column, and bounded petgraph walks over a loaded neighbourhood, which is the memory doc's recommendation extended to documents. Reasons, in order:

1. Measured latency is already two orders of magnitude inside the think budget at 1M edges when both edge columns are indexed and the walk is bounded. The slow cases in the research were query-shape mistakes (a materialized union view, an unbounded hub walk), and the design above forbids both.
2. One file format, one build, one backup, one `sqlite3` CLI for inspection, and the same engine the event log and ledger already use.
3. LadybugDB would add a C++ static library, a CMake fallback, an undocumented incremental-index story, and a cross-build risk for the future small server, in exchange for wins that appear at a scale three orders of magnitude above mote's.
4. SurrealDB's traversal numbers are slower than SQLite's on this shape and its license is not open source until 2030. HelixDB's embedded mode is not published. oxigraph has no vectors or text. cozo is unmaintained.

The graph store sits behind one `GraphStore` trait in `mote-index` (`neighbourhood(seeds, hops, max_edges)`, `insert_edge`, `expire_edge`, `node_by_key`) with the SQLite implementation as default and a feature-gated `lbug` implementation for the spike, both run against the same tests. If walks ever exceed the budget on real data, the first upgrade is the resident-graph hybrid, which changes no schema. The second is LadybugDB, which the spike prices.

Spike plan, before the choice is final:

1. Datasets. The LoCoMo and LongMemEval_S replays from MEM-11 through the real ingest path. A synthetic graph at 100k and 1M edges with a power-law degree distribution and ten hubs of 4,000 edges. The owner's real index after 30 days of ingest, measured in place.
2. Engines. The SQLite implementation. The `lbug` 0.21.2 implementation behind a feature, with the HNSW and FTS extensions loaded. The resident-graph hybrid as a third row.
3. Measures. Edge ingest throughput. 2-hop and 3-hop neighbourhood p50 and p95 from random seeds and from hub seeds with the 200-edge bound. PPR over the bounded neighbourhood. KNN at 100k and 500k int8 vectors. BM25 top 40. Recall@5, recall@10 and MRR on the replays for `fts`, `vec`, `fused` and `fused+graph`. Clean build time on the Mac. Binary size delta. RSS at idle and after a 10k-query run. A `cargo zigbuild --target aarch64-unknown-linux-gnu.2.36` build of the `lbug` feature.
4. Gate for switching engines. The think-lane retrieval p95 on the real index improves by at least 2 times, recall@10 does not fall, the binary grows by at most 15 MB, the aarch64 build works, and incremental HNSW and FTS updates are shown to work without a rebuild. Any failed condition keeps SQLite.
5. Record. `docs/verification/CTXL-48.md` with the table, the commit, the machine and the date. The README board flips the engine open decision only after that record exists.

## Efficiency targets

Targets are for the owner's Mac at 100,000 documents and 500,000 chunks, which is about one year of the owner's Slack, mail, files, calendar, Linear, Notion, browser titles and transcripts. Every target is measured by `moted bench context <sub>` and the output goes to the evidence record.

| Measure | Target | How |
| --- | --- | --- |
| Ingest throughput, steady state, with embedding | at least 200 documents per second and 1,000 chunks per second | `bench context ingest` over a 10k-document fixture |
| Embedding throughput | at least 4,000 chunks per second on one core | `bench context embed` |
| Model calls per ingested document | 0 | ledger count during the bench |
| Index size per 10,000 documents at 2 KB average text | at most 80 MB | `page_count * page_size` of `index.sqlite` after `VACUUM`. Estimate: 20 MB text, 9 MB FTS at 45%, 15 MB for 60k int8 vectors, 15 MB metadata and indexes, about 60 MB. |
| Talk retrieval at 500k chunks | p50 at most 40 ms, p95 at most 80 ms | `bench context retrieve --lane talk` |
| Think retrieval at 500k chunks, with the walk | p95 at most 250 ms | `bench context retrieve --lane think` |
| Hub seed, think lane | no deadline miss with a 4,000-edge hub | `bench context walk --hub` |
| Hot-partition vector scan, 100k int8 vectors | p50 at most 15 ms | `bench context vec --tier hot` |
| Full vector scan, 500k int8 vectors | p50 at most 120 ms, else binary-first or `ann` | `bench context vec --tier all` |
| Coverage report | at most 5 ms | `bench context coverage` |
| Query log write | at most 1 ms, inside the call's transaction | same |
| Inbox push latency, Slack and FSEvents | p95 at most 5 s from source event to inbox row | live, in evidence |
| Backfill, 100,000 documents from local sources | at most 30 minutes | `bench context backfill` |
| Idle CPU of the context layer | at most 0.2 percentage points of the ALWAYS-11 0.5% budget over 10 minutes with connectors on | `ps` samples |
| Ingest yield | pauses within 50 ms of a talk turn start | scripted turn during the bench |
| Resident memory added by `mote-index` and `mote-connectors` | at most 150 MB with `cache_size` 64 MB on the index connection | RSS delta |
| Index open at start | at most 200 ms, no full load | timing log |
| Event-log append during ingest | p99 at most 5 ms | concurrent bench with LOG-09 |

The bench also prints recall@5, recall@10 and MRR on the replays, the share of packs marked `partial`, and the verdict AUROC on LongMemEval's answerable versus abstention questions.

## Privacy and limits

What never leaves the machine: the index, the inbox, the registry, the coverage store, the query log, every token and app password, every snapshot body. What leaves: redacted pack parts inside a `claude -p` call under the owner's subscription, and nothing at all for Ollama calls. Connector traffic goes only to the connector's own host. A test with networking disabled passes, and the wiremock tests assert no other host is contacted (TEST-10).

Redaction before index. Every chunk passes `mote-redact` before it is hashed, stored, indexed in FTS5 or embedded. The owner can add patterns in a `redact.toml`. A planted token in any source never appears in `index.sqlite`, a pack, a ledger row or a card (CTX-12 extended).

Never indexed: files matching the secret list (`.env*`, `*.pem`, `*.key`, `id_*`, `*.p12`, `credentials.json`, `.netrc`, `.npmrc`, `hosts.yml`, `.credentials.json`, keychain files, token directories), binary and media files, private browsing (which the browsers do not record anyway), Messages unless switched on, locked Notes, and tool outputs inside agent transcripts.

Opt-out. Per connection, per stream (a channel, a mail folder, a calendar, a path glob, a domain), per kind, and per realm pause. A later opt-out deletes what was already indexed from that scope. Rewind's app and site exclusions are the model.

Deletion propagation. A source tombstone removes the document's chunks, FTS rows, vectors, links and mentions within one ingest cycle. Memory nodes derived from a deleted document get `source_deleted_ms` and drop out of packs but stay queryable by sequence (MEM-12). Pack parts keep only hashes and pointers, as CTX-01 already requires. A "forget this" action on any evidence chip does the same with an event. Removing a connection deletes its data and its keychain item. Retention: inbox rows 30 days, query log 90 days, browser visits 180 days, snapshots 7 days, everything else until the source deletes it or the owner does.

Tokens. Every token and app password lives in the macOS Keychain (`keyring` with `apple-native-keyring-store`) or the Linux Secret Service, under an item named by `connection.token_ref`, created for mote's binary only. The registry stores the item name. `gh` and `claude` logins are never read. Note from the research: Claude Code's own keychain item is readable by any process that runs `/usr/bin/security`, which is Anthropic's choice and not mote's concern, but mote must not repeat it.

Lent capabilities. Each is a switch in the Mac client: files (on by default for the owner's project roots), Spotlight discovery (on), Apple Mail store (off until Full Disk Access is granted and the owner turns it on), Notes (same), Calendar store (same), Messages (off), browser profiles (on for Chrome and Brave, Safari needs Full Disk Access), signed-in browser session for the web connector (off, with a per-domain allowlist, read-only navigation, no form submission), Overseer (on when `overseerd` runs). The coverage map shows each switch's state.

Web. The web connector fetches explicit URLs only, honours `robots.txt`, sends one request per second per host, never submits forms, and stores extracted text with a TTL.

Limits of this design. Deletions in Linear, Notion and polled sources are found only by the daily full listing. Gmail push and all webhooks wait for the relay. Apple's store formats are undocumented and can change with a macOS release, so each store reader has a version check and reports `error` with the version instead of guessing. The cross-encoder reranker is off until measured. Nothing here indexes screen content or audio.

## Open decisions with defaults

1. Files. Default: a second file `index.sqlite` with its own writer, beside `mote.sqlite`. One file only if the concurrent bench shows event-log appends unaffected by ingest.
2. Talk evidence budget. Default: 600 tokens taken from the talk pack's `snapshot` and `memory` allocations in `reuse/context.md`, exact split set under CTX-00.
3. Embedding. Default: `potion-base-8M` int8 for everything, as MEM-10. A second `strong` column through Ollama `/api/embed` only if recall@10 trails the bench by more than five points and the owner approves the model pull. No embedding model is installed today.
4. Reranker. Default: off. The `rerank` feature with `mxbai-rerank-xsmall-v1` through fastembed when the think lane has 60 ms to spare and the bench shows a recall gain.
5. Mail path. Default: IMAP with an app password or XOAUTH2 for every account. The Gmail API only for a Workspace account on an Internal OAuth client, because a personal gmail.com client in Testing expires tokens after 7 days and restricted scopes need verification.
6. Calendar path. Default: CalDAV for iCloud and Fastmail, the Google Calendar API for the Workspace account, the Apple store as a lent fallback.
7. Messages. Default: off.
8. Realms. Default: `personal`, `work`, and `shared` for profile facts. Client realms when the owner adds them. Files map by path rules the owner edits.
9. Walk bounds. Default: 2 hops, 200 edges, 20 edges per node.
10. Staleness. Default: 3 times the expected interval. Push sources expect a heartbeat every 60 s.
11. Priority to wake. Default: 3 and 4 wake, 2 digests, 0 and 1 index only, 10 wakes per minute per realm.
12. Backfill depth. Default: 7, 30, 90 days, then everything, only when idle.
13. Transcripts. Default: on for `~/.claude/projects` and `~/.codex`, user and assistant text only.
14. Spotlight. Default: discovery only, never the index.
15. Storage engine. Default: SQLite. The spike gate in the storage section decides any change.
16. Chunking. Default: 400 tokens recursive, no overlap, with the situating header. Late chunking or an LLM situating prompt only if the bench shows more than three points of recall@10.
17. Browser session lending. Default: off, allowlist empty.
18. Microsoft 365. Default: later, behind its own criterion.

## Acceptance criteria

Each criterion is one sentence plus a Verify clause. The checkboxes belong in the root README once the owner approves CTXL-00. Coverage-map criteria come first, realm criteria right after.

### Approval

- **CTXL-00** The owner approves this RFC, the four-part topology as applied here, the realm model, the SQLite default and the spike gate. **Verify:** the owner's note in the README decisions list.

### Coverage map

- **CTXL-01** The registry holds one row per connection and one per lent capability with a state in `connected`, `syncing`, `stale`, `needs_reauth`, `missing_scope`, `denied`, `disabled`, `never_synced`, `client_offline`, `rate_limited`, `error`, `removed`, and every state change writes an event with the reason. **Verify:** `cargo test -p mote-connectors coverage::states` and a `SELECT` after a revoked Slack token shows `needs_reauth` and the event.
- **CTXL-02** Every retrieval builds a coverage report in code that lists, per realm and source, state, freshness, covered window, whether it was searched, hits and a verdict in `strong`, `weak`, `none`, and stores it in `query_log` and against the pack. **Verify:** `cargo test -p mote-index coverage::report` and a join of `ledger`, `context_part` and `query_log` after a live think turn.
- **CTXL-03** A source in the thread's realm scope that is not `connected` appears in the card's sources strip with its state and exact fix, rendered by the daemon regardless of the model's text. **Verify:** scripted model returns a confident answer about the calendar with no calendar connection, and the card still shows "Calendar: not connected" with the connect action.
- **CTXL-04** Freshness is `now - covered_to` per stream, a stream past three expected intervals is `stale`, and the card says the age in words. **Verify:** paused-time test advances 3 h with the Slack stream paused and asserts the strip reads "Slack is 3 hours stale".
- **CTXL-05** The `coverage` pack part sits last before the turn, is at most 60 tokens for talk and 300 for think, and lists only unavailable, stale and empty sources plus the verdict. **Verify:** pack inspection in `cargo test -p mote-context coverage_part` and a CTX-02 prefix diff across two turns with a changed source state.
- **CTXL-06** `moted mcp` serves `coverage` and `context_search`, every call writes a ledger row and an event, and `context_search` returns per-source hit counts with every result. **Verify:** `cargo test -p moted tools::context` with an in-process rmcp client plus a live think turn that calls both.
- **CTXL-07** Queries with verdict `none` are recorded with realm scope and sources consulted, the window lists them under "What I could not find", and the reflect job can propose a connection as a card but never create one. **Verify:** `SELECT` on `query_log`, a screenshot, and a test that the reflect job's output has no registry write.
- **CTXL-08** The code-computed verdict separates LongMemEval's answerable questions from its abstention questions with AUROC at least 0.75, with no model call. **Verify:** `moted bench context coverage --set longmemeval-s` output in `docs/verification/CTXL-08.md`.
- **CTXL-09** The Connections screen groups connections by realm with identity, state, freshness, covered window, scopes and a fix action, and a lent capability reads "reachable while the Mac client is connected" and flips to `client_offline` within 5 s of the client disconnecting. **Verify:** screenshots with the Mac client connected and after it quits.

### Realms and the registry

- **CTXL-10** Every connection row carries provider, kind, realm, identity id, display, fingerprint, tenant, scopes granted, scopes needed, token reference, follows-app, lender and health, and never the token itself. **Verify:** schema test plus a `grep` of both database files for a planted token value after connecting a wiremock Slack.
- **CTXL-11** At connect time mote proposes a realm from the `hd` claim, the Microsoft `tid`, the Slack `team_id` and `enterprise_id`, GitHub org memberships or the mail domain, and no inbox row exists before the owner confirms. **Verify:** wiremock identity responses for a Workspace account and a gmail.com account yield `work` and `personal` proposals, and `inbox` is empty until the confirm action.
- **CTXL-12** Every doc, chunk, vector row, inbox row, stream, memory node and memory edge carries a realm, and the vec0 partition key is the realm. **Verify:** `PRAGMA table_info` tests and an insert without a realm fails.
- **CTXL-13** A thread's pack never contains evidence, memory nodes or coverage lines from outside its realm scope unless the owner set the scope to `all`, which is an event and a badge. **Verify:** plant a unique string in a work-only document, ask from a personal thread with a scripted model, assert the pack and card never contain it, set `all`, assert it appears with the realm badge.
- **CTXL-14** Outward actions default to the thread's realm and connection, the approval card names the realm and the acting identity, and a target in another realm needs an explicit owner choice. **Verify:** approval row and card for a Slack post from a work thread, and a test that a watcher-initiated action cannot select a cross-realm target.
- **CTXL-15** Removing a connection deletes its docs, chunks, vectors, inbox rows and keychain item, keeps a `removed` registry row with counts, and leaves other realms unchanged. **Verify:** counts before and after, a failing keychain lookup, and an unchanged `SELECT` on another realm.
- **CTXL-16** The same person in two realms is two entity nodes until the owner merges them, and the merge is a reversible event. **Verify:** `cargo test -p mote-index entity::merge` and the MEM-09 revert path.
- **CTXL-17** A login mote does not own (`gh`, `claude`) is registered as `follows_app` with a fingerprint only, is never logged out by mote, and the registry shows when it changes underneath. **Verify:** switch `gh auth` to another account and assert the registry row updates its fingerprint and emits an event.

### Event inbox

- **CTXL-18** Every connector emits inbox rows of the one schema and the id is the connection id plus the source event id, so a redelivered event changes only `observed_ms`. **Verify:** schema test and a Slack envelope replayed twice yields one row.
- **CTXL-19** Push sources deliver an inbox row within 5 s at p95 of the source event, and every push source has a reconciliation poll that finds a change made while the push link was down. **Verify:** fixture tests that cut the socket, IDLE and FSEvents links, plus a live Slack mention timing in evidence.
- **CTXL-20** Priority 3 and 4 rows wake the router as `utterance` with source `watcher:<connection>` within 1 s, priority 2 rows batch into one digest card per hour per realm, lower rows only index, and wakes are capped at 10 per minute per realm with overflow in the digest. **Verify:** paused-time test with 50 rows of mixed priority.
- **CTXL-21** Edits and deletions (`message_changed`, `message_deleted`, IMAP `VANISHED` or a missing UID, FSEvents removal, a cancelled calendar event, a 404 on refetch) become `updated` or `tombstone` rows, and the index reflects them within one ingest cycle. **Verify:** one fixture per source and a `SELECT` showing the chunk, FTS and vec rows gone.
- **CTXL-22** A stream's cursor advances only in the transaction that commits the last index write for its batch, and a kill between the two re-ingests without duplicates. **Verify:** `cargo test -p mote-connectors cursor::kill` spawns a child, kills it between write and cursor save, and counts docs after restart.
- **CTXL-23** A full listing per stream runs on its schedule in key order with resumable progress while incremental polling continues, diffs remote ids against local ids, and emits tombstones for ids that vanished. **Verify:** fixture where the remote drops 3 of 100 items.

### Connectors

- **CTXL-24** Slack ingests Socket Mode events and user-token history with cursor pagination at Tier 3, backfills per channel newest first, uses the user token for search and non-member channels, and without the user token shows `missing_scope` naming `search:read`. **Verify:** wiremock asserts at most 50 calls per minute per method and the registry row lists the missing scope.
- **CTXL-25** GitHub polls notifications through `gh api` with `If-Modified-Since` and `X-Poll-Interval`, lists issues and PRs with `since` and `sort=updated` through `gh api --paginate`, indexes code from local clones through the file connector, and never reads a token from `gh`. **Verify:** a fake `gh` on PATH records calls and headers and the test asserts no `auth token` call.
- **CTXL-26** IMAP re-issues IDLE under 29 minutes, treats a `UIDVALIDITY` change as a full resync, uses QRESYNC when advertised, parses bodies with mail-parser, reads the credential from the keychain, and never touches the Gmail API unless enabled. **Verify:** scripted IMAP fixture plus one live IDLE session in evidence.
- **CTXL-27** CalDAV uses `sync-collection` with the stored token, resyncs fully on the precondition error, falls back to `getctag` where RFC 6578 is absent, the Google Calendar option uses `syncToken` and handles 410, and events carry start, end, attendees and status. **Verify:** fixture server for both paths and a live iCloud sync in evidence.
- **CTXL-28** The file connector uses FSEvents through notify with the stored event id and volume UUID, replays since the id on start, rescans on `MustScanSubDirs` or a UUID change, honours gitignore and mote's ignore list, uses Spotlight only for discovery with `-onlyin`, and a 300,000-file tree yields under 5,000 documents with default ignores. **Verify:** fixture tree counts and the owner's `~/projects` count in evidence.
- **CTXL-29** Apple Mail, Notes, Calendar and Messages are read by the Mac client from a copy of each store opened read-only, each is its own connection with its own switch (Messages off by default), a TCC denial shows `denied` with the Full Disk Access fix, and a store schema the reader does not recognize shows `error` with the macOS version. **Verify:** runs without and with Full Disk Access in evidence, and a fixture with an unknown schema version.
- **CTXL-30** Chrome, Brave and Safari history and bookmarks are read from a copy per profile, each profile is a connection with a realm, visit times convert from the WebKit and Apple epochs, and no page content is fetched. **Verify:** fixture `History` databases with known epochs.
- **CTXL-31** Linear and Notion poll with `updatedAt` and `last_edited_time` cursors under 2,500 requests per hour and 180 per minute, index only pages shared with the integration, and have webhooks disabled until a relay exists. **Verify:** wiremock counts and a registry row with `webhooks: disabled`.
- **CTXL-32** The web connector fetches only explicit URLs, honours `robots.txt`, sends at most one request per second per host, extracts main text with dom_smoothie, stores snapshots with `fetched_ms` and TTL, and uses a lent browser session only for allowlisted domains with the switch on. **Verify:** wiremock rate and robots tests plus an allowlist test.
- **CTXL-33** Agent transcripts from Claude Code, Codex and Overseer index as documents with user and assistant text only, tool outputs excluded, under the project path's realm. **Verify:** fixture JSONL and a `SELECT` that finds no tool output text.

### Index

- **CTXL-34** The index lives in `index.sqlite` with `doc`, `chunk`, `chunk_fts`, `chunk_vec`, `link`, `mention`, `inbox` and `stream`, has its own writer, and an ingest transaction never delays an event-log append beyond 5 ms at p99. **Verify:** schema test and `moted bench context concurrent` with LOG-09 running.
- **CTXL-35** Ingestion normalizes, redacts, excludes, dedupes by content hash and simhash, chunks at 400 tokens with a situating header, embeds with model2vec int8, writes in batches, links entities by canonical key, and makes no model call. **Verify:** `cargo test -p mote-index pipeline::` and a ledger count of 0 model rows during the ingest bench.
- **CTXL-36** Re-ingesting an unchanged document is a no-op, a changed document replaces its chunks and vectors in one transaction, and a near-duplicate links `duplicate_of` and is not indexed twice. **Verify:** unit tests with equal, changed and near-duplicate inputs.
- **CTXL-37** Every doc carries `event_ms`, `updated_ms` and `observed_ms`, and the planner filters by source time window. **Verify:** `cargo test -p mote-index time::`.
- **CTXL-38** Ingest pauses within 50 ms of a talk turn start and resumes after it, and backfill never uses more than one core. **Verify:** bench with a scripted turn and CPU samples.

### Retrieval

- **CTXL-39** Talk retrieval fuses FTS5 and vec0 over the hot partition with RRF k 60, recency decay per kind, the working-set boost and dedupe, and returns at most 8 items under 600 tokens within 40 ms p50 and 80 ms p95 at 500,000 chunks. **Verify:** `moted bench context retrieve --lane talk` in evidence.
- **CTXL-40** Think retrieval adds entity seeds, a walk bounded to 2 hops, 200 edges and 20 edges per node, PPR, both vector tiers and explicit filters, returns at most 20 items under 4,000 tokens within 250 ms p95, and marks a deadline miss `partial`. **Verify:** `moted bench context retrieve --lane think` plus a test with an injected slow walk.
- **CTXL-41** A hub node with 4,000 edges does not push think retrieval past its deadline. **Verify:** `moted bench context walk --hub` in evidence.
- **CTXL-42** `moted bench context membench` replays LoCoMo and LongMemEval_S through the real ingest path and prints recall@5, recall@10, MRR, latency and tokens for `fts`, `vec`, `fused` and `fused+graph`, and `fused` beats both single lanes on recall@10. **Verify:** the table in `docs/verification/CTXL-42.md`.
- **CTXL-43** At or above 250,000 vectors the think lane uses binary-first rescoring or the `ann` feature and keeps its deadline. **Verify:** bench at 500,000 vectors.
- **CTXL-44** The planner never classifies natural-language query text with regexes, and source and time filters come only from structured tokens. **Verify:** a code review item recorded in the evidence file and a test that free text with the word "slack" applies no source filter.

### Consumption

- **CTXL-45** Evidence parts carry cite id, realm, source, kind, pointer, title, author fingerprint, time, age, tokens and redacted text, and are ordered by `(realm, source, event_ms, position)` after the budget cut. **Verify:** a CTX-02 style diff of two consecutive packs shows one changed suffix.
- **CTXL-46** The model cites with `[c<n>]` markers, the daemon maps each marker to its pointer on the card, drops unknown markers and counts them in the ledger row, and each citation chip opens the source. **Verify:** scripted model output with one bad marker, a `SELECT` of the count, and a screenshot.
- **CTXL-47** Raw redacted text is the default, a summary replaces it only for documents over 1,500 tokens from a structured verdict parsed fail-closed and cached by hash and model, and a malformed verdict leaves the raw head. **Verify:** unit tests with empty, truncated and prose verdicts.
### Storage

- **CTXL-48** The spike measures the SQLite store and the `lbug` 0.21.2 store on the replays and the synthetic hub graphs against the gate in the storage section, and the result is recorded before any engine change. **Verify:** `docs/verification/CTXL-48.md` with the table, the commit, the machine and the date.
- **CTXL-49** Graph storage sits behind a `GraphStore` trait with the SQLite implementation as default, the walk bounded as above, and the `lbug` implementation behind a feature that is off by default. **Verify:** the trait test suite passes for the SQLite implementation and `cargo build` without the feature links no C++ library.
- **CTXL-50** Every crate used by `mote-index` and `mote-connectors` is pinned to an exact version or a git tag, and `cargo tree` shows no `lbug` or `surrealdb` in the default build. **Verify:** `grep -E '"=|tag =' Cargo.toml` and `cargo tree -p moted | grep -E 'lbug|surrealdb'` prints nothing.

### Efficiency

- **CTXL-51** The index grows by at most 80 MB per 10,000 documents at 2 KB average text, ingest sustains 200 documents per second with embedding, and the two crates add at most 150 MB resident. **Verify:** `moted bench context size`, `ingest` and the RSS delta in evidence.
- **CTXL-52** The coverage report takes at most 5 ms, the query log write at most 1 ms, and the context layer adds at most 0.2 percentage points of idle CPU over 10 minutes with connectors on. **Verify:** bench output and `ps` samples in evidence.

### Privacy and limits

- **CTXL-53** Nothing from the index, inbox, registry, query log or token store leaves the machine except redacted pack parts in a model call, and connector traffic goes only to the connector's own host. **Verify:** `cargo test --workspace` with networking disabled and the TEST-10 host assertion extended to every connector.
- **CTXL-54** A planted secret in any source never appears in `index.sqlite`, `mote.sqlite`, a pack, a ledger row or a card. **Verify:** one fixture per connector with a fake token and a `grep` of both database files.
- **CTXL-55** Opt-outs per connection, stream, path, domain, kind and realm are enforced before indexing, and a later opt-out deletes already-indexed items from that scope. **Verify:** `cargo test -p mote-index exclude::` and counts before and after.
- **CTXL-56** A source tombstone removes chunks, FTS rows, vectors, links and mentions within one ingest cycle, marks derived memory nodes `source_deleted`, "forget this" on an evidence chip does the same with an event, and secret-bearing and binary files are never indexed. **Verify:** tombstone and forget tests, a fixture tree with `.env` and a PNG, and a `SELECT` showing nothing indexed from them.

## Sources

Every source, URL, number and unknown is in [context-layer-research.md](../research/context-layer-research.md). The decisions above rest mainly on:

- The author's measurements on the owner's Mac (research 1.1 to 1.5): SQLite 2-hop and 3-hop at 100k and 1M edges, the hub case, Spotlight timings, TCC denials, store sizes.
- Storage engines: https://crates.io/api/v1/crates/lbug, https://raw.githubusercontent.com/LadybugDB/ladybug-rust/main/build.rs, https://docs.ladybugdb.com/extensions/vector, https://docs.ladybugdb.com/extensions/full-text-search, https://docs.ladybugdb.com/cypher/transaction, https://cs.uwaterloo.ca/~ssalihog/papers/kuzu-tr.pdf, https://github.com/surrealdb/surrealdb/blob/main/LICENSE, https://surrealdb.com/blog/surrealdb-3-x-by-the-numbers, https://dev.to/surrealdb/surrealdb-30-benchmarks-a-new-foundation-for-performance-4ofm, https://docs.rs/crate/helix-db/3.0.0/features, https://docs.rs/crate/oxigraph/latest/features, https://crates.io/api/v1/crates/cozo, https://sqlite.org/about.html, https://alexgarcia.xyz/sqlite-vec/features/vec0.html, https://www.sqlite.org/fts5.html
- Connectors: https://docs.slack.dev/apis/web-api/rate-limits, https://docs.slack.dev/changelog/2025/05/29/rate-limit-changes-for-non-marketplace-apps, https://docs.slack.dev/reference/scopes/search:read, https://docs.github.com/en/rest/using-the-rest-api/rate-limits-for-the-rest-api, https://docs.github.com/en/rest/activity/notifications, https://developers.google.com/workspace/gmail/api/reference/quota, https://developers.google.com/workspace/gmail/api/guides/sync, https://support.google.com/cloud/answer/15549945, https://www.rfc-editor.org/rfc/rfc2177.html, https://www.rfc-editor.org/rfc/rfc7162.html, https://www.rfc-editor.org/rfc/rfc6578.html, https://developers.google.com/workspace/calendar/api/guides/sync, https://linear.app/developers/rate-limiting, https://developers.notion.com/reference/request-limits, https://support.apple.com/en-us/guide/mac-help/mchl211c911f/mac, https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/UsingtheFSEventsFramework/UsingtheFSEventsFramework.html, https://keith.github.io/xcode-man-pages/mdfind.1.html
- Sync designs: https://learn.microsoft.com/en-us/graph/delta-query-overview, https://learn.microsoft.com/en-us/graph/change-notifications-lifecycle-events, https://docs.airbyte.com/platform/understanding-airbyte/airbyte-protocol, https://nango.dev/docs/reference/api/connection/get, https://docs.composio.dev/docs/auth-configuration/connected-accounts, https://help.merge.dev/articles/6074068-linked-account-statuses-and-the-account-details-endpoint, https://debezium.io/blog/2021/10/07/incremental-snapshots/, https://github.com/cloudevents/spec/blob/v1.0.2/cloudevents/spec.md, https://raw.githubusercontent.com/dust-tt/dust/main/connectors/src/connectors/slack/temporal/workflows.ts
- Identity and realms: https://developers.google.com/identity/openid-connect/openid-connect, https://learn.microsoft.com/en-us/entra/identity-platform/id-token-claims-reference, https://docs.slack.dev/reference/methods/auth.test, https://docs.github.com/en/rest/orgs/members, https://support.google.com/chrome/answer/2364824, https://developer.android.com/work/managed-profiles, https://support.apple.com/guide/mac-help/mchl613dc43f/mac, https://learn.microsoft.com/en-us/purview/sensitivity-labels, https://docs.glean.com/security/security-principles, `/Users/bilal/projects/overseer/docs/rfcs/account-governance.md`
- Knowing its limits: https://arxiv.org/abs/2411.06037 (Sufficient Context), https://arxiv.org/abs/2506.09038 (AbstentionBench), https://arxiv.org/abs/2207.05221 (Kadavath et al.), https://arxiv.org/abs/2509.04664 (Kalai et al.), https://arxiv.org/html/2410.13029v1, https://ar5iv.labs.arxiv.org/html/2305.10923 (query performance prediction), https://learn.microsoft.com/en-us/microsoftsearch/semantic-index-for-copilot, https://docs.ollama.com/api/chat
- Retrieval: https://www.anthropic.com/news/contextual-retrieval, https://www.trychroma.com/research/evaluating-chunking, https://www.trychroma.com/research/context-rot, https://arxiv.org/abs/2502.14802 (HippoRAG 2), https://arxiv.org/abs/2410.05779 (LightRAG), https://www.microsoft.com/en-us/research/blog/lazygraphrag-setting-a-new-standard-for-quality-and-cost/, https://www.elastic.co/guide/en/elasticsearch/reference/current/rrf.html, https://www.elastic.co/guide/en/elasticsearch/reference/current/query-dsl-function-score-query.html, https://arxiv.org/abs/2210.11934, https://github.com/MinishLab/model2vec-rs, https://qdrant.tech/blog/oxidizing-cross-encoders/, https://platform.claude.com/docs/en/build-with-claude/citations
- Prior art: https://9to5mac.com/2025/12/05/rewind-limitless-meta-acquisition/, https://github.com/khoj-ai/khoj, https://docs.glean.com/connectors/crawling-refresh-rates, https://developers.llamaindex.ai/python/framework/module_guides/loading/ingestion_pipeline/, https://docs.haystack.deepset.ai/docs/documentjoiner, https://github.com/mediar-ai/screenpipe, https://learn.chatgpt.com/docs/dots, https://support.claude.com/en/articles/12489464-use-enterprise-search, https://blogs.windows.com/windowsexperience/2024/09/27/update-on-recall-security-and-privacy-architecture/
