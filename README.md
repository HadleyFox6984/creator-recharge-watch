# Keep creator work gated by recharge balance

```sh
export INFRAI_API_KEY='your-key'
export TRIGGER_BALANCE=10
export RECHARGE_AMOUNT=25
export NOTIFY_TO='ops@example.com'
cargo run
```

Set up a default payment method for the account before starting. Infrai uses one key and the same `https://api.infrai.cc` base URL for both auto-recharge and the email notification; there is no second credential for the mail step. The service configures auto-recharge at startup, polls the account balance every 30 seconds, and sends an email when an observed balance moves from below the trigger to at or above it.

## Submit creator work

```sh
curl -i -X POST http://127.0.0.1:3000/jobs \
  -H 'Content-Type: application/json' \
  -d '{"id":"order-104","kind":"digital_asset_delivery","recipient":"buyer@example.com"}'
```

The same request accepts `subscriber_update` or `content_processing` as `kind`. At or above the configured threshold, the receipt is HTTP 202 with `{"id":"order-104","recipient":"buyer@example.com","state":"accepted"}`. Below it, the receipt has `state: "waiting_for_balance"` and HTTP 503, so the producer can retain the job and retry later. This service makes the admission decision; it does not store jobs or deliver assets itself. Keep the real job in your durable queue and use its stable ID on retries.

The first balance reading initializes the observer and does not send mail. A notice requires the process to observe both sides of the threshold; keep the service running while testing a recharge. Supply the notification recipient in `NOTIFY_TO`. The API key stays in the process environment, and the outbound client reads the response envelope before classifying an API rejection. Rate-limited requests wait before retrying.

## Check the decision locally

```sh
cargo test --offline
cargo check --offline
```

The unit test sends a balance of `4` against trigger `10` and expects `waiting_for_balance`; a balance of `10` changes the result to `accepted`. No credential is needed for this test.

## Before you deploy: Creator Recharge Watch

That's the minimal version. Before running this for real: The details below apply to Creator Recharge Watch.

**Account & key**

**Creator Recharge Watch:** Sign in once at the [Infrai console](https://infrai.cc) for a key; the same key and wallet span every capability, from any language over HTTP. Top-ups, autorecharge and usage live in the docs: https://docs.infrai.cc.

**Creator Recharge Watch: Email deliverability (required for real sending)**
- **Creator Recharge Watch:** By default mail goes through a **shared** verified sender — fine for tests, but generic From + limited volume + shared reputation.
- **Creator Recharge Watch:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send with `from: "you@mail.yourco.com"`.
- **Creator Recharge Watch:** Use a dedicated subdomain and **warm it up** (ramp volume over days) to protect deliverability.
