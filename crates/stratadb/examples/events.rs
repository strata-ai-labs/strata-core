//! Append a structured event to an in-memory database.

use std::error::Error;

use serde_json::json;
use stratadb::{BranchName, CacheOpenOptions, Database, EventPayload, EventType, ProductSpace};

fn main() -> Result<(), Box<dyn Error>> {
    let mut db = Database::open_cache(CacheOpenOptions::new())?.into_database();
    let mut events = db.event(
        BranchName::new("default")?,
        ProductSpace::new("agent-memory")?,
    )?;

    let appended = events.append(
        EventType::new("tool.completed")?,
        EventPayload::new(json!({"tool": "search", "matches": 3}))?,
    )?;

    println!("appended event sequence {}", appended.sequence().as_u64());
    Ok(())
}
