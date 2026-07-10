//! Property test: the product branch DAG matches a simple reference model.
//!
//! A random sequence of create / fork / delete operations is applied to both
//! the engine and an independent in-memory model. After every operation the
//! model predicts the exact outcome (success or a specific error code) and the
//! full set of active branches with their generations, and the engine must
//! agree. This pins generation monotonicity, active-set correctness, the
//! undeletable default branch, and the create/fork/delete error precedence.

mod common;

use std::collections::BTreeMap;

use common::{branch, open_cache_database};
use proptest::prelude::*;
use strata_engine::{Database, EngineResult};

/// Name pool, including the always-present, undeletable `default`.
const POOL: [&str; 4] = ["default", "alpha", "beta", "gamma"];

#[derive(Clone, Copy, Debug)]
enum Op {
    Create(usize),
    Fork(usize, usize),
    Delete(usize),
}

fn any_op() -> impl Strategy<Value = Op> {
    let pool = 0usize..POOL.len();
    prop_oneof![
        pool.clone().prop_map(Op::Create),
        (pool.clone(), pool.clone()).prop_map(|(source, dest)| Op::Fork(source, dest)),
        pool.prop_map(Op::Delete),
    ]
}

#[derive(Clone, Copy)]
struct NameState {
    last_generation: u64,
    active: bool,
}

struct Model {
    names: BTreeMap<&'static str, NameState>,
}

impl Model {
    fn new() -> Self {
        let mut names = BTreeMap::new();
        names.insert(
            "default",
            NameState {
                last_generation: 1,
                active: true,
            },
        );
        Self { names }
    }

    fn is_active(&self, name: &str) -> bool {
        self.names.get(name).is_some_and(|state| state.active)
    }

    fn active_count(&self) -> usize {
        self.names.values().filter(|state| state.active).count()
    }

    fn record_create(&mut self, name: &'static str) {
        // Generation is monotonic per name across deletes (the storage rule is
        // "last generation ever assigned, plus one").
        let generation = self
            .names
            .get(name)
            .map_or(1, |state| state.last_generation + 1);
        self.names.insert(
            name,
            NameState {
                last_generation: generation,
                active: true,
            },
        );
    }

    fn record_delete(&mut self, name: &str) {
        if let Some(state) = self.names.get_mut(name) {
            state.active = false;
        }
    }

    fn active_generations(&self) -> BTreeMap<String, u64> {
        self.names
            .iter()
            .filter(|(_, state)| state.active)
            .map(|(name, state)| ((*name).to_string(), state.last_generation))
            .collect()
    }
}

#[derive(Clone, Copy)]
enum Expect {
    Ok,
    Err(&'static str),
}

fn expect_create(model: &Model, name: &str) -> Expect {
    if model.is_active(name) {
        Expect::Err("already_exists.engine.branch")
    } else {
        Expect::Ok
    }
}

fn expect_fork(model: &Model, source: &str, dest: &str) -> Expect {
    // The engine rejects a duplicate destination before checking the source.
    if model.is_active(dest) {
        Expect::Err("already_exists.engine.branch")
    } else if model.is_active(source) {
        Expect::Ok
    } else {
        Expect::Err("not_found.engine.branch")
    }
}

fn expect_delete(model: &Model, name: &str) -> Expect {
    // The engine checks the default guard, then the last-active-count guard,
    // then existence — so a missing name with only `default` active still
    // reports the last-active error, not not-found.
    if name == "default" || model.active_count() <= 1 {
        Expect::Err("invalid_argument.engine.branch_delete")
    } else if model.is_active(name) {
        Expect::Ok
    } else {
        Expect::Err("not_found.engine.branch")
    }
}

fn check_outcome(
    result: EngineResult<()>,
    expected: Expect,
    label: &str,
) -> Result<(), TestCaseError> {
    match (result, expected) {
        (Ok(()), Expect::Ok) => Ok(()),
        (Err(error), Expect::Ok) => Err(TestCaseError::fail(format!(
            "{label} should succeed but failed with {}",
            error.code()
        ))),
        (Ok(()), Expect::Err(code)) => Err(TestCaseError::fail(format!(
            "{label} should fail with {code} but succeeded"
        ))),
        (Err(error), Expect::Err(code)) => {
            prop_assert_eq!(error.code(), code);
            Ok(())
        }
    }
}

fn apply(db: &mut Database, model: &mut Model, op: Op) -> Result<(), TestCaseError> {
    match op {
        Op::Create(index) => {
            let name = POOL[index];
            let expected = expect_create(model, name);
            let result = db
                .branches()
                .expect("branch service")
                .create(branch(name))
                .map(|_| ());
            check_outcome(result, expected, "create")?;
            if matches!(expected, Expect::Ok) {
                model.record_create(name);
            }
        }
        Op::Fork(source_index, dest_index) => {
            let source = POOL[source_index];
            let dest = POOL[dest_index];
            let expected = expect_fork(model, source, dest);
            let result = db
                .branches()
                .expect("branch service")
                .create_from_head(&branch(source), branch(dest))
                .map(|_| ());
            check_outcome(result, expected, "fork")?;
            if matches!(expected, Expect::Ok) {
                model.record_create(dest);
            }
        }
        Op::Delete(index) => {
            let name = POOL[index];
            let expected = expect_delete(model, name);
            let result = db
                .branches()
                .expect("branch service")
                .delete(&branch(name))
                .map(|_| ());
            check_outcome(result, expected, "delete")?;
            if matches!(expected, Expect::Ok) {
                model.record_delete(name);
            }
        }
    }
    Ok(())
}

fn check_invariants(db: &mut Database, model: &Model) -> Result<(), TestCaseError> {
    let summaries = db.branches().expect("branch service").list().expect("list");
    let engine: BTreeMap<String, u64> = summaries
        .iter()
        .map(|summary| (summary.name().as_str().to_string(), summary.generation()))
        .collect();
    prop_assert_eq!(engine, model.active_generations());
    Ok(())
}

proptest! {
    #[test]
    fn branch_dag_matches_reference_model(ops in prop::collection::vec(any_op(), 1..30)) {
        let mut db = open_cache_database().expect("cache database opens");
        let mut model = Model::new();
        check_invariants(&mut db, &model)?;
        for op in ops {
            apply(&mut db, &mut model, op)?;
            check_invariants(&mut db, &model)?;
        }
    }
}
