use omnicede::omniseed_provider::{OmnicedeProvider, PROTOCOL};
use serde_json::json;

fn provider(path: &str) -> OmnicedeProvider {
    OmnicedeProvider::initialize(&json!({
        "protocolVersion": PROTOCOL,
        "configuration": {"databasePath": path},
        "context": {"companyId": "omniseed_ecosystem"}
    }))
    .expect("initialize Provider")
}

#[tokio::test]
async fn provider_is_memory_only_and_observation_contains_evidence() {
    let file = std::env::temp_dir().join(format!("omnicede-provider-{}.db", uuid::Uuid::new_v4()));
    let subject = provider(file.to_str().unwrap());
    let initialized = subject.initialization();
    assert_eq!(initialized["provider"]["id"], "omnicede");
    assert_eq!(initialized["primitiveFamilies"], json!(["memory"]));
    let action = json!({"id": "memory-1", "family": "memory", "resourceId": "ecosystem_memory"});
    assert_eq!(subject.validate(&action)["valid"], true);
    let applied = subject.apply(&action).expect("apply");
    let observed = subject.observe(&applied).await.expect("observe");
    assert_eq!(observed["status"], "healthy");
    assert_eq!(observed["evidence"][0]["source"], "omnicede");
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn provider_round_trips_company_isolated_memory() {
    let file = std::env::temp_dir().join(format!("omnicede-provider-{}.db", uuid::Uuid::new_v4()));
    let subject = provider(file.to_str().unwrap());
    let item = json!({
        "id": "decision-1",
        "title": "Provider naming decision",
        "content": "Providers identify supplying organisations.",
        "provenance": {"sourceReference": "git:decision-1", "kind": "decision"},
        "capabilityReferences": ["steward_omniseed_ecosystem"],
        "evidenceReferences": ["git:decision-1"]
    });
    subject
        .invoke(
            "index",
            &json!({"companyId": "omniseed_ecosystem", "item": item}),
        )
        .await
        .expect("index");
    let found = subject
        .invoke(
            "search",
            &json!({"companyId": "omniseed_ecosystem", "query": "supplying organisations"}),
        )
        .await
        .expect("search");
    assert_eq!(found.as_array().unwrap().len(), 1);
    assert_eq!(found[0]["id"], "decision-1");
    let retrieved = subject
        .invoke(
            "retrieve",
            &json!({"companyId": "omniseed_ecosystem", "id": "decision-1"}),
        )
        .await
        .expect("retrieve");
    assert_eq!(retrieved["sourceReference"], "git:decision-1");
    let crossed = subject
        .invoke(
            "retrieve",
            &json!({"companyId": "another_company", "id": "decision-1"}),
        )
        .await;
    assert!(crossed.is_err());
    let _ = std::fs::remove_file(file);
}

#[test]
fn provider_rejects_missing_durable_path() {
    let result = OmnicedeProvider::initialize(&json!({
        "protocolVersion": PROTOCOL,
        "configuration": {},
        "context": {"companyId": "omniseed_ecosystem"}
    }));
    assert!(result.is_err());
}
