use super::support::Output;

#[path = "output_cases/admin_space.rs"]
mod admin_space;
#[path = "output_cases/arrow.rs"]
mod arrow;
#[path = "output_cases/event.rs"]
mod event;
#[path = "output_cases/graph.rs"]
mod graph;
#[path = "output_cases/kv_json.rs"]
mod kv_json;
#[path = "output_cases/vector.rs"]
mod vector;

use self::admin_space::*;
use self::arrow::*;
use self::event::*;
use self::graph::*;
use self::kv_json::*;
use self::vector::*;

pub(super) use self::vector::vector_index_diagnostics_fixture;

pub(super) fn all_outputs() -> Vec<Output> {
    let mut outputs = admin_space_outputs();
    outputs.extend(branch_outputs());
    outputs.extend(kv_outputs());
    outputs.extend(json_outputs());
    outputs.extend(vector_outputs());
    outputs.extend(event_outputs());
    outputs.extend(graph_outputs());
    outputs.extend(arrow_outputs());
    outputs
}
