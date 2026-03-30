# wht

Send webhooks from the command line or a browser. One request or ten thousand, with rate limiting, concurrency, templates, and payloads up to 25MB.

## Install

```bash
git clone https://github.com/kroqdotdev/wht.git
cd wht
cargo build --release
# binary: target/release/wht
```

Requires [Rust](https://rustup.rs/) 1.85+.

## Usage

```bash
# single request
wht https://example.com/webhook -d '{"event": "test"}'

# 100 requests at 10/sec with 4 concurrent connections
wht https://example.com/webhook -n 100 -r 10 -c 4

# send for 30 seconds
wht https://example.com/webhook -t 30

# interactive — press Enter to fire each request
wht https://example.com/webhook -i

# use a built-in template
wht https://example.com/webhook --template github-push

# generate a 5MB payload
wht https://example.com/webhook --size 5mb

# PATCH with custom headers
wht https://example.com/api -m PATCH -d '{"status": "active"}' \
  -H "Authorization: Bearer token"
```

Output:

```
--- Results ---
Total:      100 requests
Success:    98 (2xx)
Failed:     2
Duration:   10.1s
Throughput: 9.9 req/s
Latency:    avg=45ms  min=12ms  max=230ms  p50=38ms  p95=120ms  p99=210ms
```

## Templates

| Name | Payload |
|---|---|
| `github-push` | GitHub push event |
| `stripe-payment` | Stripe payment_intent.succeeded |
| `slack-msg` | Slack incoming webhook |
| `json-simple` | `{"event", "message", "timestamp"}` |
| `form-data` | URL-encoded form body |

## Web GUI

```bash
wht gui              # http://localhost:3000
wht gui --port 8080
```

Same controls as the CLI — URL, method, headers, payload, send mode — in the browser. Progress streams in real time. You can cancel running jobs.

## License

[MIT](LICENSE)
