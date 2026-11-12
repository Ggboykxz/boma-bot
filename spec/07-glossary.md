# 07 — Glossary

Terms as this specification uses them. Where a term collides with common usage, the definition here
wins inside the spec.

| Term | Meaning |
|---|---|
| **Actor** | Whoever a request runs as: a user, a bot, or the system. Carried through the request context and needed by `signedIn`. |
| **Agent home** | Persistent file area belonging to a bot per computer provider (`AgentHome`), reclaimable and recorded on deletion. |
| **Approval** | A card that gates an external effect. Nothing leaves the system until it is answered, and the effect is recorded once. |
| **Artifact** | A file a bot or user produced, stored out of band, versioned as a chain rooted at the first version. |
| **Attempt** | One execution try of a run, with its own fence and outcome. Retries create attempts, not new runs. |
| **Background run** | A run whose trigger must not appear in the chat (`tickets`). Its output goes to the board. |
| **Board** | Feature-flagged ticket board: tickets, comments, and background work attached to them. |
| **Bot** | A persistent AI teammate: one thread, one memory, tools, optional computer, optional secrets, owned by a user inside a space. |
| **Card** | A structured message block asking for input (question, approval, purchase, takeover). A pending card holds no compute but holds the queue. |
| **Client nonce** | Caller-generated id making a mutation idempotent. Unique per scope; replay returns the original result. |
| **Computer** | A sandboxed machine (container, VM, desktop host) a bot drives: screen, mouse, keyboard, shell, browser profile, files. |
| **Computer execution lease** | The right to drive a computer for a run, mirrored from the run lease, so two runs cannot drive one screen. |
| **Connector** | An integration a space installs (a provider + credentials). A bot sees only the installs enabled for it. |
| **Effect key** | Unique id on `ExternalEffect` making a side effect exactly-once per run. |
| **Event** | Append-only row on a thread feeding subscriptions, carrying a per-thread `seq`. Distinct from a message. |
| **Fence** | Monotonically increasing number on a lease. Every write conditional on it; a stale worker loses silently. |
| **Golden example** | A request/response pair (`golden/*.json`) or a UI state/expectation pair (`golden/ui/*.json`) used as a compatibility fixture. |
| **Group thread** | A conversation with several bots and several members; each member has an owner bot. |
| **Helper run** | A delegated, scoped sub-run whose result returns to the parent run and never renders in the chat. |
| **Lease** | Time-bounded ownership of a run (`leaseOwner`, `leaseExpiresAt`). Expiry returns the run to `queued`. |
| **Memory** | Bot-scoped or space-shared documents, append-only revisions, current content as the latest revision. |
| **Message** | A visible chat row with typed blocks, a per-thread `seq`, and a unique `clientNonce`. |
| **MCP server** | An external tool server, remote (HTTP) or local (stdio, allow-listed commands only). |
| **Page** | A bot-authored document compiled by the page runtime, private or link-shared by hashed token. |
| **Plugin** | A distributed bundle (skills, connectors, templates) installed from a repository. Parts that would execute third-party code are refused by policy. |
| **Routine** | A cron-scheduled prompt for a bot, with a timezone, a one-shot `@once`, and failure accounting that pauses it at threshold. |
| **Run** | One attempt to do a unit of work for a bot: status machine, model, tools, approvals, effects, timestamps. |
| **Space** | The tenancy boundary. Every bot, thread, run, artifact and routine belongs to exactly one. |
| **Steering** | A user message delivered into an active run instead of queueing a new turn. Not accepted by non-conversational triggers. |
| **Subagent / helper** | A run spawned by another run with a narrower scope and a structured result. |
| **Takeover** | A user taking control of a bot's computer while it is working; the run moves to `waiting_takeover`. |
| **Thread** | Exactly one of: a bot conversation, a group conversation, or an external platform conversation. Holds messages and events with monotonic `seq`. |
| **Trigger** | Why a run exists: `user`, `routine`, `webhook`, `bot_message`, `tickets`, `created`, `helper`, `retry`. Determines steering and visibility. |
| **Unread** | Messages with `seq > lastSeq` not authored by the reader, excluding hidden background steps. |
| **Working status** | `queued`, `leased`, `running` — shown as activity. |
| **Waiting status** | `waiting_input`, `waiting_takeover` — paused on a person, no compute held. |

## Status vocabulary (exhaustive)

`queued → leased → running → completed | failed | cancelled`, plus
`running ↔ waiting_input | waiting_takeover` and `failed → queued` (retry only). Every other transition
is an error and must be rejected, not ignored, when a client requests it.

## Abbreviations

- **RPC** — the JSON procedure interface at `POST /rpc/<namespace>/<procedure>`.
- **RLS** — Postgres row-level security, the reference's tenancy backstop.
- **SSE** — server-sent events, the push channel for threads.
- **BYOK** — bring your own key: a user's model credentials, stored sealed.
- **ADR** — architecture decision record, under `docs/adr/` in the rebuild.
