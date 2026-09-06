//! Open a durable database and write a KV value to it.

use std::{error::Error, path::PathBuf};

use stratadb::{BranchName, Database, DurableLocalOpenOptions, KvKey, KvValue, ProductSpace};

fn main() -> Result<(), Box<dyn Error>> {
    let path = std::env::args_os()
        .nth(1)
        .map_or_else(|| PathBuf::from("strata-example.db"), PathBuf::from);
    let mut db = Database::open_local(&path, DurableLocalOpenOptions::new())?.into_database();

    db.kv(
        BranchName::new("default")?,
        ProductSpace::new("agent-memory")?,
    )?
    .put(KvKey::new("status")?, KvValue::new(b"persisted".to_vec()))?;

    println!("opened durable database at {}", path.display());
    Ok(())
}
