use super::{ByteReader, FormatError};
use strata_core_next::CommitVersion;

mod artifact;
mod data;
mod index;
mod properties;

pub(crate) use artifact::{
    decode_immutable_table, decode_immutable_table_data_block, decode_immutable_table_metadata,
    encode_immutable_table, encode_immutable_table_with_block_compressions,
    seek_immutable_table_data_block_point, streaming_entry_size_estimate, ImmutableTable,
    ImmutableTableMetadata, ImmutableTableStreamingEncoder, ImmutableTableStreamingOutput,
    StreamingTableRow,
};
pub(crate) use data::TableDataBlockPointSeek;
pub(crate) use index::TableIndexEntry;
pub(crate) const MAX_TABLE_HEADER_SIZE: usize = TABLE_HEADER_SIZE;
pub(crate) const MAX_TABLE_FOOTER_SIZE: usize = TABLE_FOOTER_SIZE;

const TABLE_HEADER_FORMAT: &str = "table_header";
const TABLE_FOOTER_FORMAT: &str = "table_footer";
const TABLE_BLOCK_FRAME_FORMAT: &str = "table_block_frame";
const TABLE_FILTER_FRAME_FORMAT: &str = "table_filter_frame";
const TABLE_MAGIC: [u8; 4] = *b"STTB";
const TABLE_FOOTER_MAGIC: [u8; 4] = *b"STTF";
const TABLE_FORMAT_VERSION: u32 = 1;
pub(crate) const TABLE_HEADER_SIZE: usize = 64;
const TABLE_HEADER_SIZE_U32: u32 = 64;
pub(crate) const TABLE_FOOTER_SIZE: usize = 64;
const TABLE_BLOCK_FRAME_HEADER_SIZE: usize = 12;
const TABLE_BLOCK_FRAME_OVERHEAD: usize = TABLE_BLOCK_FRAME_HEADER_SIZE + 4;
pub(crate) const MAX_TABLE_DATA_BLOCKS: u32 = 1_048_576;
pub(crate) const MAX_TABLE_ROWS: u64 = 1 << 40;
pub(crate) const MAX_TABLE_BLOCK_ENTRIES: u32 = 1_048_576;
pub(crate) const MAX_TABLE_KEY_BYTES: usize = 64 * 1024;
pub(crate) const MAX_TABLE_ROW_BYTES: usize = 64 * 1024 * 1024;
const MAX_TABLE_BLOCK_ENCODED_BYTES: usize = 64 * 1024 * 1024;
pub(crate) const MAX_TABLE_BLOCK_DECODED_BYTES: usize = 64 * 1024 * 1024;
const ZSTD_COMPRESSION_LEVEL: i32 = 0;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableHeader {
    target_data_block_size: u32,
    data_block_count: u32,
    row_count: u64,
    commit_min: CommitVersion,
    commit_max: CommitVersion,
}

impl TableHeader {
    pub(crate) fn new(
        target_data_block_size: u32,
        data_block_count: u32,
        row_count: u64,
        commit_min: CommitVersion,
        commit_max: CommitVersion,
    ) -> Result<Self, FormatError> {
        validate_table_header_facts(
            target_data_block_size,
            data_block_count,
            row_count,
            commit_min,
            commit_max,
        )?;
        Ok(Self {
            target_data_block_size,
            data_block_count,
            row_count,
            commit_min,
            commit_max,
        })
    }

    pub(crate) const fn target_data_block_size(self) -> u32 {
        self.target_data_block_size
    }

    pub(crate) const fn data_block_count(self) -> u32 {
        self.data_block_count
    }

    pub(crate) const fn row_count(self) -> u64 {
        self.row_count
    }

    pub(crate) const fn commit_min(self) -> CommitVersion {
        self.commit_min
    }

    pub(crate) const fn commit_max(self) -> CommitVersion {
        self.commit_max
    }
}

pub(crate) fn encode_table_header(header: &TableHeader) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(TABLE_HEADER_SIZE);
    bytes.extend_from_slice(&TABLE_MAGIC);
    bytes.extend_from_slice(&TABLE_FORMAT_VERSION.to_le_bytes());
    bytes.extend_from_slice(&TABLE_HEADER_SIZE_U32.to_le_bytes());
    bytes.extend_from_slice(&0u32.to_le_bytes());
    bytes.extend_from_slice(&header.target_data_block_size().to_le_bytes());
    bytes.extend_from_slice(&header.data_block_count().to_le_bytes());
    bytes.extend_from_slice(&header.row_count().to_le_bytes());
    bytes.extend_from_slice(&header.commit_min().as_u64().to_le_bytes());
    bytes.extend_from_slice(&header.commit_max().as_u64().to_le_bytes());
    bytes.extend_from_slice(&[0; 16]);
    debug_assert_eq!(bytes.len(), TABLE_HEADER_SIZE);
    bytes
}

pub(crate) fn decode_table_header(bytes: &[u8]) -> Result<(TableHeader, usize), FormatError> {
    if bytes.len() < TABLE_HEADER_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: TABLE_HEADER_FORMAT,
            needed: TABLE_HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let mut reader = ByteReader::new(TABLE_HEADER_FORMAT, &bytes[..TABLE_HEADER_SIZE]);
    if reader.read_exact(4)? != TABLE_MAGIC {
        return Err(FormatError::InvalidMagic {
            format: TABLE_HEADER_FORMAT,
        });
    }
    validate_table_version(reader.read_u32_le()?, TABLE_HEADER_FORMAT)?;
    let header_size = reader.read_u32_le()?;
    if header_size != TABLE_HEADER_SIZE_U32 {
        return Err(FormatError::InvalidLength {
            field: "table_header_size",
        });
    }
    let flags = reader.read_u32_le()?;
    if flags != 0 {
        return Err(FormatError::UnsupportedFlags {
            format: TABLE_HEADER_FORMAT,
            flags,
        });
    }

    let target_data_block_size = reader.read_u32_le()?;
    let data_block_count = reader.read_u32_le()?;
    let row_count = reader.read_u64_le()?;
    let commit_min = CommitVersion::new(reader.read_u64_le()?);
    let commit_max = CommitVersion::new(reader.read_u64_le()?);
    let reserved = reader.read_exact(16)?;
    if reserved.iter().any(|byte| *byte != 0) {
        return Err(FormatError::InvalidValue { field: "reserved" });
    }
    reader.finish()?;

    Ok((
        TableHeader::new(
            target_data_block_size,
            data_block_count,
            row_count,
            commit_min,
            commit_max,
        )?,
        TABLE_HEADER_SIZE,
    ))
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TableFooter {
    index_block_offset: u64,
    index_block_frame_len: u32,
    // BS4.2: 0 = absent (the historical layout); nonzero locates the filter frame between the data
    // region and the index. Validity of a nonzero slot is enforced by `validate_footer_layout`.
    filter_block_offset: u64,
    filter_block_frame_len: u32,
    properties_block_offset: u64,
    properties_block_frame_len: u32,
}

impl TableFooter {
    pub(crate) fn new(
        index_block_offset: u64,
        index_block_frame_len: u32,
        properties_block_offset: u64,
        properties_block_frame_len: u32,
    ) -> Result<Self, FormatError> {
        Self::new_with_filter(
            index_block_offset,
            index_block_frame_len,
            0,
            0,
            properties_block_offset,
            properties_block_frame_len,
        )
    }

    /// BS4.2: construct a footer that may carry a nonzero filter slot (`0` = absent).
    pub(crate) fn new_with_filter(
        index_block_offset: u64,
        index_block_frame_len: u32,
        filter_block_offset: u64,
        filter_block_frame_len: u32,
        properties_block_offset: u64,
        properties_block_frame_len: u32,
    ) -> Result<Self, FormatError> {
        if index_block_frame_len == 0 {
            return Err(FormatError::InvalidLength {
                field: "index_block_frame_len",
            });
        }
        if properties_block_frame_len == 0 {
            return Err(FormatError::InvalidLength {
                field: "properties_block_frame_len",
            });
        }
        Ok(Self {
            index_block_offset,
            index_block_frame_len,
            filter_block_offset,
            filter_block_frame_len,
            properties_block_offset,
            properties_block_frame_len,
        })
    }

    pub(crate) const fn index_block_offset(self) -> u64 {
        self.index_block_offset
    }

    pub(crate) const fn index_block_frame_len(self) -> u32 {
        self.index_block_frame_len
    }

    pub(crate) const fn filter_block_offset(self) -> u64 {
        self.filter_block_offset
    }

    pub(crate) const fn filter_block_frame_len(self) -> u32 {
        self.filter_block_frame_len
    }

    /// BS4.2: whether a persisted filter frame is present (nonzero slot).
    pub(crate) const fn has_filter(self) -> bool {
        self.filter_block_frame_len != 0
    }

    pub(crate) const fn properties_block_offset(self) -> u64 {
        self.properties_block_offset
    }

    pub(crate) const fn properties_block_frame_len(self) -> u32 {
        self.properties_block_frame_len
    }
}

pub(crate) fn encode_table_footer(
    footer: &TableFooter,
    table_prefix: &[u8],
) -> Result<Vec<u8>, FormatError> {
    validate_footer_layout(footer, table_prefix.len())?;

    let mut bytes = Vec::with_capacity(TABLE_FOOTER_SIZE);
    bytes.extend_from_slice(&footer.index_block_offset().to_le_bytes());
    bytes.extend_from_slice(&footer.index_block_frame_len().to_le_bytes());
    bytes.extend_from_slice(&footer.filter_block_offset().to_le_bytes());
    bytes.extend_from_slice(&footer.filter_block_frame_len().to_le_bytes());
    bytes.extend_from_slice(&footer.properties_block_offset().to_le_bytes());
    bytes.extend_from_slice(&footer.properties_block_frame_len().to_le_bytes());
    bytes.extend_from_slice(&TABLE_FOOTER_MAGIC);
    bytes.extend_from_slice(&[0; 20]);

    let mut hasher = crc32fast::Hasher::new();
    hasher.update(table_prefix);
    hasher.update(&bytes);
    bytes.extend_from_slice(&hasher.finalize().to_le_bytes());
    debug_assert_eq!(bytes.len(), TABLE_FOOTER_SIZE);
    Ok(bytes)
}

pub(crate) fn decode_table_footer(table_bytes: &[u8]) -> Result<TableFooter, FormatError> {
    if table_bytes.len() < TABLE_FOOTER_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: TABLE_FOOTER_FORMAT,
            needed: TABLE_FOOTER_SIZE,
            actual: table_bytes.len(),
        });
    }
    let footer_start = table_bytes.len() - TABLE_FOOTER_SIZE;
    let stored_crc = u32::from_le_bytes(table_bytes[table_bytes.len() - 4..].try_into().map_err(
        |_| FormatError::InvalidLength {
            field: "table_crc32",
        },
    )?);
    let computed_crc = crc32fast::hash(&table_bytes[..table_bytes.len() - 4]);
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: TABLE_FOOTER_FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let footer_bytes = &table_bytes[footer_start..table_bytes.len() - 4];
    let mut reader = ByteReader::new(TABLE_FOOTER_FORMAT, footer_bytes);
    let index_block_offset = reader.read_u64_le()?;
    let index_block_frame_len = reader.read_u32_le()?;
    // BS4.2: the filter slot is accepted (0 = absent, nonzero = present); a nonzero slot is
    // range-validated by `validate_footer_layout`, not rejected outright.
    let filter_block_offset = reader.read_u64_le()?;
    let filter_block_frame_len = reader.read_u32_le()?;
    let properties_block_offset = reader.read_u64_le()?;
    let properties_block_frame_len = reader.read_u32_le()?;
    if reader.read_exact(4)? != TABLE_FOOTER_MAGIC {
        return Err(FormatError::InvalidMagic {
            format: TABLE_FOOTER_FORMAT,
        });
    }
    let reserved = reader.read_exact(20)?;
    if reserved.iter().any(|byte| *byte != 0) {
        return Err(FormatError::InvalidValue { field: "reserved" });
    }
    reader.finish()?;

    let footer = TableFooter::new_with_filter(
        index_block_offset,
        index_block_frame_len,
        filter_block_offset,
        filter_block_frame_len,
        properties_block_offset,
        properties_block_frame_len,
    )?;
    validate_footer_layout(&footer, footer_start)?;
    Ok(footer)
}

pub(crate) fn decode_table_footer_metadata(
    footer_bytes: &[u8],
    footer_start: usize,
) -> Result<TableFooter, FormatError> {
    if footer_bytes.len() != TABLE_FOOTER_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: TABLE_FOOTER_FORMAT,
            needed: TABLE_FOOTER_SIZE,
            actual: footer_bytes.len(),
        });
    }

    let mut reader = ByteReader::new(TABLE_FOOTER_FORMAT, &footer_bytes[..TABLE_FOOTER_SIZE - 4]);
    let index_block_offset = reader.read_u64_le()?;
    let index_block_frame_len = reader.read_u32_le()?;
    // BS4.2: the filter slot is accepted (0 = absent, nonzero = present); a nonzero slot is
    // range-validated by `validate_footer_layout`, not rejected outright.
    let filter_block_offset = reader.read_u64_le()?;
    let filter_block_frame_len = reader.read_u32_le()?;
    let properties_block_offset = reader.read_u64_le()?;
    let properties_block_frame_len = reader.read_u32_le()?;
    if reader.read_exact(4)? != TABLE_FOOTER_MAGIC {
        return Err(FormatError::InvalidMagic {
            format: TABLE_FOOTER_FORMAT,
        });
    }
    let reserved = reader.read_exact(20)?;
    if reserved.iter().any(|byte| *byte != 0) {
        return Err(FormatError::InvalidValue { field: "reserved" });
    }
    reader.finish()?;

    let footer = TableFooter::new_with_filter(
        index_block_offset,
        index_block_frame_len,
        filter_block_offset,
        filter_block_frame_len,
        properties_block_offset,
        properties_block_frame_len,
    )?;
    validate_footer_layout(&footer, footer_start)?;
    Ok(footer)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableBlockKind {
    Data,
    Index,
    // BS4.2: byte 3, the reserved gap between Index (2) and Properties (4). Holds the persisted
    // bloom-filter frame; sits physically between the data region and the index.
    Filter,
    Properties,
}

impl TableBlockKind {
    const fn as_byte(self) -> u8 {
        match self {
            Self::Data => 1,
            Self::Index => 2,
            Self::Filter => 3,
            Self::Properties => 4,
        }
    }

    fn from_byte(value: u8) -> Result<Self, FormatError> {
        match value {
            1 => Ok(Self::Data),
            2 => Ok(Self::Index),
            3 => Ok(Self::Filter),
            4 => Ok(Self::Properties),
            _ => Err(invalid_block_type()),
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum TableCompression {
    Uncompressed,
    Zstd,
}

impl TableCompression {
    const fn as_byte(self) -> u8 {
        match self {
            Self::Uncompressed => 0,
            Self::Zstd => 1,
        }
    }

    fn from_byte(value: u8) -> Result<Self, FormatError> {
        match value {
            0 => Ok(Self::Uncompressed),
            1 => Ok(Self::Zstd),
            codec => Err(FormatError::UnsupportedCompression {
                format: TABLE_BLOCK_FRAME_FORMAT,
                codec,
            }),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableBlockFrame {
    kind: TableBlockKind,
    compression: TableCompression,
    decoded_payload: Vec<u8>,
}

impl TableBlockFrame {
    pub(crate) fn new(
        kind: TableBlockKind,
        compression: TableCompression,
        decoded_payload: impl Into<Vec<u8>>,
    ) -> Result<Self, FormatError> {
        let decoded_payload = decoded_payload.into();
        validate_block_payload_len(
            decoded_payload.len(),
            "decoded_len",
            MAX_TABLE_BLOCK_DECODED_BYTES,
        )?;
        Ok(Self {
            kind,
            compression,
            decoded_payload,
        })
    }

    pub(crate) const fn kind(&self) -> TableBlockKind {
        self.kind
    }

    pub(crate) const fn compression(&self) -> TableCompression {
        self.compression
    }

    pub(crate) fn decoded_payload(&self) -> &[u8] {
        &self.decoded_payload
    }
}

pub(crate) fn encode_table_block_frame(frame: &TableBlockFrame) -> Result<Vec<u8>, FormatError> {
    let encoded_payload = match frame.compression() {
        TableCompression::Uncompressed => frame.decoded_payload().to_vec(),
        TableCompression::Zstd => zstd_compress(frame.decoded_payload())?,
    };
    validate_block_payload_len(
        encoded_payload.len(),
        "encoded_len",
        MAX_TABLE_BLOCK_ENCODED_BYTES,
    )?;

    let encoded_len =
        u32::try_from(encoded_payload.len()).map_err(|_| FormatError::InvalidLength {
            field: "encoded_len",
        })?;
    let decoded_len =
        u32::try_from(frame.decoded_payload().len()).map_err(|_| FormatError::InvalidLength {
            field: "decoded_len",
        })?;

    let mut bytes = Vec::with_capacity(TABLE_BLOCK_FRAME_OVERHEAD + encoded_payload.len());
    bytes.push(frame.kind().as_byte());
    bytes.push(frame.compression().as_byte());
    bytes.extend_from_slice(&0u16.to_le_bytes());
    bytes.extend_from_slice(&encoded_len.to_le_bytes());
    bytes.extend_from_slice(&decoded_len.to_le_bytes());
    bytes.extend_from_slice(&encoded_payload);
    let crc = crc32fast::hash(&bytes);
    bytes.extend_from_slice(&crc.to_le_bytes());
    Ok(bytes)
}

pub(crate) fn decode_table_block_frame(
    bytes: &[u8],
) -> Result<(TableBlockFrame, usize), FormatError> {
    decode_table_block_frame_inner(bytes, None)
}

pub(crate) fn decode_table_block_frame_as(
    bytes: &[u8],
    expected_kind: TableBlockKind,
) -> Result<(TableBlockFrame, usize), FormatError> {
    decode_table_block_frame_inner(bytes, Some(expected_kind))
}

/// BS4.2: the filter-frame payload, serialized inside a `TableBlockKind::Filter` block frame. The
/// format layer stays bloom-agnostic — it moves these raw parts; the `table` layer converts to
/// and from a `TableBloomFilter`.
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TableFilterFrame {
    pub(crate) probes: u8,
    pub(crate) key_count: u64,
    pub(crate) bit_count: u64,
    pub(crate) bits: Vec<u8>,
}

/// The only filter subformat assigned in V1 (spec §17).
const FILTER_FORMAT_VERSION: u32 = 1;
/// Payload header before the bit array: version(4) + probes(1) + `key_count`(8) + `bit_count`(8).
const FILTER_FRAME_HEADER_SIZE: usize = 4 + 1 + 8 + 8;
/// Format-validity ceilings, mirroring the runtime bloom bounds (`table/cache.rs`): ≤30 probes, and a
/// bit array ≤16 MiB (the frame codec independently bounds the whole payload).
const MAX_FILTER_PROBES: u8 = 30;
const MAX_FILTER_BITS_BYTES: usize = 16 * 1024 * 1024;

/// Serialize a filter frame: the LE payload wrapped (uncompressed — bloom bits are ~random) in the
/// shared CRC'd block frame.
pub(crate) fn encode_filter_frame(frame: &TableFilterFrame) -> Result<Vec<u8>, FormatError> {
    // BS4.2: encode is the exact inverse gate of decode — an out-of-bounds frame must never be
    // persisted in this frozen format, or the written table would be permanently undecodable.
    if frame.probes > MAX_FILTER_PROBES {
        return Err(FormatError::InvalidValue {
            field: "filter_probes",
        });
    }
    if frame.bits.len() > MAX_FILTER_BITS_BYTES {
        return Err(FormatError::InvalidLength {
            field: "filter_bits",
        });
    }
    let expected_bits_len = usize::try_from(frame.bit_count)
        .map_err(|_| FormatError::InvalidLength {
            field: "filter_bit_count",
        })?
        .div_ceil(8);
    if frame.bits.len() != expected_bits_len {
        return Err(FormatError::InvalidLength {
            field: "filter_bits",
        });
    }

    let mut payload = Vec::with_capacity(FILTER_FRAME_HEADER_SIZE + frame.bits.len());
    payload.extend_from_slice(&FILTER_FORMAT_VERSION.to_le_bytes());
    payload.push(frame.probes);
    payload.extend_from_slice(&frame.key_count.to_le_bytes());
    payload.extend_from_slice(&frame.bit_count.to_le_bytes());
    payload.extend_from_slice(&frame.bits);
    let block = TableBlockFrame::new(
        TableBlockKind::Filter,
        TableCompression::Uncompressed,
        payload,
    )?;
    encode_table_block_frame(&block)
}

/// Decode + validate a filter frame. The block frame's CRC guards the bytes; this additionally
/// rejects an unknown subformat version (forward-compat gate) and any probe/bit-array shape outside
/// the runtime bounds — the integrity gate that keeps a corrupt filter from later answering a false
/// `DefinitelyAbsent`.
pub(crate) fn decode_filter_frame(bytes: &[u8]) -> Result<(TableFilterFrame, usize), FormatError> {
    let (block, consumed) = decode_table_block_frame_as(bytes, TableBlockKind::Filter)?;
    let mut reader = ByteReader::new(TABLE_FILTER_FRAME_FORMAT, block.decoded_payload());
    let version = reader.read_u32_le()?;
    if version != FILTER_FORMAT_VERSION {
        return Err(FormatError::FutureFormat {
            format: TABLE_FILTER_FRAME_FORMAT,
            version,
            max_supported: FILTER_FORMAT_VERSION,
        });
    }
    let probes = reader.read_u8()?;
    let key_count = reader.read_u64_le()?;
    let bit_count = reader.read_u64_le()?;
    let remaining = reader.remaining();
    let bits = reader.read_exact(remaining)?.to_vec();

    if probes > MAX_FILTER_PROBES {
        return Err(FormatError::InvalidValue {
            field: "filter_probes",
        });
    }
    if bits.len() > MAX_FILTER_BITS_BYTES {
        return Err(FormatError::InvalidLength {
            field: "filter_bits",
        });
    }
    let expected_bits_len = usize::try_from(bit_count)
        .map_err(|_| FormatError::InvalidLength {
            field: "filter_bit_count",
        })?
        .div_ceil(8);
    if bits.len() != expected_bits_len {
        return Err(FormatError::InvalidLength {
            field: "filter_bits",
        });
    }
    Ok((
        TableFilterFrame {
            probes,
            key_count,
            bit_count,
            bits,
        },
        consumed,
    ))
}

fn decode_table_block_frame_inner(
    bytes: &[u8],
    expected_kind: Option<TableBlockKind>,
) -> Result<(TableBlockFrame, usize), FormatError> {
    if bytes.len() < TABLE_BLOCK_FRAME_HEADER_SIZE {
        return Err(FormatError::InsufficientBytes {
            format: TABLE_BLOCK_FRAME_FORMAT,
            needed: TABLE_BLOCK_FRAME_HEADER_SIZE,
            actual: bytes.len(),
        });
    }

    let mut reader = ByteReader::new(
        TABLE_BLOCK_FRAME_FORMAT,
        &bytes[..TABLE_BLOCK_FRAME_HEADER_SIZE],
    );
    let kind = TableBlockKind::from_byte(reader.read_u8()?)?;
    if let Some(expected_kind) = expected_kind {
        if kind != expected_kind {
            return Err(FormatError::InvalidValue {
                field: "block_type",
            });
        }
    }
    let compression = TableCompression::from_byte(reader.read_u8()?)?;
    let flags = reader.read_u16_le()?;
    if flags != 0 {
        return Err(FormatError::UnsupportedFlags {
            format: TABLE_BLOCK_FRAME_FORMAT,
            flags: u32::from(flags),
        });
    }
    let encoded_len =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "encoded_len",
        })?;
    let decoded_len =
        usize::try_from(reader.read_u32_le()?).map_err(|_| FormatError::InvalidLength {
            field: "decoded_len",
        })?;
    reader.finish()?;
    validate_block_payload_len(encoded_len, "encoded_len", MAX_TABLE_BLOCK_ENCODED_BYTES)?;
    validate_block_payload_len(decoded_len, "decoded_len", MAX_TABLE_BLOCK_DECODED_BYTES)?;

    let total_len = TABLE_BLOCK_FRAME_HEADER_SIZE
        .checked_add(encoded_len)
        .and_then(|len| len.checked_add(4))
        .ok_or(FormatError::InvalidLength {
            field: TABLE_BLOCK_FRAME_FORMAT,
        })?;
    if bytes.len() < total_len {
        return Err(FormatError::InsufficientBytes {
            format: TABLE_BLOCK_FRAME_FORMAT,
            needed: total_len,
            actual: bytes.len(),
        });
    }

    let encoded_payload =
        &bytes[TABLE_BLOCK_FRAME_HEADER_SIZE..TABLE_BLOCK_FRAME_HEADER_SIZE + encoded_len];
    let stored_crc =
        u32::from_le_bytes(bytes[total_len - 4..total_len].try_into().map_err(|_| {
            FormatError::InvalidLength {
                field: "table_block_crc32",
            }
        })?);
    let computed_crc = crc32fast::hash(&bytes[..total_len - 4]);
    if stored_crc != computed_crc {
        return Err(FormatError::ChecksumMismatch {
            format: TABLE_BLOCK_FRAME_FORMAT,
            expected: stored_crc,
            computed: computed_crc,
        });
    }

    let decoded_payload = match compression {
        TableCompression::Uncompressed => {
            if encoded_len != decoded_len {
                return Err(FormatError::InvalidLength {
                    field: "table_block_lengths",
                });
            }
            encoded_payload.to_vec()
        }
        TableCompression::Zstd => zstd_decompress(encoded_payload, decoded_len)?,
    };
    if decoded_payload.len() != decoded_len {
        return Err(FormatError::InvalidLength {
            field: "decoded_len",
        });
    }

    Ok((
        TableBlockFrame::new(kind, compression, decoded_payload)?,
        total_len,
    ))
}

fn validate_table_header_facts(
    target_data_block_size: u32,
    data_block_count: u32,
    row_count: u64,
    commit_min: CommitVersion,
    commit_max: CommitVersion,
) -> Result<(), FormatError> {
    if target_data_block_size == 0 {
        return Err(FormatError::InvalidLength {
            field: "target_data_block_size",
        });
    }
    if data_block_count == 0 || data_block_count > MAX_TABLE_DATA_BLOCKS {
        return Err(FormatError::InvalidLength {
            field: "data_block_count",
        });
    }
    if row_count == 0 || row_count > MAX_TABLE_ROWS {
        return Err(FormatError::InvalidLength { field: "row_count" });
    }
    if commit_min.as_u64() > commit_max.as_u64() {
        return Err(FormatError::InvalidValue {
            field: "commit_range",
        });
    }
    Ok(())
}

fn validate_table_version(version: u32, format: &'static str) -> Result<(), FormatError> {
    match version {
        TABLE_FORMAT_VERSION => Ok(()),
        0 => Err(FormatError::PreV1Format { format, version }),
        version => Err(FormatError::FutureFormat {
            format,
            version,
            max_supported: TABLE_FORMAT_VERSION,
        }),
    }
}

fn validate_footer_layout(footer: &TableFooter, footer_start: usize) -> Result<(), FormatError> {
    if footer_start < TABLE_HEADER_SIZE {
        return Err(FormatError::InvalidLength {
            field: "table_footer_layout",
        });
    }

    let (index_start, index_end) = checked_footer_range(
        footer.index_block_offset(),
        footer.index_block_frame_len(),
        "index_block",
    )?;
    if index_start < TABLE_HEADER_SIZE {
        return Err(FormatError::InvalidLength {
            field: "index_block_offset",
        });
    }
    if index_end > footer_start {
        return Err(FormatError::InvalidLength {
            field: "index_block_range",
        });
    }

    // BS4.2: a present filter frame sits between the data region and the index — `filter_end` must
    // equal `index_start`, and `filter_start` must be within the object (data may be empty). An
    // absent filter requires both slot fields zero (a lone nonzero offset is malformed).
    if footer.has_filter() {
        let (filter_start, filter_end) = checked_footer_range(
            footer.filter_block_offset(),
            footer.filter_block_frame_len(),
            "filter_block",
        )?;
        if filter_start < TABLE_HEADER_SIZE {
            return Err(FormatError::InvalidLength {
                field: "filter_block_offset",
            });
        }
        if filter_end != index_start {
            return Err(FormatError::InvalidLength {
                field: "filter_block_range",
            });
        }
    } else if footer.filter_block_offset() != 0 {
        return Err(FormatError::InvalidLength {
            field: "filter_block_offset",
        });
    }

    let (properties_start, properties_end) = checked_footer_range(
        footer.properties_block_offset(),
        footer.properties_block_frame_len(),
        "properties_block",
    )?;
    if properties_start != index_end {
        return Err(FormatError::InvalidLength {
            field: "properties_block_offset",
        });
    }
    if properties_end != footer_start {
        return Err(FormatError::InvalidLength {
            field: "properties_block_range",
        });
    }
    Ok(())
}

fn checked_footer_range(
    offset: u64,
    len: u32,
    field: &'static str,
) -> Result<(usize, usize), FormatError> {
    if len == 0 {
        return Err(FormatError::InvalidLength { field });
    }
    let start = usize::try_from(offset).map_err(|_| FormatError::InvalidLength { field })?;
    let len = usize::try_from(len).map_err(|_| FormatError::InvalidLength { field })?;
    let end = start
        .checked_add(len)
        .ok_or(FormatError::InvalidLength { field })?;
    Ok((start, end))
}

fn validate_block_payload_len(
    len: usize,
    field: &'static str,
    max: usize,
) -> Result<(), FormatError> {
    if len == 0 {
        return Err(FormatError::InvalidLength { field });
    }
    if len > max {
        return Err(FormatError::InvalidLength { field });
    }
    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn zstd_compress(decoded_payload: &[u8]) -> Result<Vec<u8>, FormatError> {
    zstd::bulk::compress(decoded_payload, ZSTD_COMPRESSION_LEVEL).map_err(|_| {
        FormatError::CompressionFailed {
            format: TABLE_BLOCK_FRAME_FORMAT,
        }
    })
}

#[cfg(target_arch = "wasm32")]
fn zstd_compress(decoded_payload: &[u8]) -> Result<Vec<u8>, FormatError> {
    use zstd_pure_rs::prelude::{ERR_isError, ZSTD_compress, ZSTD_compressBound};

    let bound = ZSTD_compressBound(decoded_payload.len());
    if ERR_isError(bound) {
        return Err(FormatError::CompressionFailed {
            format: TABLE_BLOCK_FRAME_FORMAT,
        });
    }

    let mut encoded_payload = vec![0; bound];
    let written = ZSTD_compress(
        &mut encoded_payload,
        decoded_payload,
        ZSTD_COMPRESSION_LEVEL,
    );
    if ERR_isError(written) {
        return Err(FormatError::CompressionFailed {
            format: TABLE_BLOCK_FRAME_FORMAT,
        });
    }
    encoded_payload.truncate(written);
    Ok(encoded_payload)
}

#[cfg(not(target_arch = "wasm32"))]
fn zstd_decompress(encoded_payload: &[u8], decoded_len: usize) -> Result<Vec<u8>, FormatError> {
    zstd::bulk::decompress(encoded_payload, decoded_len).map_err(|_| {
        FormatError::DecompressionFailed {
            format: TABLE_BLOCK_FRAME_FORMAT,
        }
    })
}

#[cfg(target_arch = "wasm32")]
fn zstd_decompress(encoded_payload: &[u8], decoded_len: usize) -> Result<Vec<u8>, FormatError> {
    use zstd_pure_rs::prelude::{ERR_isError, ZSTD_decompress};

    let mut decoded_payload = vec![0; decoded_len];
    let written = ZSTD_decompress(&mut decoded_payload, encoded_payload);
    if ERR_isError(written) {
        return Err(FormatError::DecompressionFailed {
            format: TABLE_BLOCK_FRAME_FORMAT,
        });
    }
    decoded_payload.truncate(written);
    Ok(decoded_payload)
}

const fn invalid_block_type() -> FormatError {
    FormatError::InvalidValue {
        field: "block_type",
    }
}

#[cfg(test)]
mod artifact_tests;
#[cfg(test)]
mod data_tests;
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod filter_tests;
#[cfg(test)]
mod golden_tests;
#[cfg(test)]
mod index_tests;
#[cfg(test)]
mod properties_tests;
#[cfg(test)]
#[cfg(not(target_arch = "wasm32"))]
mod property_tests;
#[cfg(test)]
mod test_support;
#[cfg(test)]
mod tests;
