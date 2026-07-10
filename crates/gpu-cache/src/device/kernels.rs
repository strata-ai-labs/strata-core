//! Baseline selection kernels (GT3), embedded as PTX.
//!
//! Deliberately naive — correctness-first references that Moho's fused
//! kernels replace by registration, not by forking the tier. ASCII-only
//! PTX targeting `sm_80` (the HT-8 floor); the driver JIT specializes for
//! the resident device.
//!
//! - `score_slots` — one thread per slot: validity + tag-filter mask, then
//!   an f32 dot product of the query against the slot's summary. Masked
//!   slots score `f32::MIN` so selection never picks them.
//! - `block_topk` + `merge_topk` — exact hierarchical top-k: every block
//!   bitonic-sorts its 256-slot slice by (score desc, slot asc) and emits
//!   its local top-k (any global top-k element is necessarily in its
//!   window's top-k, so each round is exact); `merge_topk` then sorts
//!   256-candidate windows round by round until one block writes the final
//!   selection. The comparator is a total order, so ties match the
//!   host-sim oracle bitwise. No k-sequential passes anywhere.
//! - `seed_bitmap` — marks the selected slots in the dedup bitmap before
//!   expansion.
//! - `expand` — one thread per (selection, edge) pair: bounded one-hop
//!   adjacency walk with atomic bitmap dedup and an atomic output cursor.

/// Kernel entry names, resolved eagerly at module load. This list is the
/// registration contract: a replacement module (Moho) must define every
/// name with the baseline ABI. Re-exported as
/// `strata_gpu_cache::SELECTION_KERNEL_NAMES`.
pub const KERNELS: &[&str] = &[
    "score_slots",
    "block_topk",
    "merge_topk",
    "seed_bitmap",
    "expand",
    "gather_pages",
];

/// The baseline PTX module source. Re-exported as
/// `strata_gpu_cache::BASELINE_SELECTION_PTX` so replacement modules can
/// start from (or fall back to) the reference implementation.
pub const SELECTION_PTX: &str = r"
.version 7.0
.target sm_80
.address_size 64

// scores[slot] = valid && filter ? dot(query, summary[slot]) : f32::MIN
.visible .entry score_slots(
    .param .u64 p_scores,
    .param .u64 p_summaries,
    .param .u64 p_valid,
    .param .u64 p_tags,
    .param .u64 p_query,
    .param .u32 p_capacity,
    .param .u32 p_dim,
    .param .u32 p_findex,
    .param .u64 p_fvalue
)
{
    .reg .pred  %p<6>;
    .reg .b16   %h<2>;
    .reg .b32   %r<12>;
    .reg .b64   %rd<24>;
    .reg .f32   %f<6>;

    ld.param.u64    %rd1, [p_scores];
    ld.param.u64    %rd2, [p_summaries];
    ld.param.u64    %rd3, [p_valid];
    ld.param.u64    %rd4, [p_tags];
    ld.param.u64    %rd5, [p_query];
    ld.param.u32    %r1, [p_capacity];
    ld.param.u32    %r2, [p_dim];
    ld.param.u32    %r3, [p_findex];
    ld.param.u64    %rd6, [p_fvalue];

    mov.u32         %r4, %ctaid.x;
    mov.u32         %r5, %ntid.x;
    mov.u32         %r6, %tid.x;
    mad.lo.s32      %r7, %r4, %r5, %r6;
    setp.ge.u32     %p1, %r7, %r1;
    @%p1 bra        DONE;

    cvt.u64.u32     %rd7, %r7;
    shl.b64         %rd8, %rd7, 2;
    add.u64         %rd9, %rd1, %rd8;       // &scores[slot]

    // Validity byte.
    add.u64         %rd10, %rd3, %rd7;
    ld.global.u8    %h1, [%rd10];
    cvt.u32.u16     %r8, %h1;
    setp.eq.u32     %p2, %r8, 0;
    @%p2 bra        MASKED;

    // Optional tag filter: p_findex is 0..3, or 0xFFFFFFFF for none.
    setp.eq.u32     %p3, %r3, 0xFFFFFFFF;
    @%p3 bra        SCORE;
    shl.b64         %rd11, %rd7, 5;         // slot * 32
    add.u64         %rd12, %rd4, %rd11;
    cvt.u64.u32     %rd13, %r3;
    shl.b64         %rd13, %rd13, 3;        // index * 8
    add.u64         %rd12, %rd12, %rd13;
    ld.global.u64   %rd14, [%rd12];
    setp.ne.u64     %p4, %rd14, %rd6;
    @%p4 bra        MASKED;

SCORE:
    cvt.u64.u32     %rd15, %r2;
    shl.b64         %rd16, %rd15, 2;        // dim * 4
    mul.lo.u64      %rd17, %rd7, %rd16;
    add.u64         %rd18, %rd2, %rd17;     // &summary[slot][0]
    mov.u64         %rd19, %rd5;            // &query[0]
    mov.f32         %f1, 0f00000000;
    mov.u32         %r9, 0;
LOOP:
    setp.ge.u32     %p5, %r9, %r2;
    @%p5 bra        STORE;
    ld.global.f32   %f2, [%rd18];
    ld.global.f32   %f3, [%rd19];
    fma.rn.f32      %f1, %f2, %f3, %f1;
    add.u64         %rd18, %rd18, 4;
    add.u64         %rd19, %rd19, 4;
    add.u32         %r9, %r9, 1;
    bra             LOOP;

STORE:
    st.global.f32   [%rd9], %f1;
    ret;

MASKED:
    mov.f32         %f4, 0fFF7FFFFF;        // f32::MIN
    st.global.f32   [%rd9], %f4;
DONE:
    ret;
}

// Stage 1: each 256-thread block bitonic-sorts its 256-slot slice by
// (score desc, slot asc) in shared memory -- 36 compare-exchange substages,
// one barrier each -- and threads 0..k emit the block's best (score, slot)
// candidates. Out-of-range lanes and exhausted entries carry
// f32::MIN / 0xFFFFFFFF.
.visible .entry block_topk(
    .param .u64 p_scores,
    .param .u32 p_capacity,
    .param .u32 p_k,
    .param .u64 p_cand_scores,
    .param .u64 p_cand_slots
)
{
    .reg .pred  %p<12>;
    .reg .b32   %r<20>;
    .reg .b64   %rd<24>;
    .reg .f32   %f<8>;
    .shared .align 4 .b8 s_score[1024];
    .shared .align 4 .b8 s_slot[1024];

    ld.param.u64    %rd1, [p_scores];
    ld.param.u32    %r1, [p_capacity];
    ld.param.u32    %r2, [p_k];
    ld.param.u64    %rd2, [p_cand_scores];
    ld.param.u64    %rd3, [p_cand_slots];

    mov.u32         %r3, %tid.x;
    mov.u32         %r4, %ctaid.x;
    shl.b32         %r5, %r4, 8;            // block base slot
    add.u32         %r6, %r5, %r3;          // this thread's global slot

    cvt.u64.u32     %rd4, %r3;
    shl.b64         %rd5, %rd4, 2;
    mov.u64         %rd6, s_score;
    add.u64         %rd7, %rd6, %rd5;       // &s_score[tid]
    mov.u64         %rd8, s_slot;
    add.u64         %rd9, %rd8, %rd5;       // &s_slot[tid]

    // Load the slice; out-of-range lanes hold (f32::MIN, 0xFFFFFFFF).
    mov.f32         %f1, 0fFF7FFFFF;
    mov.u32         %r7, 0xFFFFFFFF;
    setp.ge.u32     %p1, %r6, %r1;
    @%p1 bra        BLOADED;
    cvt.u64.u32     %rd10, %r6;
    shl.b64         %rd11, %rd10, 2;
    add.u64         %rd12, %rd1, %rd11;
    ld.global.f32   %f1, [%rd12];
    mov.u32         %r7, %r6;
BLOADED:
    st.shared.f32   [%rd7], %f1;
    st.shared.u32   [%rd9], %r7;
    bar.sync        0;

    // Bitonic sort, 256 elements: the lower lane of each pair owns the
    // compare-exchange, so one barrier per substage suffices.
    mov.u32         %r8, 2;                 // size
BSIZE:
    setp.gt.u32     %p1, %r8, 256;
    @%p1 bra        BSORTED;
    shr.u32         %r9, %r8, 1;            // stride
BSTRIDE:
    setp.eq.u32     %p1, %r9, 0;
    @%p1 bra        BSIZENEXT;
    and.b32         %r10, %r3, %r9;
    setp.ne.u32     %p1, %r10, 0;
    @%p1 bra        BSKIP;                  // upper lane idles this substage
    add.u32         %r11, %r3, %r9;         // partner = tid + stride
    cvt.u64.u32     %rd13, %r11;
    shl.b64         %rd14, %rd13, 2;
    mov.u64         %rd15, s_score;
    add.u64         %rd16, %rd15, %rd14;    // &s_score[partner]
    mov.u64         %rd17, s_slot;
    add.u64         %rd18, %rd17, %rd14;    // &s_slot[partner]
    ld.shared.f32   %f2, [%rd7];
    ld.shared.u32   %r12, [%rd9];
    ld.shared.f32   %f3, [%rd16];
    ld.shared.u32   %r13, [%rd18];
    // a-before-b: higher score first, lower slot on ties (total order).
    setp.gt.f32     %p2, %f2, %f3;
    setp.eq.f32     %p3, %f2, %f3;
    setp.lt.u32     %p4, %r12, %r13;
    and.pred        %p5, %p3, %p4;
    or.pred         %p6, %p2, %p5;
    // Ascending run iff (tid & size) == 0; swap when order disagrees.
    and.b32         %r14, %r3, %r8;
    setp.eq.u32     %p7, %r14, 0;
    xor.pred        %p8, %p7, %p6;
    @!%p8 bra       BSKIP;
    st.shared.f32   [%rd7], %f3;
    st.shared.u32   [%rd9], %r13;
    st.shared.f32   [%rd16], %f2;
    st.shared.u32   [%rd18], %r12;
BSKIP:
    bar.sync        0;
    shr.u32         %r9, %r9, 1;
    bra             BSTRIDE;
BSIZENEXT:
    shl.b32         %r8, %r8, 1;
    bra             BSIZE;
BSORTED:

    // Threads 0..k emit the candidates at block*k + tid; pads (score still
    // f32::MIN) carry slot 0xFFFFFFFF.
    setp.ge.u32     %p1, %r3, %r2;
    @%p1 bra        BDONE;
    ld.shared.f32   %f4, [%rd7];
    ld.shared.u32   %r15, [%rd9];
    mov.f32         %f5, 0fFF7FFFFF;
    setp.eq.f32     %p2, %f4, %f5;
    @%p2 mov.u32    %r15, 0xFFFFFFFF;
    mad.lo.s32      %r16, %r4, %r2, %r3;
    cvt.u64.u32     %rd19, %r16;
    shl.b64         %rd20, %rd19, 2;
    add.u64         %rd21, %rd2, %rd20;
    st.global.f32   [%rd21], %f4;
    add.u64         %rd21, %rd3, %rd20;
    st.global.u32   [%rd21], %r15;
BDONE:
    ret;
}

// Stage 2+: each block bitonic-sorts a 256-candidate window of the
// (score, slot) list and emits its top k -- applied round by round (the
// grid shrinks ~4x per round at k=64) until a single block writes the
// final selection. Same comparator as stage 1, so ties stay deterministic.
.visible .entry merge_topk(
    .param .u64 p_src_scores,
    .param .u64 p_src_slots,
    .param .u32 p_count,
    .param .u32 p_k,
    .param .u64 p_out_slots,
    .param .u64 p_out_scores
)
{
    .reg .pred  %p<12>;
    .reg .b32   %r<20>;
    .reg .b64   %rd<26>;
    .reg .f32   %f<8>;
    .shared .align 4 .b8 m_score[1024];
    .shared .align 4 .b8 m_slot[1024];

    ld.param.u64    %rd1, [p_src_scores];
    ld.param.u64    %rd2, [p_src_slots];
    ld.param.u32    %r1, [p_count];
    ld.param.u32    %r2, [p_k];
    ld.param.u64    %rd3, [p_out_slots];
    ld.param.u64    %rd4, [p_out_scores];

    mov.u32         %r3, %tid.x;
    mov.u32         %r4, %ctaid.x;
    shl.b32         %r5, %r4, 8;
    add.u32         %r6, %r5, %r3;          // this thread's candidate index

    cvt.u64.u32     %rd5, %r3;
    shl.b64         %rd6, %rd5, 2;
    mov.u64         %rd7, m_score;
    add.u64         %rd8, %rd7, %rd6;       // &m_score[tid]
    mov.u64         %rd9, m_slot;
    add.u64         %rd10, %rd9, %rd6;      // &m_slot[tid]

    // Load the window; out-of-range lanes hold (f32::MIN, 0xFFFFFFFF).
    mov.f32         %f1, 0fFF7FFFFF;
    mov.u32         %r7, 0xFFFFFFFF;
    setp.ge.u32     %p1, %r6, %r1;
    @%p1 bra        MLOADED;
    cvt.u64.u32     %rd11, %r6;
    shl.b64         %rd12, %rd11, 2;
    add.u64         %rd13, %rd1, %rd12;
    ld.global.f32   %f1, [%rd13];
    add.u64         %rd14, %rd2, %rd12;
    ld.global.u32   %r7, [%rd14];
MLOADED:
    st.shared.f32   [%rd8], %f1;
    st.shared.u32   [%rd10], %r7;
    bar.sync        0;

    mov.u32         %r8, 2;                 // size
MSIZE:
    setp.gt.u32     %p1, %r8, 256;
    @%p1 bra        MSORTED;
    shr.u32         %r9, %r8, 1;            // stride
MSTRIDE:
    setp.eq.u32     %p1, %r9, 0;
    @%p1 bra        MSIZENEXT;
    and.b32         %r10, %r3, %r9;
    setp.ne.u32     %p1, %r10, 0;
    @%p1 bra        MSKIP;
    add.u32         %r11, %r3, %r9;
    cvt.u64.u32     %rd15, %r11;
    shl.b64         %rd16, %rd15, 2;
    mov.u64         %rd17, m_score;
    add.u64         %rd18, %rd17, %rd16;
    mov.u64         %rd19, m_slot;
    add.u64         %rd20, %rd19, %rd16;
    ld.shared.f32   %f2, [%rd8];
    ld.shared.u32   %r12, [%rd10];
    ld.shared.f32   %f3, [%rd18];
    ld.shared.u32   %r13, [%rd20];
    setp.gt.f32     %p2, %f2, %f3;
    setp.eq.f32     %p3, %f2, %f3;
    setp.lt.u32     %p4, %r12, %r13;
    and.pred        %p5, %p3, %p4;
    or.pred         %p6, %p2, %p5;
    and.b32         %r14, %r3, %r8;
    setp.eq.u32     %p7, %r14, 0;
    xor.pred        %p8, %p7, %p6;
    @!%p8 bra       MSKIP;
    st.shared.f32   [%rd8], %f3;
    st.shared.u32   [%rd10], %r13;
    st.shared.f32   [%rd18], %f2;
    st.shared.u32   [%rd20], %r12;
MSKIP:
    bar.sync        0;
    shr.u32         %r9, %r9, 1;
    bra             MSTRIDE;
MSIZENEXT:
    shl.b32         %r8, %r8, 1;
    bra             MSIZE;
MSORTED:

    setp.ge.u32     %p1, %r3, %r2;
    @%p1 bra        MDONE;
    ld.shared.f32   %f4, [%rd8];
    ld.shared.u32   %r15, [%rd10];
    mov.f32         %f5, 0fFF7FFFFF;
    setp.eq.f32     %p2, %f4, %f5;
    @%p2 mov.u32    %r15, 0xFFFFFFFF;
    mad.lo.s32      %r16, %r4, %r2, %r3;
    cvt.u64.u32     %rd21, %r16;
    shl.b64         %rd22, %rd21, 2;
    add.u64         %rd23, %rd3, %rd22;
    st.global.u32   [%rd23], %r15;
    add.u64         %rd23, %rd4, %rd22;
    st.global.f32   [%rd23], %f4;
MDONE:
    ret;
}

// Marks each selected slot in the dedup bitmap.
.visible .entry seed_bitmap(
    .param .u64 p_slots,
    .param .u32 p_k,
    .param .u64 p_bitmap
)
{
    .reg .pred  %p<3>;
    .reg .b32   %r<10>;
    .reg .b64   %rd<8>;

    ld.param.u64    %rd1, [p_slots];
    ld.param.u32    %r1, [p_k];
    ld.param.u64    %rd2, [p_bitmap];
    mov.u32         %r2, %tid.x;
    setp.ge.u32     %p1, %r2, %r1;
    @%p1 bra        DONE;
    cvt.u64.u32     %rd3, %r2;
    shl.b64         %rd4, %rd3, 2;
    add.u64         %rd5, %rd1, %rd4;
    ld.global.u32   %r3, [%rd5];
    setp.eq.u32     %p2, %r3, 0xFFFFFFFF;
    @%p2 bra        DONE;
    shr.u32         %r4, %r3, 5;
    and.b32         %r5, %r3, 31;
    mov.u32         %r6, 1;
    shl.b32         %r7, %r6, %r5;
    cvt.u64.u32     %rd6, %r4;
    shl.b64         %rd7, %rd6, 2;
    add.u64         %rd7, %rd2, %rd7;
    atom.global.or.b32 %r8, [%rd7], %r7;
DONE:
    ret;
}

// One thread per (selection, edge): bounded one-hop expansion with atomic
// bitmap dedup and an atomic output cursor (over-reservation past the
// budget is clamped by the host).
.visible .entry expand(
    .param .u64 p_slots,
    .param .u32 p_k,
    .param .u64 p_adj,
    .param .u32 p_degree,
    .param .u64 p_valid,
    .param .u64 p_bitmap,
    .param .u64 p_out,
    .param .u64 p_cursor,
    .param .u32 p_budget
)
{
    .reg .pred  %p<8>;
    .reg .b16   %h<2>;
    .reg .b32   %r<20>;
    .reg .b64   %rd<20>;

    ld.param.u64    %rd1, [p_slots];
    ld.param.u32    %r1, [p_k];
    ld.param.u64    %rd2, [p_adj];
    ld.param.u32    %r2, [p_degree];
    ld.param.u64    %rd3, [p_valid];
    ld.param.u64    %rd4, [p_bitmap];
    ld.param.u64    %rd5, [p_out];
    ld.param.u64    %rd6, [p_cursor];
    ld.param.u32    %r3, [p_budget];

    mov.u32         %r4, %ctaid.x;
    mov.u32         %r5, %ntid.x;
    mov.u32         %r6, %tid.x;
    mad.lo.s32      %r7, %r4, %r5, %r6;
    mul.lo.u32      %r8, %r1, %r2;
    setp.ge.u32     %p1, %r7, %r8;
    @%p1 bra        DONE;

    div.u32         %r9, %r7, %r2;          // selection index
    rem.u32         %r10, %r7, %r2;         // edge index

    cvt.u64.u32     %rd7, %r9;
    shl.b64         %rd8, %rd7, 2;
    add.u64         %rd9, %rd1, %rd8;
    ld.global.u32   %r11, [%rd9];           // source slot
    setp.eq.u32     %p2, %r11, 0xFFFFFFFF;
    @%p2 bra        DONE;

    mul.lo.u32      %r12, %r11, %r2;
    add.u32         %r12, %r12, %r10;
    cvt.u64.u32     %rd10, %r12;
    shl.b64         %rd11, %rd10, 2;
    add.u64         %rd12, %rd2, %rd11;
    ld.global.u32   %r13, [%rd12];          // neighbor slot
    setp.eq.u32     %p3, %r13, 0xFFFFFFFF;
    @%p3 bra        DONE;

    cvt.u64.u32     %rd13, %r13;
    add.u64         %rd14, %rd3, %rd13;
    ld.global.u8    %h1, [%rd14];
    cvt.u32.u16     %r14, %h1;
    setp.eq.u32     %p4, %r14, 0;
    @%p4 bra        DONE;

    shr.u32         %r15, %r13, 5;
    and.b32         %r16, %r13, 31;
    mov.u32         %r17, 1;
    shl.b32         %r17, %r17, %r16;
    cvt.u64.u32     %rd15, %r15;
    shl.b64         %rd16, %rd15, 2;
    add.u64         %rd16, %rd4, %rd16;
    atom.global.or.b32 %r18, [%rd16], %r17;
    and.b32         %r18, %r18, %r17;
    setp.ne.u32     %p5, %r18, 0;
    @%p5 bra        DONE;                   // already present

    mov.u32         %r19, 1;
    atom.global.add.u32 %r19, [%rd6], %r19;
    setp.ge.u32     %p6, %r19, %r3;
    @%p6 bra        DONE;                   // over budget: drop
    cvt.u64.u32     %rd17, %r19;
    shl.b64         %rd18, %rd17, 2;
    add.u64         %rd19, %rd5, %rd18;
    st.global.u32   [%rd19], %r13;
DONE:
    ret;
}

// Gathers the selected pages contiguously: out[i] = pool[sel[i]], one u32
// word per thread; pad selections (0xFFFFFFFF) zero-fill their row.
.visible .entry gather_pages(
    .param .u64 p_slots,
    .param .u32 p_k,
    .param .u64 p_pool,
    .param .u32 p_words,
    .param .u64 p_out
)
{
    .reg .pred  %p<4>;
    .reg .b32   %r<14>;
    .reg .b64   %rd<14>;

    ld.param.u64    %rd1, [p_slots];
    ld.param.u32    %r1, [p_k];
    ld.param.u64    %rd2, [p_pool];
    ld.param.u32    %r2, [p_words];
    ld.param.u64    %rd3, [p_out];

    mov.u32         %r3, %ctaid.x;
    mov.u32         %r4, %ntid.x;
    mov.u32         %r5, %tid.x;
    mad.lo.s32      %r6, %r3, %r4, %r5;
    mul.lo.u32      %r7, %r1, %r2;
    setp.ge.u32     %p1, %r6, %r7;
    @%p1 bra        DONE;

    div.u32         %r8, %r6, %r2;          // selection index
    rem.u32         %r9, %r6, %r2;          // word index

    cvt.u64.u32     %rd4, %r8;
    shl.b64         %rd5, %rd4, 2;
    add.u64         %rd6, %rd1, %rd5;
    ld.global.u32   %r10, [%rd6];           // slot

    cvt.u64.u32     %rd7, %r6;
    shl.b64         %rd8, %rd7, 2;
    add.u64         %rd9, %rd3, %rd8;       // &out word

    setp.eq.u32     %p2, %r10, 0xFFFFFFFF;
    @%p2 bra        PAD;

    mul.lo.u32      %r11, %r10, %r2;
    add.u32         %r11, %r11, %r9;
    cvt.u64.u32     %rd10, %r11;
    shl.b64         %rd11, %rd10, 2;
    add.u64         %rd12, %rd2, %rd11;
    ld.global.u32   %r12, [%rd12];
    st.global.u32   [%rd9], %r12;
    ret;

PAD:
    mov.u32         %r13, 0;
    st.global.u32   [%rd9], %r13;
DONE:
    ret;
}
";

#[cfg(test)]
mod tests {
    use super::SELECTION_PTX;

    #[test]
    fn ptx_is_ascii() {
        assert!(
            SELECTION_PTX.is_ascii(),
            "the driver JIT requires ASCII PTX"
        );
    }

    #[test]
    fn ptx_declares_every_kernel() {
        for kernel in super::KERNELS {
            assert!(
                SELECTION_PTX.contains(&format!(".visible .entry {kernel}(")),
                "missing kernel {kernel}"
            );
        }
    }
}
