# rustvideoplatform-indexer

Indexes media data from the [rustvideoplatform](https://github.com/panmourovaty/rustvideoplatform) PostgreSQL database into Meilisearch.

## What it does

1. **Initial sync** — reads every row from the `media` table and bulk-indexes it into a Meilisearch `media` index
2. **Live updates** — installs a PostgreSQL `LISTEN/NOTIFY` trigger on the `media` table and applies inserts, updates, and deletes to Meilisearch in real-time

## Configuration

Copy `config.example.json` to `config.json` and fill in the values:

```json
{
    "database_url": "postgresql://vids:password@localhost:5432/vids",
    "meilisearch_url": "http://localhost:7700",
    "meilisearch_key": null,
    "batch_size": 1000,
    "notify_channel": "media_changes"
}
```

You can also use environment variables instead of `config.json`:

| Variable | Required | Default |
|---|---|---|
| `DATABASE_URL` | yes | — |
| `MEILISEARCH_URL` | no | `http://localhost:7700` |
| `MEILISEARCH_KEY` | no | none |
| `BATCH_SIZE` | no | `1000` |
| `NOTIFY_CHANNEL` | no | `media_changes` |

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
