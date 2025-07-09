# Solana Geyser Plugin NATS

A Solana geyser plugin that streams transaction data from the validator to NATS messaging system.

## Features

- Streams transactions from Solana validator to NATS in real-time
- Configurable transaction filtering (vote/non-vote or specific addresses)
- Transactions are sent in JSON format
- Automatic retry logic with configurable timeouts

## Configuration

Create a JSON configuration file:

```json
{
    "libpath": "/path/to/libagave_geyser_plugin_nats.so",
    "nats_url": "nats://localhost:4222",
    "subject": "solana.transactions",
    "max_retries": 5,
    "timeout_secs": 10,
    "filter": {
        "select_all_transactions": true,
        "select_vote_transactions": false,
        "mentioned_addresses": []
    }
}
```

### Configuration Options

- `libpath`: Path to the compiled plugin library (.so file on Linux, .dylib on macOS)
- `nats_url`: NATS server connection URL
- `subject`: NATS subject to publish transactions to
- `max_retries`: Number of retry attempts for failed publishes (default: 5)
- `timeout_secs`: Connection timeout in seconds (default: 10)
- `filter.select_all_transactions`: Include all non-vote transactions (default: true)
- `filter.select_vote_transactions`: Include voting transactions (default: false)
- `filter.mentioned_addresses`: Specific account addresses to filter ("*" for all, empty for default)

## Usage

Configure your Solana validator to load this plugin:

```bash
solana-validator --geyser-plugin-config config.json
```

## License

Licensed under the GNU Affero General Public License v3.0 (AGPL-3.0).
