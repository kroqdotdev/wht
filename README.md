# wht — Webhook Tester

A fast, flexible CLI and web-based tool for sending HTTP webhooks. Control rate, volume, concurrency, and payload — from a single request to sustained high-throughput bursts.

## Features

- **Multiple send modes** — single shot, fixed count, timed duration, or interactive (press Enter)
- **Rate limiting** — requests per second with token-bucket throttling
- **Concurrency** — parallel connections for maximum throughput
- **Built-in templates** — GitHub push, Stripe payment, Slack message, and more
- **Sized payloads** — generate valid JSON payloads up to 25MB
- **Web GUI** — browser-based interface with real-time progress via SSE
- **Full stats** — success/fail counts, throughput, latency percentiles (p50/p95/p99)

## Install

```bash
# Clone and build
git clone https://github.com/kroqdotdev/wht.git
cd wht
cargo build --release

# Binary is at target/release/wht
```

Requires [Rust](https://rustup.rs/) 1.85+.

## CLI Usage

```
wht <URL> [OPTIONS]
```

### Send Modes

```bash
# Single request (default)
wht https://example.com/webhook

# Send 100 requests as fast as possible
wht https://example.com/webhook -n 100

# Send 100 requests at 10/sec
wht https://example.com/webhook -n 100 -r 10

# Send for 30 seconds with 5 concurrent connections
wht https://example.com/webhook -t 30 -c 5

# Interactive — press Enter to fire one at a time
wht https://example.com/webhook -i
```

### Payloads

```bash
# Inline data
wht https://example.com/webhook -d '{"event": "test"}'

# From file
wht https://example.com/webhook -f payload.json

# Built-in template
wht https://example.com/webhook --template github-push

# Generated sized payload (up to 25mb)
wht https://example.com/webhook --size 5mb
```

### HTTP Methods & Headers

```bash
# Different method
wht https://example.com/api -m PATCH -d '{"status": "active"}'

# Custom headers
wht https://example.com/webhook -H "Authorization: Bearer token" -H "X-Custom: value"

# Custom content type
wht https://example.com/webhook --content-type text/plain -d "hello"
```

### Templates

```bash
# List available templates
wht https://example.com/webhook --template list
```

| Template | Description |
|---|---|
| `github-push` | GitHub push event webhook |
| `stripe-payment` | Stripe payment_intent.succeeded |
| `slack-msg` | Slack incoming webhook message |
| `json-simple` | Simple JSON with timestamp |
| `form-data` | URL-encoded form body |

### Output

```
Sending 100 requests to https://example.com/webhook [POST] (rate limited) (concurrency: 4)

--- Results ---
Total:      100 requests
Success:    98 (2xx)
Failed:     2
Duration:   10.1s
Throughput: 9.9 req/s
Latency:    avg=45ms  min=12ms  max=230ms  p50=38ms  p95=120ms  p99=210ms
```

## Web GUI

Launch the browser-based interface:

```bash
wht gui              # starts on http://localhost:3000
wht gui --port 8080  # custom port
```

The web GUI provides:

- URL, method, headers, and content-type configuration
- Payload editor with manual input, template selection, or sized generation
- Single / Count / Timed send modes with concurrency and RPS controls
- Real-time progress bar and live request log via Server-Sent Events
- Cancel running jobs mid-flight
- Full results with latency stats and percentiles

## All Options

```
wht <URL> [OPTIONS]

Arguments:
  <URL>  Target URL

Options:
  -m, --method <METHOD>              HTTP method [default: POST]
  -n, --count <COUNT>                Number of requests to send
  -r, --rps <RPS>                    Requests per second (rate limit)
  -t, --duration <DURATION>          Send for a duration in seconds
  -i, --interactive                  Interactive mode
  -c, --concurrency <CONCURRENCY>    Concurrent connections [default: 1]
  -d, --data <DATA>                  Inline request body
  -f, --file <FILE>                  Read body from file
      --template <TEMPLATE>          Built-in template name
      --size <SIZE>                  Generate payload of this size
  -H, --header <HEADERS>             Add header (Key: Value), repeatable
      --content-type <CONTENT_TYPE>  Content-Type header [default: application/json]
  -h, --help                         Print help
  -V, --version                      Print version

Subcommands:
  gui   Launch the web GUI
```

## License

[MIT](LICENSE)
