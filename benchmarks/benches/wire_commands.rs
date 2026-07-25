//! TCP5.2: instruction-count gates over the executor wire surface — the
//! user-felt path (JSON command parse → execute → wire output).
//!
//! Each measured body is one wire command against a warmed cache executor;
//! warm-up (executor open, seeded rows) runs in setup. KV keys and values
//! are base64 on the wire, matching the executor fixtures.

use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine as _;
use iai_callgrind::{library_benchmark, library_benchmark_group, main};
use strata_executor::{Command, Executor};

fn wire(executor: &mut Executor, json: &str) -> serde_json::Value {
    let command: Command = serde_json::from_str(json).expect("valid wire command");
    let output = executor.execute(command).expect("wire command succeeds");
    serde_json::to_value(&output).expect("serializable output")
}

fn b64(text: &str) -> String {
    BASE64.encode(text.as_bytes())
}

/// A cache executor with 64 seeded kv rows and one JSON document.
fn warmed_executor() -> Executor {
    let mut executor = Executor::open_cache().expect("open cache executor");
    for index in 0..64 {
        let key = b64(&format!("bench-{index:04}"));
        let value = b64(&format!("v{index:04}"));
        wire(
            &mut executor,
            &format!(r#"{{"type":"kv_put","key":"{key}","value":"{value}"}}"#),
        );
    }
    wire(
        &mut executor,
        r#"{"type":"json_set","key":"bench-doc","path":"$","value":{"name":"bench","count":1}}"#,
    );
    executor
}

#[library_benchmark]
#[bench::warmed(setup = warmed_executor)]
fn kv_put_wire(mut executor: Executor) -> Executor {
    let key = b64("bench-write");
    let value = b64("payload");
    wire(
        &mut executor,
        &format!(r#"{{"type":"kv_put","key":"{key}","value":"{value}"}}"#),
    );
    executor
}

#[library_benchmark]
#[bench::warmed(setup = warmed_executor)]
fn kv_get_wire(mut executor: Executor) -> Executor {
    let key = b64("bench-0032");
    wire(&mut executor, &format!(r#"{{"type":"kv_get","key":"{key}"}}"#));
    executor
}

#[library_benchmark]
#[bench::warmed(setup = warmed_executor)]
fn kv_scan_wire(mut executor: Executor) -> Executor {
    let start = b64("bench-");
    wire(
        &mut executor,
        &format!(r#"{{"type":"kv_scan","start":"{start}","limit":64}}"#),
    );
    executor
}

#[library_benchmark]
#[bench::warmed(setup = warmed_executor)]
fn json_set_wire(mut executor: Executor) -> Executor {
    wire(
        &mut executor,
        r#"{"type":"json_set","key":"bench-doc-write","path":"$","value":{"name":"written","count":2}}"#,
    );
    executor
}

#[library_benchmark]
#[bench::warmed(setup = warmed_executor)]
fn json_get_wire(mut executor: Executor) -> Executor {
    wire(
        &mut executor,
        r#"{"type":"json_get","key":"bench-doc","path":"$"}"#,
    );
    executor
}

library_benchmark_group!(
    name = wire_commands;
    benchmarks = kv_put_wire, kv_get_wire, kv_scan_wire, json_set_wire, json_get_wire
);
main!(library_benchmark_groups = wire_commands);
