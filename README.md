# omnicede

**Omnichannel AI agent and OmniSeed memory Provider powered by embedded memory graphs. One API, every channel, one graph.**

The canonical project and executable name is **Omnicede** (`omnicede`). The
former `omni-cede` GitHub URL redirects here only for migration compatibility.

omnicede extends [cede](https://github.com/MikeSquared-Agency/cede) with an HTTP API, identity resolution, and per-channel session management — all backed by an embedded memory graph (single SQLite file, no external DB). Connect WhatsApp, Telegram, Slack, Discord, or any custom integration — the agent remembers across all of them because every interaction is a node in the same graph.

## Ecosystem

```
cortex-embedded          <-- embedded memory graph engine (upstream)
  |-- cede               <-- forkable starter kit
       |-- omnicede     <-- you are here (omnichannel deployment)
```

## What omnicede Adds

On top of everything in cede (embedded memory graph, hybrid recall, auto-linking, decay, tools, sub-agents, TUI), omnicede adds:

| Layer | What it does |
|-------|-------------|
| **HTTP API** | `POST /v1/message` — send a message from any channel and get a reply |
| **Identity** | Maps `(channel, external_id)` pairs to internal user IDs. Same person on WhatsApp and Telegram = same user |
| **Sessions** | One active session per (user, channel). WhatsApp gets its own conversational flow; Telegram gets another. Semantic recall searches the global graph — cross-channel knowledge |
| **Auth** | `x-api-key` header middleware. Set `API_KEY` env var to enable; omit for dev mode |

## OmniSeed memory Provider

Omnicede is also a language-independent OmniSeed Provider Protocol v1
implementation for the canonical `memory` primitive family. Its Provider ID is
`omnicede`, identifying the supplying Omnicede boundary. SQLite, graph memory,
HNSW, and the agent application are implementation choices beneath that
Provider; Omnicede does not directly realise business Capabilities.

The Provider advertises `organisational_context`, `engineering_history`, and
`retained_company_knowledge`, plus the ordinary `index`, `update`, `remove`,
`search`, and `retrieve` operations. Every process is bound to exactly one
company and one durable SQLite file:

```bash
cargo build --release --bin omniseed-provider-omnicede
# OmniSeed starts the binary and supplies databasePath + companyId during
# provider.initialize; JSON-RPC is written only on stdout and diagnostics on stderr.
```

Creating a process or selecting `omnicede` in Omniform is not evidence that the
Provider is connected or healthy. OmniSeed must apply and observe the declared
memory resource and retain the resulting evidence. The SQLite file must live on
durable storage; an ephemeral serverless filesystem is not a production
deployment target.

## Quick Start

```bash
# Clone
git clone https://github.com/MikeSquared-Agency/omnicede.git
cd omnicede

# Build
cargo build --release

# Start the API server
ANTHROPIC_API_KEY=sk-ant-... omnicede serve
# Custom host/port
omnicede serve --host 127.0.0.1 --port 8080
# With Ollama
omnicede --ollama llama3 serve

# Send a message
curl -X POST http://localhost:3000/v1/message \
  -H "Content-Type: application/json" \
  -d '{"channel": "whatsapp", "external_id": "+447123456789", "text": "Hello!"}'

# Health check
curl http://localhost:3000/v1/health

# List sessions for a user
curl http://localhost:3000/v1/sessions/<user_id>

# Stats
curl http://localhost:3000/v1/stats
```

### With Auth

```bash
# Start with auth enabled
API_KEY=my-secret-key ANTHROPIC_API_KEY=sk-ant-... omnicede serve

# Requests require the header
curl -X POST http://localhost:3000/v1/message \
  -H "Content-Type: application/json" \
  -H "x-api-key: my-secret-key" \
  -d '{"channel": "telegram", "external_id": "12345678", "text": "Hello!"}'
```

## API Reference

### `POST /v1/message`

Send a message from any channel. The server resolves the user's identity, gets or creates a session, runs the agent, and returns the reply.

**Request:**
```json
{
  "channel": "whatsapp",
  "external_id": "+447123456789",
  "text": "What did we discuss yesterday?"
}
```

**Response:**
```json
{
  "reply": "Yesterday we discussed the new API design...",
  "user_id": "a1b2c3d4-...",
  "session_id": "e5f6g7h8-..."
}
```

### `GET /v1/health`

```json
{
  "status": "ok",
  "version": "0.1.0"
}
```

### `GET /v1/sessions/:user_id`

```json
[
  {
    "session_id": "e5f6g7h8-...",
    "channel": "whatsapp",
    "created_at": 1711324800,
    "turn_count": 42,
    "last_active": 1711411200
  }
]
```

### `GET /v1/stats`

```json
{
  "nodes": 1234,
  "edges": 5678,
  "by_kind": {"fact": 200, "soul": 1, "session": 15, "...": "..."},
  "managed_sessions": 15,
  "total_turns": 342
}
```

## How Identity Works

```
WhatsApp +447123456789  -+
                          |-> user_id: a1b2c3d4
Telegram @johndoe       -+    (linked via identity layer)
```

When a message arrives, the identity layer:
1. Looks up `(channel, external_id)` in the `channel_mappings` table
2. If found, returns the existing internal user
3. If not, creates a new user and mapping

You can link multiple channels to one user via the identity API.

## How Sessions Work

Each (user, channel) pair gets its own session. This means:

- **Recency window is channel-scoped** — "stop using big words" on WhatsApp only affects WhatsApp's briefing
- **Semantic recall is global** — facts learned on Telegram are available when the user asks on WhatsApp
- **Sessions persist** — reconnecting to the same channel resumes the same session

## Architecture

```
+---------------------------------------------+
|                 omnicede                    |
+-----------+-----------+---------------------+
|  HTTP API |  Identity |  Session Manager    |
| (axum)    | (channel  | (one per user +     |
|           |  mapping) |  channel pair)      |
+-----------+-----------+---------------------+
|                  cede core                   |
+---------+----------+---------+--------------+
|  recall | briefing |  tools  |    agent     |
| (HNSW + | (scored  | (custom |  (loop +    |
|  graph) |  context)|  + std) |  subagent)  |
+---------+----------+---------+--------------+
|            graph + memory                    |
|       (BFS, scoring, decay)                  |
+---------+------------------------------------+
|  HNSW   |         SQLite                     |
| (2-tier)|  (WAL, bundled rusqlite)           |
+---------+------------------------------------+
|            fastembed                          |
|      (BAAI/bge-small-en-v1.5)                |
+----------------------------------------------+
```

## CLI Commands

omnicede retains all of cede's CLI commands and adds `serve`:

```bash
omnicede serve                    # Start HTTP API server (0.0.0.0:3000)
omnicede serve --port 8080        # Custom port
omnicede chat                     # Interactive CLI chat
omnicede ask "question"           # Single query
omnicede graph explore            # TUI graph explorer
omnicede graph overview           # Graph visualization
omnicede memory stats             # Memory statistics
omnicede memory search "query"    # Semantic search
omnicede soul show                # View identity
omnicede doctor                   # Health check
omnicede consolidate              # Trust propagation
omnicede init                     # Initialize DB + download model
```

## Environment Variables

| Variable | Required | Description |
|----------|----------|-------------|
| `ANTHROPIC_API_KEY` | Yes* | Anthropic API key (*or use `--ollama`) |
| `ANTHROPIC_MODEL` | No | Model override (default: `claude-sonnet-4-20250514`) |
| `API_KEY` | No | If set, requires `x-api-key` header on all requests |
| `RUST_LOG` | No | Tracing filter (default: `omnicede=info,tower_http=info`) |

## Staying Updated

omnicede tracks cede as `upstream`. To pull improvements:

```bash
git fetch upstream
git merge upstream/master
```

## Dependencies

Everything from cede, plus:

| Crate | Purpose |
|-------|---------|
| `axum` 0.8 | HTTP framework |
| `tower-http` 0.6 | CORS + request tracing middleware |
| `tracing` + `tracing-subscriber` | Structured logging |

## Tests

```bash
# Run all 22 tests
cargo test -- --test-threads=1
```

## License

MIT
