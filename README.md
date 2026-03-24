# rustvideoplatform-indexer

Indexes media data from the [rustvideoplatform](https://github.com/panmourovaty/rustvideoplatform) PostgreSQL database into Meilisearch.

## What it does

1. **Initial sync** — reads every row from the `media` table and bulk-indexes it into a Meilisearch `media` index
2. **Live updates** — installs a PostgreSQL `LISTEN/NOTIFY` trigger on the `media` table and applies inserts, updates, and deletes to Meilisearch in real-time

## Configuration

Copy `config.example.json` to `config.json` and fill in the values:

```json
{
    "scylla_nodes": ["scylladb:9042"],
    "scylla_keyspace": "videoplatform",
    "meilisearch_url": "http://meilisearch:7700",
    "meilisearch_key": null,
    "meilisearch_embedder": {
        "name": "default",
        "source": "rest",
        "url": "http://embedllama:8084/v1/embeddings",
        "model": "llama.cpp",
        "document_template": "{{doc.name}} {{doc.description}}",
        "dimensions": 1024,
        "request": {
            "model": "llama.cpp",
            "input": ["{{text}}", "{{..}}"],
            "encoding_format": "float"
        },
        "response": {
            "data": [
                { "embedding": "{{embedding}}" },
                "{{..}}"
            ]
        }
    },
    "batch_size": 1000,
    "redis_url": "redis://dragonfly:6379",
    "cache_interval_secs": 60,
    "poll_interval_secs": 30,
    "site_url": "https://example.com"
}
```

You can also use environment variables instead of `config.json`:

| Variable | Required | Default |
|---|---|---|
| `SCYLLA_NODES` | yes | — |
| `SCYLLA_KEYSPACE` | no | `videoplatform` |
| `MEILISEARCH_URL` | no | `http://localhost:7700` |
| `MEILISEARCH_KEY` | no | none |
| `MEILISEARCH_EMBEDDER` | no | `default` |
| `MEILISEARCH_EMBEDDER_SOURCE` | no | `rest` |
| `MEILISEARCH_EMBEDDER_URL` | depends on source | `http://embedllama:8084/v1/embeddings` |
| `MEILISEARCH_EMBEDDER_API_KEY` | depends on source | none |
| `MEILISEARCH_EMBEDDER_MODEL` | no | `llama.cpp` |
| `MEILISEARCH_EMBEDDER_REVISION` | no | none |
| `MEILISEARCH_EMBEDDER_POOLING` | no | none |
| `MEILISEARCH_EMBEDDER_DOCUMENT_TEMPLATE` | no | `{{doc.name}} {{doc.description}}` |
| `MEILISEARCH_EMBEDDER_DOCUMENT_TEMPLATE_MAX_BYTES` | no | none |
| `MEILISEARCH_EMBEDDER_DIMENSIONS` | no | `1024` |
| `MEILISEARCH_EMBEDDER_REQUEST` | no | `{"model":"llama.cpp","input":["{{text}}","{{..}}"],"encoding_format":"float"}` |
| `MEILISEARCH_EMBEDDER_RESPONSE` | no | `{"data":[{"embedding":"{{embedding}}"},"{{..}}"]}` |
| `MEILISEARCH_EMBEDDER_BINARY_QUANTIZED` | no | none |
| `MEILISEARCH_EMBEDDER_HEADERS` | no | none |
| `BATCH_SIZE` | no | `1000` |
| `REDIS_URL` | yes | — |
| `CACHE_INTERVAL_SECS` | no | `60` |
| `POLL_INTERVAL_SECS` | no | `30` |
| `SITE_URL` | yes | — |

`MEILISEARCH_EMBEDDER_HEADERS` can be stored in config for future use with REST embedders, but the current Meilisearch Rust SDK version used by this indexer does not expose a `headers` field on its embedder settings type, so these headers are not sent yet.

## Building

```sh
cargo build --release
```

## Running

```sh
# with config.json in the current directory
./target/release/rustvideoplatform-indexer

# or with environment variables
DATABASE_URL=postgresql://vids:pw@localhost/vids ./target/release/rustvideoplatform-indexer
```

## Docker

```sh
docker build -t rustvideoplatform-indexer .
docker run --rm \
  -v ./config.json:/config.json \
  rustvideoplatform-indexer
```

## PostgreSQL trigger

The indexer automatically creates the `LISTEN/NOTIFY` trigger on startup. If the database user lacks `CREATE` privileges, run `setup_trigger.sql` manually:

```sh
psql -U vids -d vids -f setup_trigger.sql
```
