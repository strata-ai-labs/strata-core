//! Fork a branch and read a KV value at an earlier commit timestamp.

use std::error::Error;

use stratadb::{BranchName, CacheOpenOptions, Database, KvKey, KvValue, ProductSpace};

fn main() -> Result<(), Box<dyn Error>> {
    let mut db = Database::open_cache(CacheOpenOptions::new())?.into_database();
    let default = BranchName::new("default")?;
    let experiment = BranchName::new("experiment")?;
    let space = ProductSpace::new("agent-memory")?;
    let key = KvKey::new("plan")?;

    let first_write = db.kv(default.clone(), space.clone())?.put(
        key.clone(),
        KvValue::new(b"inspect the repository".to_vec()),
    )?;
    db.branches()?.fork_current(&default, experiment.clone())?;
    db.kv(default.clone(), space.clone())?
        .put(key.clone(), KvValue::new(b"submit the fix".to_vec()))?;

    let original = db
        .kv(default, space.clone())?
        .get_at(&key, first_write.commit().timestamp())?
        .expect("the first version exists");
    let forked = db
        .kv(experiment, space)?
        .get(&key)?
        .expect("the fork inherited the first value");

    println!(
        "as of first write: {}",
        String::from_utf8_lossy(original.as_bytes())
    );
    println!(
        "forked value: {}",
        String::from_utf8_lossy(forked.as_bytes())
    );
    Ok(())
}
