use chrono::Utc;

pub const TEMPLATE_NAMES: &[&str] = &[
    "github-push",
    "stripe-payment",
    "slack-msg",
    "json-simple",
    "form-data",
];

/// Return the template names as a vec (for API use).
pub fn template_names() -> Vec<&'static str> {
    TEMPLATE_NAMES.to_vec()
}

/// Return the body string for a named built-in template.
pub fn get_template(name: &str) -> Result<String, String> {
    let now = Utc::now().to_rfc3339();

    match name {
        "github-push" => Ok(github_push(&now)),
        "stripe-payment" => Ok(stripe_payment(&now)),
        "slack-msg" => Ok(slack_message()),
        "json-simple" => Ok(json_simple(&now)),
        "form-data" => Ok(form_data()),
        _ => Err(format!(
            "Unknown template: {name}\nAvailable: {}\nUse --template list to see details",
            TEMPLATE_NAMES.join(", ")
        )),
    }
}

pub fn list_templates() {
    println!("Available templates:");
    println!("  github-push     GitHub push event webhook");
    println!("  stripe-payment  Stripe payment_intent.succeeded event");
    println!("  slack-msg       Slack incoming webhook message");
    println!("  json-simple     Simple JSON with key, value, timestamp");
    println!("  form-data       URL-encoded form body (sets Content-Type automatically)");
}

fn github_push(timestamp: &str) -> String {
    serde_json::json!({
        "ref": "refs/heads/main",
        "before": "abc1234000000000000000000000000000000000",
        "after": "def5678000000000000000000000000000000000",
        "repository": {
            "id": 123456789,
            "name": "webhook-tester",
            "full_name": "user/webhook-tester",
            "html_url": "https://github.com/user/webhook-tester"
        },
        "pusher": {
            "name": "testuser",
            "email": "test@example.com"
        },
        "sender": {
            "login": "testuser",
            "id": 1234567
        },
        "commits": [
            {
                "id": "def5678000000000000000000000000000000000",
                "message": "Test commit from webhook tester",
                "timestamp": timestamp,
                "author": {
                    "name": "Test User",
                    "email": "test@example.com"
                },
                "added": ["README.md"],
                "removed": [],
                "modified": []
            }
        ],
        "head_commit": {
            "id": "def5678000000000000000000000000000000000",
            "message": "Test commit from webhook tester",
            "timestamp": timestamp
        }
    })
    .to_string()
}

fn stripe_payment(timestamp: &str) -> String {
    serde_json::json!({
        "id": "evt_test_webhook_001",
        "object": "event",
        "api_version": "2023-10-16",
        "created": timestamp,
        "type": "payment_intent.succeeded",
        "data": {
            "object": {
                "id": "pi_test_001",
                "object": "payment_intent",
                "amount": 2000,
                "currency": "usd",
                "status": "succeeded",
                "payment_method": "pm_card_visa",
                "description": "Test payment from webhook tester",
                "metadata": {
                    "order_id": "order_12345"
                }
            }
        },
        "livemode": false,
        "pending_webhooks": 1
    })
    .to_string()
}

fn slack_message() -> String {
    serde_json::json!({
        "text": "Hello from webhook tester!",
        "blocks": [
            {
                "type": "section",
                "text": {
                    "type": "mrkdwn",
                    "text": "*Webhook Test* :rocket:\nThis is a test message sent by `wht`."
                }
            },
            {
                "type": "divider"
            },
            {
                "type": "section",
                "fields": [
                    {
                        "type": "mrkdwn",
                        "text": "*Status:*\nActive"
                    },
                    {
                        "type": "mrkdwn",
                        "text": "*Source:*\nwht CLI"
                    }
                ]
            }
        ]
    })
    .to_string()
}

fn json_simple(timestamp: &str) -> String {
    serde_json::json!({
        "event": "test",
        "message": "Hello from webhook tester",
        "timestamp": timestamp,
        "data": {
            "id": 1,
            "key": "value"
        }
    })
    .to_string()
}

fn form_data() -> String {
    "event=test&message=Hello+from+webhook+tester&key=value".to_string()
}
