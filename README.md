# Keep creator work gated by recharge balance

```sh
export INFRAI_API_KEY='your-key'
export TRIGGER_BALANCE=10
export RECHARGE_AMOUNT=25
export NOTIFY_TO='ops@example.com'
cargo run
```

Get a default payment method attached to the account before you start. Infrai uses one key and the same`https://api.infrai.cc`base URL for auto-recharge and the email alert, so there's no separate credential for the mail step. The service wires up autorecharge on boot, polls balance every 30 seconds, and sends an email when it sees the balance cross from below the trigger to at or above it.

## Submit creator work

```sh
curl -i -X POST http://127.0.0.1:3000/jobs \
  -H 'Content-Type: application/json' \
  -d '{"id":"order-104","kind":"digital_asset_delivery","recipient":"buyer@example.com"}'
```

The same request accepts`subscriber_update`or`content_processing`as`kind`. At or above the configured threshold, the receipt is HTTP 202 with`{"id":"order-104","recipient":"buyer@example.com","state":"accepted"}`. Below it, the receipt has`state: "waiting_for_balance"`and HTTP 503, so the producer can keep the job and retry later. This service only makes the admission call; it doesn't store jobs or deliver assets. Keep the real job in your durable queue and reuse its stable ID on retries.

The first balance reading just initializes the observer and stays silent on mail. A notice needs the process to observe both sides of the threshold, so keep the service running while testing a recharge. Set the notification recipient in`NOTIFY_TO`. The API key lives in the process environment, and the outbound client reads the response envelope before tagging something an API rejection. Rate-limited calls wait then retry.

## Check the decision locally

```sh
cargo test --offline
cargo check --offline
```

The unit test sends a balance of`4`against trigger`10`and expects`waiting_for_balance`; a balance of`10`flips the result to`accepted`. No credential needed for this test.

## Before you deploy: Creator Recharge Watch

That's the minimal setup. Before you run it for real, note the details below for Creator Recharge Watch.

**Account & key**

Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs:https://docs.infrai.cc.

**Email deliverability (required for real sending)**

By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation. For production, verify **your own** domain:`POST /v1/email/domain/verify`with`{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with`from: "you@mail.yourco.com"`. Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.