use std::io::Write;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, BufReader};

use omnicede::omniseed_provider::OmnicedeProvider;

#[tokio::main]
async fn main() {
    let mut lines = BufReader::new(tokio::io::stdin()).lines();
    let mut provider: Option<OmnicedeProvider> = None;
    while let Ok(Some(line)) = lines.next_line().await {
        let request: Value = match serde_json::from_str(&line) {
            Ok(value) => value,
            Err(_) => {
                respond(
                    Value::Null,
                    None,
                    Some(json!({"code": -32700, "message": "Parse error"})),
                );
                continue;
            }
        };
        let id = request.get("id").cloned().unwrap_or(Value::Null);
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let params = request.get("params").cloned().unwrap_or_else(|| json!({}));
        let result = match method {
            "provider.initialize" => match OmnicedeProvider::initialize(&params) {
                Ok(value) => {
                    let response = value.initialization();
                    provider = Some(value);
                    Ok(response)
                }
                Err(error) => Err(error.to_string()),
            },
            "provider.status" => current(&provider).map(|value| value.status()),
            "provider.validate" => current(&provider)
                .map(|value| value.validate(params.get("action").unwrap_or(&Value::Null))),
            "provider.plan" => current(&provider)
                .map(|value| value.plan(params.get("action").unwrap_or(&Value::Null))),
            "provider.apply" => match current(&provider) {
                Ok(value) => value
                    .apply(params.get("action").unwrap_or(&Value::Null))
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            },
            "provider.observe" => match current(&provider) {
                Ok(value) => value
                    .observe(params.get("resource").unwrap_or(&Value::Null))
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            },
            "provider.invoke" => match current(&provider) {
                Ok(value) => value
                    .invoke(
                        params
                            .get("operation")
                            .and_then(Value::as_str)
                            .unwrap_or(""),
                        params.get("input").unwrap_or(&Value::Null),
                    )
                    .await
                    .map_err(|e| e.to_string()),
                Err(e) => Err(e),
            },
            "provider.shutdown" => Ok(json!({"shutdown": true})),
            _ => {
                respond(
                    id,
                    None,
                    Some(json!({"code": -32601, "message": "Method not found"})),
                );
                continue;
            }
        };
        match result {
            Ok(value) => respond(id, Some(value), None),
            Err(message) => respond(id, None, Some(json!({"code": -32000, "message": message}))),
        }
        if method == "provider.shutdown" {
            break;
        }
    }
}

fn current(provider: &Option<OmnicedeProvider>) -> std::result::Result<&OmnicedeProvider, String> {
    provider
        .as_ref()
        .ok_or_else(|| "Provider is not initialized".into())
}

fn respond(id: Value, result: Option<Value>, error: Option<Value>) {
    let value = if let Some(error) = error {
        json!({"jsonrpc": "2.0", "id": id, "error": error})
    } else {
        json!({"jsonrpc": "2.0", "id": id, "result": result.unwrap_or(Value::Null)})
    };
    let mut stdout = std::io::stdout().lock();
    let _ = writeln!(stdout, "{}", value);
    let _ = stdout.flush();
}
