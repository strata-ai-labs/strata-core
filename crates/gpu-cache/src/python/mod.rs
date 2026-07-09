//! The Python seam (design §12): `strata_tier`, a `PyO3` extension module
//! over the concrete CUDA tier with an engine-backed store of record.
//!
//! Build: `cargo build -p strata-gpu-cache --release --features python`,
//! then expose `libstrata_gpu_cache.so` as `strata_tier.so` on the Python
//! path (or package with maturin).
//!
//! The decode-loop calls are host-async; `Selection` tensors implement
//! `__dlpack__(stream=...)` by making the *consumer's* stream wait on the
//! producing fence — cross-stream ordering with zero host syncs, verified
//! by the exported `sync_calls()` counter.

mod dlpack;

use pyo3::exceptions::{PyRuntimeError, PyValueError};
use pyo3::prelude::*;
use pyo3::types::PyDict;

use crate::device::cuda_backend::SelectionAddresses;
use crate::tier::backend::TagFilter;
use crate::tier::engine_store::EnginePageStore;
use crate::tier::page_table::PageId;
use crate::tier::store::PageBlob;
use crate::tier::tier::{Tier as RustTier, TierConfig};
use crate::tier::CudaBackend;
use crate::GpuError;

type ConcreteTier = RustTier<CudaBackend, EnginePageStore>;

#[allow(
    clippy::needless_pass_by_value,
    reason = "conversion sink for map_err(gpu_err) on owned errors"
)]
fn gpu_err(error: GpuError) -> PyErr {
    PyRuntimeError::new_err(error.to_string())
}

/// The GPU cache tier over a durable Strata database.
#[pyclass]
struct Tier {
    inner: ConcreteTier,
    device_id: i32,
}

#[pymethods]
impl Tier {
    /// Opens the tier: a CUDA device context plus a durable-local Strata
    /// database at `path` (created if absent).
    ///
    /// `selection_ptx` registers a replacement selection module (Moho's
    /// kernels) in place of the baseline — it must define every baseline
    /// entry point with the baseline ABI; a missing entry fails here, not
    /// at the first decode step.
    ///
    /// `profile=True` records a timestamp event after every selection
    /// stage, so `Selection.timings()` carries the per-stage breakdown —
    /// benchmarking runs, not the steady-state decode loop.
    #[new]
    #[pyo3(signature = (path, space, page_bytes, summary_bytes, page_slots,
                        adjacency_degree = 32, promotion_batch = 8,
                        write_behind_batch = 8, write_backlog_cap = 64,
                        selection_ptx = None, profile = false))]
    #[allow(clippy::too_many_arguments)]
    fn new(
        path: &str,
        space: &str,
        page_bytes: u64,
        summary_bytes: u64,
        page_slots: u32,
        adjacency_degree: u16,
        promotion_batch: usize,
        write_behind_batch: usize,
        write_backlog_cap: usize,
        selection_ptx: Option<String>,
        profile: bool,
    ) -> PyResult<Self> {
        let staging = usize::try_from(page_bytes)
            .map_err(|_| PyValueError::new_err("page_bytes exceeds the address width"))?;
        let mut backend = CudaBackend::new(staging).map_err(gpu_err)?;
        if let Some(ptx) = selection_ptx {
            backend.register_selection_ptx(ptx).map_err(gpu_err)?;
        }
        if profile {
            backend.enable_profiling();
        }
        let device_id = backend.device_ordinal();
        let store = EnginePageStore::open(path, space).map_err(gpu_err)?;
        let inner = RustTier::open(
            backend,
            store,
            TierConfig {
                page_bytes,
                summary_bytes,
                page_slots,
                promotion_batch,
                adjacency_degree,
                write_behind_batch,
                write_backlog_cap,
            },
        )
        .map_err(gpu_err)?;
        Ok(Self { inner, device_id })
    }

    /// Appends a page: hot immediately, durable at the next batch commit or
    /// `flush()`. Returns the page id.
    #[pyo3(signature = (bytes, summary, tags = None, edges = None))]
    fn append(
        &mut self,
        bytes: &[u8],
        summary: &[u8],
        tags: Option<[u64; 4]>,
        edges: Option<Vec<u64>>,
    ) -> PyResult<u64> {
        let blob = PageBlob {
            bytes: bytes.to_vec(),
            summary: summary.to_vec(),
            tags: tags.unwrap_or_default(),
            edges: edges.unwrap_or_default().into_iter().map(PageId).collect(),
        };
        self.inner.append(&blob).map(|id| id.0).map_err(gpu_err)
    }

    /// Requests residency. Returns `True` on a hit.
    fn request(&mut self, page_id: u64, priority: u32) -> bool {
        matches!(
            self.inner.request(PageId(page_id), priority),
            crate::tier::tier::RequestOutcome::Hit
        )
    }

    /// One maintenance round: durability batches, gate sweeps, promotions.
    fn maintain(&mut self) {
        self.inner.maintain();
    }

    /// Begins a decode step (installs the epoch fence).
    fn step_begin(&mut self) -> PyResult<u64> {
        self.inner.step_begin().map_err(gpu_err)
    }

    /// Drains the write-behind queue; returns the durability point as a
    /// `(version, timestamp_micros)` tuple, or `None` if nothing was ever
    /// committed.
    fn flush(&mut self) -> PyResult<Option<(u64, u64)>> {
        self.inner
            .flush()
            .map(|receipt| receipt.map(|r| (r.version, r.timestamp)))
            .map_err(gpu_err)
    }

    /// True when the page is resident and selectable.
    fn is_selectable(&self, page_id: u64) -> bool {
        self.inner.is_selectable(PageId(page_id))
    }

    /// Enqueues selection (+ contiguous materialization) on the device and
    /// returns the handle. Host-async: nothing here blocks.
    #[pyo3(signature = (query, k, expand = None, filter_index = None, filter_value = None))]
    #[allow(
        clippy::needless_pass_by_value,
        reason = "pyo3 extracts owned Vec from Python lists"
    )]
    fn topk(
        slf: Bound<'_, Self>,
        query: Vec<f32>,
        k: u16,
        expand: Option<u16>,
        filter_index: Option<u8>,
        filter_value: Option<u64>,
    ) -> PyResult<Selection> {
        let filter = match (filter_index, filter_value) {
            (Some(index), Some(value)) => Some(TagFilter { index, value }),
            (None, None) => None,
            _ => {
                return Err(PyValueError::new_err(
                    "filter_index and filter_value must be given together",
                ))
            }
        };
        let (addresses, device_id) = {
            let mut tier = slf.borrow_mut();
            tier.inner
                .topk_enqueue(&query, k, expand, filter)
                .map_err(gpu_err)?;
            tier.inner.materialize_enqueue().map_err(gpu_err)?;
            let addresses = tier
                .inner
                .backend()
                .selection_addresses()
                .map_err(gpu_err)?;
            (addresses, tier.device_id)
        };
        Ok(Selection {
            tier: slf.unbind(),
            addresses,
            device_id,
        })
    }

    /// The whole device page pool as `uint8 [slots, page_bytes]` — the
    /// zero-copy path for fused kernels that gather via `block_table()`
    /// themselves, skipping materialization. A slot's contents are stable
    /// only under the epoch contract: read only slots named by the current
    /// step's selection.
    fn pool(slf: Bound<'_, Self>) -> PyResult<DeviceTensor> {
        let (address, device_id) = {
            let tier = slf.borrow();
            let address = tier.inner.backend().pool_address().map_err(gpu_err)?;
            (address, tier.device_id)
        };
        let page_bytes = i64::try_from(address.page_bytes)
            .map_err(|_| PyValueError::new_err("page_bytes exceeds i64"))?;
        Ok(DeviceTensor {
            tier: slf.unbind(),
            ptr: address.base,
            shape: vec![i64::from(address.slots), page_bytes],
            dtype: dlpack::DType::Uint8,
            device_id,
        })
    }

    /// Non-blocking readiness probe for the most recent selection.
    fn selection_ready(&self) -> bool {
        self.inner.selection_ready()
    }

    /// Blocking convenience (test/demo path): page ids of the most recent
    /// selection, best first. Documented sync.
    fn selection_page_ids(&mut self) -> PyResult<Vec<u64>> {
        use crate::tier::backend::DeviceBackend;
        let readback = self.inner.backend_mut().read_topk().map_err(gpu_err)?;
        Ok(readback
            .selected
            .iter()
            .filter_map(|(slot, _)| self.inner.page_of_slot(*slot).map(|id| id.0))
            .collect())
    }

    /// HT-9 counters as a dict.
    fn stats<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyDict>> {
        let stats = self.inner.stats();
        let dict = PyDict::new(py);
        dict.set_item("hits", stats.hits)?;
        dict.set_item("queued", stats.queued)?;
        dict.set_item("promotions_started", stats.promotions_started)?;
        dict.set_item("promotions_completed", stats.promotions_completed)?;
        dict.set_item("promotion_failures", stats.promotion_failures)?;
        dict.set_item("store_misses", stats.store_misses)?;
        dict.set_item("degraded_placements", stats.degraded_placements)?;
        dict.set_item("evictions", stats.evictions)?;
        dict.set_item("slots_reused", stats.slots_reused)?;
        dict.set_item("write_commits", stats.write_commits)?;
        dict.set_item("write_commit_failures", stats.write_commit_failures)?;
        Ok(dict)
    }

    /// Host synchronizations issued by the device runtime since open — the
    /// zero-implicit-sync rule, measured.
    fn sync_calls(&self) -> u64 {
        self.inner.backend().context().sync_call_count()
    }

    /// Entries awaiting their durable commit.
    fn write_backlog(&self) -> usize {
        self.inner.write_backlog()
    }
}

/// One selection's device-resident results.
#[pyclass]
struct Selection {
    tier: Py<Tier>,
    addresses: SelectionAddresses,
    device_id: i32,
}

#[pymethods]
impl Selection {
    /// The block table: `int32 [k]` slot indices on the device (`-1` pads)
    /// — the paged-attention convention for custom kernels.
    fn block_table(&self, py: Python<'_>) -> DeviceTensor {
        self.tensor(
            py,
            self.addresses.slots_ptr,
            vec![i64::from(self.addresses.k)],
            dlpack::DType::Int32,
        )
    }

    /// Selection scores: `float32 [k]` on the device.
    fn scores(&self, py: Python<'_>) -> DeviceTensor {
        self.tensor(
            py,
            self.addresses.scores_ptr,
            vec![i64::from(self.addresses.k)],
            dlpack::DType::Float32,
        )
    }

    /// The materialized pages: `uint8 [k, page_bytes]` on the device,
    /// selection-ordered, pad rows zeroed — consumable by stock attention
    /// with no custom kernels.
    fn pages(&self, py: Python<'_>) -> PyResult<DeviceTensor> {
        let shape = vec![
            i64::from(self.addresses.k),
            i64::try_from(self.addresses.page_bytes)
                .map_err(|_| PyValueError::new_err("page_bytes exceeds i64"))?,
        ];
        Ok(self.tensor(
            py,
            self.addresses.materialized_ptr,
            shape,
            dlpack::DType::Uint8,
        ))
    }

    /// Non-blocking readiness probe.
    fn ready(&self, py: Python<'_>) -> bool {
        self.tier.borrow(py).inner.selection_ready()
    }

    /// Device-time measurements for this selection (the kernel-benchmarking
    /// endpoint), or `None` while the pipeline is still in flight.
    /// Non-blocking: CUDA event timestamps are probed, never waited on.
    ///
    /// Keys: `selection_us` (whole pipeline), `materialize_us` (the page
    /// gather; `None` until it completes), and — when the tier was opened
    /// with `profile=True` — `stage_query_us`, `score_us`, `block_topk_us`,
    /// `merge_us`, plus `seed_us`/`expand_us` when expansion was requested.
    /// Values describe the most recent `topk` and reset at the next one —
    /// read them before enqueuing the next step.
    fn timings<'py>(&self, py: Python<'py>) -> PyResult<Option<Bound<'py, PyDict>>> {
        let tier = self.tier.borrow(py);
        let Some(timings) = tier
            .inner
            .backend()
            .last_selection_timings()
            .map_err(gpu_err)?
        else {
            return Ok(None);
        };
        let dict = PyDict::new(py);
        dict.set_item("selection_us", timings.selection_us)?;
        dict.set_item("materialize_us", timings.materialize_us)?;
        for (key, value) in [
            ("stage_query_us", timings.stage_query_us),
            ("score_us", timings.score_us),
            ("block_topk_us", timings.block_topk_us),
            ("merge_us", timings.merge_us),
            ("seed_us", timings.seed_us),
            ("expand_us", timings.expand_us),
        ] {
            if let Some(micros) = value {
                dict.set_item(key, micros)?;
            }
        }
        Ok(Some(dict))
    }
}

impl Selection {
    fn tensor(
        &self,
        py: Python<'_>,
        ptr: u64,
        shape: Vec<i64>,
        dtype: dlpack::DType,
    ) -> DeviceTensor {
        DeviceTensor {
            tier: self.tier.clone_ref(py),
            ptr,
            shape,
            dtype,
            device_id: self.device_id,
        }
    }
}

/// A device tensor view implementing the `DLPack` producer protocol.
#[pyclass]
struct DeviceTensor {
    tier: Py<Tier>,
    ptr: u64,
    shape: Vec<i64>,
    dtype: dlpack::DType,
    device_id: i32,
}

#[pymethods]
impl DeviceTensor {
    /// `DLPack` device: `(kDLCUDA, ordinal)`.
    fn __dlpack_device__(&self) -> (i32, i32) {
        (2, self.device_id)
    }

    /// `DLPack` capsule. When the consumer passes its stream (torch does),
    /// that stream is made to wait on the producing fence — correct
    /// ordering with zero host synchronization.
    #[pyo3(signature = (stream = None, *_args, **_kwargs))]
    fn __dlpack__(
        &self,
        py: Python<'_>,
        stream: Option<u64>,
        _args: &Bound<'_, pyo3::types::PyTuple>,
        _kwargs: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<PyObject> {
        if let Some(raw_stream) = stream {
            let tier = self.tier.borrow(py);
            if let Some(fence) = tier.inner.selection_fence() {
                // SAFETY: torch passes a live stream handle for this device
                // per the DLPack protocol (1/2 are the CUDA default-stream
                // sentinels, which cuStreamWaitEvent accepts).
                #[allow(unsafe_code)]
                unsafe {
                    tier.inner
                        .backend()
                        .wait_external_stream(raw_stream, fence)
                        .map_err(gpu_err)?;
                }
            }
        }
        let keepalive: Py<PyAny> = self.tier.clone_ref(py).into_any();
        dlpack::capsule(
            py,
            self.ptr,
            &self.shape,
            self.dtype,
            self.device_id,
            keepalive,
        )
    }
}

/// The `strata_tier` extension module.
#[pymodule]
#[pyo3(name = "strata_tier")]
fn strata_tier(module: &Bound<'_, PyModule>) -> PyResult<()> {
    module.add_class::<Tier>()?;
    module.add_class::<Selection>()?;
    module.add_class::<DeviceTensor>()?;
    Ok(())
}
