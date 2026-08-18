//! OmniSeed Provider Protocol v1 adapter for Omnicede's durable graph memory.
//!
//! The Provider ID identifies the supplying Omnicede organisation boundary.
//! SQLite, HNSW, and the Omnicede agent application are implementation choices
//! beneath that Provider; this adapter advertises only the canonical `memory`
//! primitive family.

use serde_json::{json, Value};

use crate::db::{queries, Db};
use crate::error::{CortexError, Result};
use crate::types::{Node, NodeKind};

pub const PROTOCOL: &str = "omniseed.provider.protocol/1.0";
pub const METHODS: [&str; 8] = [
    "provider.initialize",
    "provider.status",
    "provider.validate",
    "provider.plan",
    "provider.apply",
    "provider.observe",
    "provider.invoke",
    "provider.shutdown",
];

pub struct OmnicedeProvider {
    db: Db,
    company_id: String,
    database_path: String,
}

impl OmnicedeProvider {
    pub fn initialize(params: &Value) -> Result<Self> {
        let protocol = params
            .get("protocolVersion")
            .and_then(Value::as_str)
            .unwrap_or("");
        if protocol != PROTOCOL {
            return Err(CortexError::Config(format!(
                "unsupported Provider protocol: {protocol}"
            )));
        }
        let configuration = params.get("configuration").and_then(Value::as_object);
        let context = params.get("context").and_then(Value::as_object);
        let company_id = context
            .and_then(|value| value.get("companyId"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CortexError::Config("Provider context requires companyId".into()))?
            .to_string();
        let database_path = configuration
            .and_then(|value| value.get("databasePath"))
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CortexError::Config("Provider configuration requires databasePath".into())
            })?
            .to_string();
        let db = Db::open(&database_path)?;
        Ok(Self {
            db,
            company_id,
            database_path,
        })
    }

    pub fn initialization(&self) -> Value {
        json!({
            "protocolVersion": PROTOCOL,
            "provider": {
                "id": "omnicede",
                "name": "Omnicede",
                "organisation": "Omnicede",
                "version": env!("CARGO_PKG_VERSION")
            },
            "primitiveFamilies": ["memory"],
            "offerings": [
                {"family": "memory", "id": "organisational_context"},
                {"family": "memory", "id": "engineering_history"},
                {"family": "memory", "id": "retained_company_knowledge"}
            ],
            "operations": ["index", "update", "remove", "search", "retrieve"],
            "methods": METHODS
        })
    }

    pub fn status(&self) -> Value {
        json!({
            "implementation_available": true,
            "configured": !self.database_path.is_empty(),
            "connected": true,
            "healthy": true
        })
    }

    pub fn validate(&self, action: &Value) -> Value {
        let valid = action.get("family").and_then(Value::as_str) == Some("memory")
            && action.get("resourceId").and_then(Value::as_str).is_some();
        json!({
            "valid": valid,
            "issues": if valid { vec![] } else { vec![json!({"message": "Omnicede requires a memory action with resourceId"})] }
        })
    }

    pub fn plan(&self, action: &Value) -> Value {
        json!({"deterministic": true, "actionId": action.get("id").cloned().unwrap_or(Value::Null)})
    }

    pub fn apply(&self, action: &Value) -> Result<Value> {
        if self.validate(action).get("valid").and_then(Value::as_bool) != Some(true) {
            return Err(CortexError::Config("invalid Omnicede memory action".into()));
        }
        let resource_id = action.get("resourceId").and_then(Value::as_str).unwrap();
        Ok(json!({
            "providerResourceId": format!("omnicede/memory/{}/{}", self.company_id, resource_id),
            "status": "deployed",
            "attributes": {"companyId": self.company_id, "durable": true, "storage": "sqlite"}
        }))
    }

    pub async fn observe(&self, resource: &Value) -> Result<Value> {
        let count = self.db.call(queries::node_count).await?;
        Ok(json!({
            "status": "healthy",
            "checkedAt": chrono::Utc::now().to_rfc3339(),
            "providerResourceId": resource.get("providerResourceId").cloned().unwrap_or(Value::Null),
            "evidence": [{
                "type": "omnicede_memory_observation",
                "source": "omnicede",
                "value": "healthy",
                "companyId": self.company_id,
                "nodeCount": count,
                "providerVersion": env!("CARGO_PKG_VERSION")
            }]
        }))
    }

    pub async fn invoke(&self, operation: &str, input: &Value) -> Result<Value> {
        self.require_company(input)?;
        match operation {
            "index" | "update" => self.index(input).await,
            "remove" => self.remove(input).await,
            "retrieve" => self.retrieve(input).await,
            "search" => self.search(input).await,
            _ => Err(CortexError::Config(format!(
                "unsupported Provider operation: {operation}"
            ))),
        }
    }

    fn require_company(&self, input: &Value) -> Result<()> {
        if input.get("companyId").and_then(Value::as_str) != Some(self.company_id.as_str()) {
            return Err(CortexError::Config(
                "memory request crosses the configured company boundary".into(),
            ));
        }
        Ok(())
    }

    async fn index(&self, input: &Value) -> Result<Value> {
        let item = input
            .get("item")
            .and_then(Value::as_object)
            .ok_or_else(|| CortexError::Config("index requires item".into()))?;
        let id = item
            .get("id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CortexError::Config("memory item requires id".into()))?;
        let provenance = item
            .get("provenance")
            .and_then(Value::as_object)
            .ok_or_else(|| CortexError::Config("memory item requires provenance".into()))?;
        if provenance
            .get("sourceReference")
            .and_then(Value::as_str)
            .is_none()
            || provenance.get("kind").and_then(Value::as_str).is_none()
        {
            return Err(CortexError::Config(
                "memory item provenance requires sourceReference and kind".into(),
            ));
        }
        let title = item
            .get("title")
            .and_then(Value::as_str)
            .or_else(|| item.get("summary").and_then(Value::as_str))
            .unwrap_or(id);
        let mut node = Node::new(NodeKind::Fact, title);
        node.id = id.to_string();
        node.body = Some(Value::Object(item.clone()).to_string());
        let stored = node.id.clone();
        self.db
            .call(move |conn| queries::upsert_node(conn, &node))
            .await?;
        Ok(
            json!({"id": stored, "companyId": self.company_id, "indexedAt": chrono::Utc::now().to_rfc3339()}),
        )
    }

    async fn remove(&self, input: &Value) -> Result<Value> {
        let id = input
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CortexError::Config("remove requires id".into()))?
            .to_string();
        Ok(Value::Bool(
            self.db
                .call(move |conn| queries::delete_node(conn, &id))
                .await?,
        ))
    }

    async fn retrieve(&self, input: &Value) -> Result<Value> {
        let id = input
            .get("id")
            .and_then(Value::as_str)
            .ok_or_else(|| CortexError::Config("retrieve requires id".into()))?
            .to_string();
        let node = self
            .db
            .call(move |conn| queries::get_node(conn, &id))
            .await?;
        Ok(node
            .and_then(node_item)
            .map(|item| result(item, 1.0))
            .unwrap_or(Value::Null))
    }

    async fn search(&self, input: &Value) -> Result<Value> {
        let query = input
            .get("query")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let limit = input
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(20)
            .min(100) as usize;
        let nodes = self
            .db
            .call(move |conn| queries::search_nodes_text(conn, &query, limit))
            .await?;
        Ok(Value::Array(
            nodes
                .into_iter()
                .filter_map(node_item)
                .map(|item| result(item, 1.0))
                .collect(),
        ))
    }
}

fn node_item(node: Node) -> Option<Value> {
    node.body.and_then(|body| serde_json::from_str(&body).ok())
}

fn result(item: Value, relevance: f64) -> Value {
    let object = item.as_object().cloned().unwrap_or_default();
    let provenance = object
        .get("provenance")
        .cloned()
        .unwrap_or_else(|| json!({}));
    let source_reference = provenance
        .get("sourceReference")
        .cloned()
        .unwrap_or(Value::Null);
    let kind = provenance.get("kind").cloned().unwrap_or(Value::Null);
    json!({
        "id": object.get("id").cloned().unwrap_or(Value::Null),
        "sourceReference": source_reference,
        "kind": kind,
        "title": object.get("title").cloned().unwrap_or(Value::Null),
        "summary": object.get("summary").cloned().unwrap_or(Value::Null),
        "content": object.get("content").cloned().unwrap_or(Value::Null),
        "provenance": provenance,
        "relevanceScore": relevance,
        "capabilityReferences": object.get("capabilityReferences").cloned().unwrap_or_else(|| json!([])),
        "evidenceReferences": object.get("evidenceReferences").cloned().unwrap_or_else(|| json!([])),
        "metadata": object.get("metadata").cloned().unwrap_or_else(|| json!({}))
    })
}
