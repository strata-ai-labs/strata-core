# strata-wasm-next

The browser boundary: a `wasm-bindgen` adapter over the executor's serialized
command boundary — the same JSON wire the CLI, MCP server, and SDK bindings
speak. Compiles the full V1 stack (core → storage → engine → executor) to
`wasm32-unknown-unknown`.

Only **cache mode** exists in the browser: wasm has no filesystem, and cache
mode is non-durable by design (no WAL, manifest, snapshot, or lock objects),
so the database lives and dies with the page.

## JS surface

```js
import init, { StrataSession, engineVersion } from './pkg/strata_wasm_next.js';
await init();

const session = new StrataSession();               // fresh in-memory database
const out = session.execute(JSON.stringify({       // one serialized command in…
  type: 'kv_put', key: btoa('greeting'), value: btoa('hello'),
}));                                               // …one JSON envelope out
JSON.parse(out);                // {"type":"write_result","data":{...}}
                                // domain failures: {"error":{class, code, ...}}
session.setBranch('experiment'); // session scope (default branch / space)
session.close();
```

`execute` throws only on malformed command JSON — the executor's own
deserializer message names the offending field and the valid set. Executed
commands never throw; failures come back as the standard error envelope.

## Playground

`www/index.html` is a self-contained demo page: guided tours over KV, branches
+ time travel, JSON, events, vectors, and graph, plus a raw-wire REPL.

```bash
./build-web.sh        # cargo build (release, wasm32) + wasm-bindgen → www/pkg
python3 -m http.server 8080 --directory www
# open http://localhost:8080
```

`www/pkg/` is generated output — do not edit or commit it.

## wasm platform notes

- Entropy: `getrandom` (fast-hnsw → rand) and `uuid` use their JS backends,
  wired as wasm32-target deps in engine-next.
- Clocks: `std::time::{Instant, SystemTime}` trap on wasm32-unknown-unknown;
  the `time_compat` modules in storage-next / engine-next / executor-next
  route through `web-time` (performance.now() / Date.now()) on wasm only.
  Native builds re-export std — bit-identical behavior.
- No threads: cache mode never starts a background maintenance executor
  (mode policy), so maintenance runs inline on the commit path.
