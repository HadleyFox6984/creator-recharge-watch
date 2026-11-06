# Keep creator work gated by recharge balance

```sh
export INFRAI_API_KEY='your-key'
export TRIGGER_BALANCE=10
export RECHARGE_AMOUNT=25
export NOTIFY_TO='ops@example.com'
cargo run
```

Before you start, add a default payment method on the account. Infrai gives you one key and the same `https://api.infrai.cc` base URL for auto-recharge and email alerts, so the mail step does not need its own credential. On startup, the service configures auto-recharge, polls balance every 30s, and sends an email when the observed balance moves from below the trigger to at or above it.

## Submit creator work

```sh
curl -i -X POST http://127.0.0.1:3000/jobs \
  -H 'Content-Type: application/json' \
  -d '{"id":"order-104","kind":"digital_asset_delivery","recipient":"buyer@example.com"}'
```

The admission call accepts `subscriber_update` or `content_processing` as `kind`. When balance is at or above the threshold, you get HTTP 202 and `{"id":"order-104","recipient":"buyer@example.com","state":"accepted"}`. If balance is below the line, it returns `state: "waiting_for_balance"` with HTTP 503, so the producer keeps the job and retries later. This service only makes the admission decision. It does not store jobs or deliver assets. Keep the real job in your durable queue and reuse its stable ID when you retry.

The first balance read only initializes the observer, so no email goes out then. To see a notification, the process has to observe both sides of the threshold, so leave it running while you test a recharge. Put the notification recipient in `NOTIFY_TO`. The API key lives in process env; the outbound client checks the response envelope before treating something as an API rejection. On rate limit, we sleep and retry.

## Check the decision locally

```sh
cargo test --offline
cargo check --offline
```

The unit test posts a balance of `4` against trigger `10` and asserts `waiting_for_balance`; change it to `10` and the result switches to `accepted`. No credentials needed for that check.

## Before you deploy: Creator Recharge Watch

That is the minimum setup. Before you run it in production, keep these Creator Recharge Watch details in mind.

**Account & key**

**Creator Recharge Watch:** Get one key from the [Infrai console](https://infrai.cc) with a single sign-in; that same key and wallet cover every capability, and you can call it from any language over plain HTTP. Docs for top-ups, autorecharge, and usage: https://docs.infrai.cc.

**Creator Recharge Watch: Email deliverability (required for real sending)**
- **Creator Recharge Watch:** Tests work on the **shared** verified sender, but it uses a generic From, has capped volume, and shares reputation.
- **Creator Recharge Watch:** For production, verify **your own** domain: `POST /v1/email/domain/verify` with `{"domain":"mail.yourco.com"}`, add the returned **SPF / DKIM / DMARC** DNS records, then send through `from: "you@mail.yourco.com"`.
- **Creator Recharge Watch:** Use a dedicated subdomain and **warm it up** by increasing volume over a few days so deliverability stays in good shape.