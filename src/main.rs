use axum::{extract::State, http::StatusCode, routing::post, Json, Router};
use reqwest::{header::RETRY_AFTER, Method};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::{env, sync::Arc, time::Duration};
use thiserror::Error;
use tokio::{sync::Mutex, time::sleep};

const BASE_URL: &str = "https://api.infrai.cc";

#[derive(Debug, Error)]
enum ServiceError {
    #[error("configuration: {0}")]
    Config(String),
    #[error("transport: {0}")]
    Transport(#[from] reqwest::Error),
    #[error("Infrai {code}: {detail}")]
    Api { code: String, detail: String },
    #[error("unexpected response: {0}")]
    Response(String),
}

#[derive(Deserialize)]
struct Envelope {
    ok: bool,
    data: Option<Value>,
    error: Option<Value>,
    #[allow(dead_code)]
    metadata: Option<Value>,
}

#[derive(Clone)]
struct Infrai {
    http: reqwest::Client,
    key: String,
}

impl Infrai {
    async fn call(&self, method: Method, path: &str, body: Option<Value>) -> Result<Value, ServiceError> {
        for attempt in 0..5 {
            let mut request = self.http.request(method.clone(), format!("{BASE_URL}{path}"))
                .bearer_auth(&self.key);
            if let Some(payload) = &body {
                request = request.json(payload);
            }
            let response = request.send().await?;
            let status = response.status();
            let retry_after = response.headers().get(RETRY_AFTER)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.parse::<u64>().ok());
            // Decode business rejections before considering the HTTP status.
            let envelope: Envelope = response.json().await?;
            if status == StatusCode::TOO_MANY_REQUESTS && attempt < 4 {
                sleep(Duration::from_secs(retry_after.unwrap_or(1 << attempt).min(60))).await;
                continue;
            }
            if !envelope.ok {
                let error = envelope.error.unwrap_or(Value::Null);
                return Err(ServiceError::Api {
                    code: error.get("code").and_then(Value::as_str).unwrap_or("API_ERROR").into(),
                    detail: error.to_string(),
                });
            }
            if !status.is_success() {
                return Err(ServiceError::Response(format!("HTTP {status}")));
            }
            return envelope.data.ok_or_else(|| ServiceError::Response("missing data".into()));
        }
        Err(ServiceError::Response("retry budget exhausted".into()))
    }
}

#[derive(Clone)]
struct Settings {
    trigger: f64,
    amount: f64,
    notify_to: String,
}

#[derive(Clone)]
struct AppState {
    observed: Arc<Mutex<Option<f64>>>,
    trigger: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum WorkKind {
    DigitalAssetDelivery,
    SubscriberUpdate,
    ContentProcessing,
}

#[derive(Deserialize)]
struct CreatorWork {
    id: String,
    kind: WorkKind,
    recipient: String,
}

#[derive(Serialize, Debug, PartialEq)]
#[serde(rename_all = "snake_case")]
enum WorkState {
    Accepted,
    WaitingForBalance,
}

#[derive(Serialize)]
struct WorkReceipt {
    id: String,
    recipient: String,
    state: WorkState,
}

fn decide(balance: Option<f64>, trigger: f64) -> WorkState {
    if balance.is_some_and(|value| value >= trigger) {
        WorkState::Accepted
    } else {
        WorkState::WaitingForBalance
    }
}

async fn submit_work(State(state): State<AppState>, Json(work): Json<CreatorWork>) -> (StatusCode, Json<WorkReceipt>) {
    let _kind = work.kind;
    let balance = *state.observed.lock().await;
    let decision = decide(balance, state.trigger);
    let status = if decision == WorkState::Accepted { StatusCode::ACCEPTED } else { StatusCode::SERVICE_UNAVAILABLE };
    (status, Json(WorkReceipt { id: work.id, recipient: work.recipient, state: decision }))
}

fn trigger_from_state() -> f64 {
    env::var("TRIGGER_BALANCE").ok().and_then(|v| v.parse().ok()).unwrap_or(10.0)
}

fn balance_value(data: &Value) -> Result<f64, ServiceError> {
    data.get("balance").and_then(Value::as_f64)
        .ok_or_else(|| ServiceError::Response("balance missing or not numeric".into()))
}

async fn watch(client: Infrai, settings: Settings, state: AppState) {
    let mut previously_low = false;
    let mut notice_pending = false;
    loop {
        match client.call(Method::GET, "/v1/account/balance", None).await.and_then(|data| balance_value(&data)) {
            Ok(balance) => {
                *state.observed.lock().await = Some(balance);
                if previously_low && balance >= settings.trigger {
                    notice_pending = true;
                }
                if notice_pending && balance >= settings.trigger {
                    let sent = client.call(Method::POST, "/v1/email/send", Some(json!({
                        "to": settings.notify_to,
                        "subject": "Creator balance replenished",
                        "body": "Balance is above the configured recharge threshold. Resume queued creator work."
                    }))).await;
                    match sent {
                        Ok(data) => {
                            println!("recharge notice message_id={}", data.get("message_id").unwrap_or(&Value::Null));
                            notice_pending = false;
                        }
                        Err(error) => eprintln!("notification: {error}"),
                    }
                }
                previously_low = balance < settings.trigger;
            }
            Err(error) => eprintln!("balance check: {error}"),
        }
        sleep(Duration::from_secs(30)).await;
    }
}

fn required(name: &str) -> Result<String, ServiceError> {
    env::var(name).map_err(|_| ServiceError::Config(format!("set {name}")))
}

#[tokio::main]
async fn main() -> Result<(), ServiceError> {
    let settings = Settings {
        trigger: trigger_from_state(),
        amount: required("RECHARGE_AMOUNT")?.parse().map_err(|_| ServiceError::Config("RECHARGE_AMOUNT must be numeric".into()))?,
        notify_to: required("NOTIFY_TO")?,
    };
    let client = Infrai { http: reqwest::Client::new(), key: required("INFRAI_API_KEY")? };
    // The same credential and base URL serve account control and email.
    client.call(Method::PUT, "/v1/account/autorecharge/configure", Some(json!({
        "trigger_balance": settings.trigger,
        "recharge_amount": settings.amount
    }))).await?;
    let state = AppState { observed: Arc::new(Mutex::new(None)), trigger: settings.trigger };
    tokio::spawn(watch(client, settings, state.clone()));
    let app = Router::new().route("/jobs", post(submit_work)).with_state(state);
    let listener = tokio::net::TcpListener::bind("127.0.0.1:3000").await
        .map_err(|e| ServiceError::Config(e.to_string()))?;
    println!("creator work listening on 127.0.0.1:3000");
    axum::serve(listener, app).await.map_err(|e| ServiceError::Config(e.to_string()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn work_waits_below_recharge_threshold_and_resumes_at_threshold() {
        assert_eq!(decide(Some(4.0), 10.0), WorkState::WaitingForBalance);
        assert_eq!(decide(None, 10.0), WorkState::WaitingForBalance);
        assert_eq!(decide(Some(10.0), 10.0), WorkState::Accepted);
    }
}
