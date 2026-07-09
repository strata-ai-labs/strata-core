//! Device smoke: the GT0 exit gate, run on real hardware.
//!
//! These tests need an NVIDIA GPU (Ampere+) and are `#[ignore]`d so CI
//! machines without one skip them; the acceptance gate runs them explicitly:
//!
//! ```bash
//! cargo test -p strata-gpu --test device_smoke -- --ignored
//! ```

use std::os::raw::c_void;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use strata_gpu::{
    DeviceArena, GpuContext, PinnedBuffer, PtxModule, RegionSpec, SlotAllocator, Stream,
};

/// Minimal ASCII PTX: `buf[i] += 1` for `i < n`. Targets `sm_80` (the HT-8
/// floor); the driver JIT specializes for the resident device.
const SMOKE_PTX: &str = r"
.version 7.0
.target sm_80
.address_size 64

.visible .entry add_one(
    .param .u64 param_buf,
    .param .u32 param_n
)
{
    .reg .pred  %p<2>;
    .reg .b32   %r<7>;
    .reg .b64   %rd<5>;

    ld.param.u64    %rd1, [param_buf];
    ld.param.u32    %r1, [param_n];
    mov.u32         %r2, %ctaid.x;
    mov.u32         %r3, %ntid.x;
    mov.u32         %r4, %tid.x;
    mad.lo.s32      %r5, %r2, %r3, %r4;
    setp.ge.u32     %p1, %r5, %r1;
    @%p1 bra        DONE;
    cvt.u64.u32     %rd2, %r5;
    shl.b64         %rd3, %rd2, 2;
    add.u64         %rd4, %rd1, %rd3;
    ld.global.u32   %r6, [%rd4];
    add.s32         %r6, %r6, 1;
    st.global.u32   [%rd4], %r6;
DONE:
    ret;
}
";

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn context_reports_supported_device() {
    let context = GpuContext::init().expect("driver + device present");
    let facts = context.facts();
    assert!(!facts.name.is_empty());
    assert!(facts.compute_capability >= (8, 0));
    assert!(facts.total_memory > 1 << 30, "at least 1 GiB of VRAM");
    let (free, total) = context.memory_info().expect("memory info");
    assert!(free > 0 && total >= free);
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn arena_pinned_stream_event_roundtrip() {
    let context = GpuContext::init().expect("driver + device present");
    let arena = DeviceArena::reserve(
        &context,
        &[
            RegionSpec {
                name: "pages",
                bytes: 48 << 20,
            },
            RegionSpec {
                name: "summaries",
                bytes: 8 << 20,
            },
            RegionSpec {
                name: "tables",
                bytes: 1 << 20,
            },
        ],
    )
    .expect("arena within a 12 GiB card");
    assert_eq!(arena.total_bytes(), (48 << 20) + (8 << 20) + (1 << 20));

    let pages = arena.region("pages").expect("pages region");
    arena.zero_region(pages).expect("zero fill");
    let mut slots = SlotAllocator::new(pages, 64 << 10).expect("64 KiB pages");
    let slot = slots.alloc().expect("a free slot");
    let dst = slots.slot_ptr(slot);

    // Pinned round trip through the copy stream, fenced by an event that is
    // *polled*, never blocked on — the tier's visibility mechanism.
    let stream = Stream::new(&context).expect("stream");
    let mut upload = PinnedBuffer::alloc(&context, 64 << 10).expect("pinned upload");
    let download = PinnedBuffer::alloc(&context, 64 << 10).expect("pinned download");
    // SAFETY: no copies reference these buffers yet.
    unsafe { upload.as_mut_slice() }
        .iter_mut()
        .enumerate()
        .for_each(|(index, byte)| *byte = u8::try_from(index % 251).unwrap());

    // SAFETY: pinned buffers stay alive past the fence below; dst is one
    // 64 KiB slot inside the pages region.
    unsafe {
        stream
            .copy_to_device(dst, upload.as_ptr(), upload.len())
            .expect("H2D enqueue");
        stream
            .copy_to_host(download.as_mut_ptr(), dst, download.len())
            .expect("D2H enqueue");
    }
    let fence = stream.record().expect("event record");
    while !fence.is_complete().expect("event query") {
        std::thread::yield_now();
    }
    // SAFETY: the fence proves both copies completed.
    let (sent, received) = unsafe { (upload.as_slice(), download.as_slice()) };
    assert_eq!(sent, received, "pinned H2D->D2H round trip");
}

#[test]
#[ignore = "requires an NVIDIA GPU (Ampere or newer)"]
fn ptx_kernel_and_host_callback_run_in_stream_order() {
    const COUNT: u32 = 1024;
    let context = GpuContext::init().expect("driver + device present");
    let module = PtxModule::load(&context, SMOKE_PTX, &["add_one"]).expect("PTX JIT");
    let arena = DeviceArena::reserve(
        &context,
        &[RegionSpec {
            name: "buf",
            bytes: 4096,
        }],
    )
    .expect("small arena");
    let region = arena.region("buf").expect("buf region");
    arena.zero_region(region).expect("zero fill");

    let stream = Stream::new(&context).expect("stream");
    let staging = PinnedBuffer::alloc(&context, 4096).expect("pinned staging");

    // Launch add_one twice: every u32 should become 2.
    let mut buf_param = region.base;
    let mut n_param = COUNT;
    for _ in 0..2 {
        let mut params: [*mut c_void; 2] = [
            (&raw mut buf_param).cast::<c_void>(),
            (&raw mut n_param).cast::<c_void>(),
        ];
        // SAFETY: params match add_one(u64, u32); launch copies them.
        unsafe {
            module
                .launch(
                    "add_one",
                    (COUNT / 256, 1, 1),
                    (256, 1, 1),
                    0,
                    &stream,
                    &mut params,
                )
                .expect("kernel launch");
        }
    }

    // Host callback fires after both launches in stream order.
    let flag = Arc::new(AtomicBool::new(false));
    let seen = Arc::clone(&flag);
    stream
        .launch_host_fn(Box::new(move || seen.store(true, Ordering::Release)))
        .expect("host fn enqueue");

    // SAFETY: staging outlives the synchronize below.
    unsafe {
        stream
            .copy_to_host(staging.as_mut_ptr(), region.base, staging.len())
            .expect("D2H enqueue");
    }
    let before_sync = context.sync_call_count();
    stream.synchronize().expect("drain");
    assert_eq!(
        context.sync_call_count(),
        before_sync + 1,
        "the sync counter sees exactly the one deliberate wait"
    );
    assert!(flag.load(Ordering::Acquire), "host callback ran");

    // SAFETY: the stream is drained.
    let bytes = unsafe { staging.as_slice() };
    for index in 0..COUNT as usize {
        let value = u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap());
        assert_eq!(value, 2, "element {index} incremented twice");
    }
}

#[test]
fn context_init_fails_cleanly_or_succeeds() {
    // Runs everywhere (no #[ignore]): on a GPU box it initializes; on a
    // CPU-only box it must return the typed driver-missing/no-device error,
    // never panic or link-fail.
    match GpuContext::init() {
        Ok(context) => assert!(context.facts().total_memory > 0),
        Err(error) => {
            let code = error.code();
            assert!(
                code.starts_with("unavailable.gpu.")
                    || code.starts_with("failed_precondition.gpu."),
                "unexpected error code {code}"
            );
        }
    }
}
