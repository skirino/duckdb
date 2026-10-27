//! The row-format scatter/gather kernels of `TupleDataCollection`.
//!
//! Rows are `row_width`-byte blocks written by a `TupleDataAllocator` and handed
//! to the kernels as addresses, one per row, in a pointer array. The kernels
//! read a source column through the three arrays a `UnifiedVectorFormat`
//! exposes — the entry array, the selection vector and the validity mask — and
//! write each row through its address.
//!
//! # What is safe here, and what is not
//!
//! Everything the kernels compute happens on slices: the source entries, the
//! two selection vectors, the validity masks and the heap regions are all
//! bounds-checked, and a kernel that ran short of any of them returns an error
//! instead of reading or writing past its end. What cannot be a slice is the
//! row itself, because every row is a separate allocation reached through its
//! address, so [`row_bytes`] and [`heap_bytes`] are the only constructors that
//! turn an address into memory and each carries the invariant the caller
//! upholds. They are opened once per row, and every heap region is opened at
//! exactly the length that is about to be written, so a kernel cannot spill
//! into the neighbouring row without first failing a bounds check.

// The row blocks are allocator memory reached through addresses, which no owned
// Rust value can stand for; `row_bytes` and `heap_bytes` are the whole of that
// surface, and every call site carries the invariant that makes it sound.
#![allow(unsafe_code)]

/// Bits per byte of a row's validity header.
const VALIDITY_BITS: usize = 8;

/// `string_t` is a 4-byte length and 12 bytes of payload.
const STRING_SIZE: usize = 16;
/// A string of up to this many bytes is stored inside the `string_t`.
const STRING_INLINE_LENGTH: usize = 12;
/// The length and prefix of a `string_t`, which stay in the row.
const STRING_HEADER_SIZE: usize = 8;

/// `list_entry_t` is an offset and a length, both 64-bit.
const LIST_ENTRY_SIZE: usize = 16;

// ---------------------------------------------------------------------------
// Views over the arrays a kernel reads
// ---------------------------------------------------------------------------

/// `SelectionVector::get_index`: an empty selection is the identity.
fn selected(selection: &[u32], index: usize) -> Result<usize, String> {
    if selection.is_empty() {
        return Ok(index);
    }
    selection
        .get(index)
        .map(|&entry| entry as usize)
        .ok_or_else(|| format!("selection vector has no entry for row {index}"))
}

/// `ValidityMask::RowIsValid`: an empty mask cannot have nulls.
fn is_valid(validity: &[u64], index: usize) -> Result<bool, String> {
    if validity.is_empty() {
        return Ok(true);
    }
    let word = index / u64::BITS as usize;
    let bit = index % u64::BITS as usize;
    validity
        .get(word)
        .map(|entry| entry & (1u64 << bit) != 0)
        .ok_or_else(|| format!("validity mask has no entry for row {index}"))
}

/// The entry at `index` of an entry array holding `entry_size`-byte entries.
fn entry<'a>(
    entries: &'a [u8],
    index: usize,
    entry_size: usize,
    what: &str,
) -> Result<&'a [u8], String> {
    let start = index
        .checked_mul(entry_size)
        .ok_or_else(|| format!("{what}: entry index {index} overflows the entry array"))?;
    let end = start
        .checked_add(entry_size)
        .ok_or_else(|| format!("{what}: entry index {index} overflows the entry array"))?;
    entries
        .get(start..end)
        .ok_or_else(|| format!("{what}: entry index {index} is outside the entry array"))
}

/// Reads a 64-bit word at `offset` of `bytes`.
fn load_u64(bytes: &[u8], offset: usize) -> Result<u64, String> {
    let end = offset
        .checked_add(8)
        .ok_or_else(|| "a 64-bit read overflows its buffer".to_string())?;
    let slice = bytes
        .get(offset..end)
        .ok_or_else(|| "a 64-bit read falls outside its buffer".to_string())?;
    slice
        .try_into()
        .map(u64::from_le_bytes)
        .map_err(|_| "a 64-bit read has the wrong length".to_string())
}

/// The `offset` and `length` of the `list_entry_t` at `index`.
fn list_entry(entries: &[u8], index: usize, what: &str) -> Result<(u64, u64), String> {
    let bytes = entry(entries, index, LIST_ENTRY_SIZE, what)?;
    Ok((load_u64(bytes, 0)?, load_u64(bytes, 8)?))
}

/// `ValidityBytes::SizeInBytes`: one bit per element.
fn validity_size_in_bytes(count: usize) -> usize {
    count.div_ceil(VALIDITY_BITS)
}

/// `ValidityMask::SetAllValid` over a mask of `count` bits.
fn set_all_valid(mask: &mut [u8], count: usize) {
    let full_bytes = count / VALIDITY_BITS;
    let remaining = count % VALIDITY_BITS;
    for byte in mask.iter_mut().take(full_bytes) {
        *byte = 0xff;
    }
    if remaining != 0
        && let Some(byte) = mask.get_mut(full_bytes)
    {
        *byte |= !(0xffu8 << remaining);
    }
}

/// Clears bit `index` of a within-collection validity mask.
fn set_element_invalid(mask: &mut [u8], index: usize, what: &str) -> Result<(), String> {
    let byte = mask.get_mut(index / VALIDITY_BITS).ok_or_else(|| {
        format!("{what}: the list validity mask has no entry for element {index}")
    })?;
    *byte &= !(1u8 << (index % VALIDITY_BITS));
    Ok(())
}

/// `ValidityBytes::RowIsValidUnsafe` over a within-collection mask.
fn element_is_valid(mask: &[u8], index: usize, what: &str) -> Result<bool, String> {
    let byte = mask.get(index / VALIDITY_BITS).ok_or_else(|| {
        format!("{what}: the list validity mask has no entry for element {index}")
    })?;
    Ok(byte & (1u8 << (index % VALIDITY_BITS)) != 0)
}

/// Clears the bit `column_index` of a row's validity header.
fn set_column_invalid(row: &mut [u8], column_index: usize) -> Result<(), String> {
    let byte = row
        .get_mut(column_index / VALIDITY_BITS)
        .ok_or_else(|| format!("row is too short for the validity bit of column {column_index}"))?;
    *byte &= !(1u8 << (column_index % VALIDITY_BITS));
    Ok(())
}

/// The validity bit of `column_index` in a row.
fn column_is_valid(row: &[u8], column_index: usize) -> Result<bool, String> {
    let byte = row
        .get(column_index / VALIDITY_BITS)
        .ok_or_else(|| format!("row is too short for the validity bit of column {column_index}"))?;
    Ok(byte & (1u8 << (column_index % VALIDITY_BITS)) != 0)
}

/// `ValidityMask::SetInvalid` on a target mask the caller made writable.
fn set_target_invalid(validity: &mut [u64], index: usize) -> Result<(), String> {
    let word = validity
        .get_mut(index / u64::BITS as usize)
        .ok_or_else(|| format!("target validity mask has no entry for row {index}"))?;
    *word &= !(1u64 << (index % u64::BITS as usize));
    Ok(())
}

// ---------------------------------------------------------------------------
// The two ways an address becomes memory
// ---------------------------------------------------------------------------

/// The `length` bytes of the row at `address`.
///
/// # Safety
///
/// `address` must be the start of a live row block of at least `length` bytes in
/// an allocation the caller may write, and no other reference to those bytes may
/// be alive for the duration of the borrow.
unsafe fn row_bytes<'a>(address: u64, length: usize) -> &'a mut [u8] {
    // SAFETY: the caller vouches for the block, which is what the allocator that
    // produced `address` guarantees; the length is a prefix of the layout's row
    // width and so never past the end of the block.
    unsafe { std::slice::from_raw_parts_mut(address as usize as *mut u8, length) }
}

/// The `length` bytes at `address`, read.
///
/// # Safety
///
/// `address` must be the start of `length` initialised bytes that stay live and
/// unmodified for the duration of the borrow.
unsafe fn heap_bytes<'a>(address: u64, length: usize) -> &'a [u8] {
    // SAFETY: the caller vouches for the region, which is a slice of the memory
    // the value at `address` was stored in.
    unsafe { std::slice::from_raw_parts(address as usize as *const u8, length) }
}

/// The `length` bytes at `address`, written.
///
/// # Safety
///
/// `address` must be the start of `length` writable bytes in an allocation the
/// caller owns for the duration of the borrow.
unsafe fn heap_bytes_mut<'a>(address: u64, length: usize) -> &'a mut [u8] {
    // SAFETY: the caller vouches for the region, which is the heap space the
    // heap-size pass reserved for this row.
    unsafe { std::slice::from_raw_parts_mut(address as usize as *mut u8, length) }
}

/// Copies `value` to `address` and returns the address past it.
///
/// # Safety
///
/// `address` must be the start of `value.len()` writable bytes.
unsafe fn write_heap(address: u64, value: &[u8]) -> u64 {
    // SAFETY: the caller vouches for `value.len()` writable bytes at `address`.
    unsafe { heap_bytes_mut(address, value.len()) }.copy_from_slice(value);
    address + value.len() as u64
}

/// Truncates `addresses` to the `count` rows a kernel was asked to write.
fn rows<'a>(addresses: &'a [u64], count: usize, what: &str) -> Result<&'a [u64], String> {
    addresses.get(..count).ok_or_else(|| {
        format!(
            "{what}: {count} rows were requested but the row-location array holds {}",
            addresses.len()
        )
    })
}

fn rows_mut<'a>(
    addresses: &'a mut [u64],
    count: usize,
    what: &str,
) -> Result<&'a mut [u64], String> {
    let available = addresses.len();
    addresses.get_mut(..count).ok_or_else(|| {
        format!("{what}: {count} rows were requested but the array holds {available}")
    })
}

fn as_usize(value: u64, what: &str) -> Result<usize, String> {
    usize::try_from(value).map_err(|_| format!("{what} does not fit in a usize"))
}

// ---------------------------------------------------------------------------
// Scatter
// ---------------------------------------------------------------------------

/// `InitializeValidityMask`: marks every column of each row valid.
pub fn initialize_validity_mask(
    rows: &[u64],
    count: usize,
    validity_bytes: usize,
) -> Result<(), String> {
    for &address in rows.iter().take(count) {
        // SAFETY: the validity header is at the start of a row block.
        unsafe { row_bytes(address, validity_bytes) }.fill(0xff);
    }
    Ok(())
}

/// Scatters a fixed-size column into the rows.
///
/// `null_entry` is `NullValue<T>()` as bytes: the pattern written for a NULL.
#[allow(clippy::too_many_arguments)]
pub fn scatter_fixed(
    source_entries: &[u8],
    entry_size: usize,
    null_entry: &[u8],
    source_selection: &[u32],
    source_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    row_locations: &[u64],
    row_width: usize,
    offset_in_row: usize,
    column_index: usize,
) -> Result<(), String> {
    const WHAT: &str = "scatter_fixed";
    let rows = rows(row_locations, count, WHAT)?;
    if null_entry.len() != entry_size {
        return Err(format!(
            "{WHAT}: the null pattern is {} bytes, not {entry_size}",
            null_entry.len()
        ));
    }

    for (i, &address) in rows.iter().enumerate() {
        let append_index = selected(append_selection, i)?;
        let source_index = selected(source_selection, append_index)?;
        // SAFETY: `address` is a row of the block being scattered into.
        let row = unsafe { row_bytes(address, row_width) };
        let destination = row
            .get_mut(offset_in_row..)
            .and_then(|rest| rest.get_mut(..entry_size))
            .ok_or_else(|| format!("{WHAT}: column {column_index} does not fit in the row"))?;
        if is_valid(source_validity, source_index)? {
            destination.copy_from_slice(entry(source_entries, source_index, entry_size, WHAT)?);
        } else {
            destination.copy_from_slice(null_entry);
            set_column_invalid(row, column_index)?;
        }
    }
    Ok(())
}

/// Scatters a `VARCHAR` column, copying non-inlined strings into the heap.
#[allow(clippy::too_many_arguments)]
pub fn scatter_string(
    source_entries: &[u8],
    source_selection: &[u32],
    source_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    row_locations: &[u64],
    heap_locations: &mut [u64],
    row_width: usize,
    offset_in_row: usize,
    column_index: usize,
) -> Result<(), String> {
    const WHAT: &str = "scatter_string";
    let rows = rows(row_locations, count, WHAT)?;
    let heap = rows_mut(heap_locations, count, WHAT)?;

    for (i, &address) in rows.iter().enumerate() {
        let append_index = selected(append_selection, i)?;
        let source_index = selected(source_selection, append_index)?;
        // SAFETY: `address` is a row of the block being scattered into.
        let row = unsafe { row_bytes(address, row_width) };
        let destination = row
            .get_mut(offset_in_row..)
            .and_then(|rest| rest.get_mut(..STRING_SIZE))
            .ok_or_else(|| format!("{WHAT}: column {column_index} does not fit in the row"))?;

        if !is_valid(source_validity, source_index)? {
            destination.copy_from_slice(&null_string());
            set_column_invalid(row, column_index)?;
            continue;
        }

        let value = entry(source_entries, source_index, STRING_SIZE, WHAT)?;
        let length = u32::from_le_bytes(
            value[..4]
                .try_into()
                .map_err(|_| format!("{WHAT}: a string_t is shorter than its length"))?,
        ) as usize;
        if length <= STRING_INLINE_LENGTH {
            destination.copy_from_slice(value);
            continue;
        }

        // Non-inlined: the characters go to the heap, the length and prefix stay
        // in the row, and the heap address is written after them.
        // SAFETY: the source string owns `length` bytes at the address its
        // `string_t` holds.
        let characters = unsafe { heap_bytes(load_u64(value, STRING_HEADER_SIZE)?, length) };
        // SAFETY: the heap-size pass reserved `length` bytes for this row.
        let mut heap_address = heap[i];
        heap_address = unsafe { write_heap(heap_address, characters) };
        destination[..STRING_HEADER_SIZE].copy_from_slice(&value[..STRING_HEADER_SIZE]);
        destination[STRING_HEADER_SIZE..].copy_from_slice(&(heap[i]).to_le_bytes());
        heap[i] = heap_address;
    }
    Ok(())
}

/// `NullValue<string_t>()` as bytes: a one-character inlined string holding
/// `'\x80'`.
fn null_string() -> [u8; STRING_SIZE] {
    let mut bytes = [0u8; STRING_SIZE];
    bytes[0] = 1;
    bytes[4] = 0x80;
    bytes
}

/// Scatters a `LIST` or `ARRAY` column's header: the heap address into the row,
/// then the list length at that address.
#[allow(clippy::too_many_arguments)]
pub fn scatter_collection(
    list_entries: &[u8],
    list_selection: &[u32],
    list_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    row_locations: &[u64],
    heap_locations: &mut [u64],
    row_width: usize,
    offset_in_row: usize,
    column_index: usize,
) -> Result<(), String> {
    const WHAT: &str = "scatter_collection";
    let rows = rows(row_locations, count, WHAT)?;
    let heap = rows_mut(heap_locations, count, WHAT)?;

    for (i, &address) in rows.iter().enumerate() {
        let append_index = selected(append_selection, i)?;
        let list_index = selected(list_selection, append_index)?;
        // SAFETY: `address` is a row of the block being scattered into.
        let row = unsafe { row_bytes(address, row_width) };

        if !is_valid(list_validity, list_index)? {
            set_column_invalid(row, column_index)?;
            continue;
        }
        let (_, length) = list_entry(list_entries, list_index, WHAT)?;
        let destination = row
            .get_mut(offset_in_row..)
            .and_then(|rest| rest.get_mut(..8))
            .ok_or_else(|| format!("{WHAT}: column {column_index} does not fit in the row"))?;
        destination.copy_from_slice(&heap[i].to_le_bytes());
        // SAFETY: the heap-size pass reserved room for the list length.
        heap[i] = unsafe { write_heap(heap[i], &length.to_le_bytes()) };
    }
    Ok(())
}

/// Scatters a `STRUCT`: its validity bits, and the address of each struct's own
/// row for the recursion to fill.
#[allow(clippy::too_many_arguments)]
pub fn scatter_struct(
    source_selection: &[u32],
    source_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    row_locations: &[u64],
    row_width: usize,
    offset_in_row: usize,
    column_index: usize,
    struct_row_locations: &mut [u64],
) -> Result<(), String> {
    const WHAT: &str = "scatter_struct";
    let rows = rows(row_locations, count, WHAT)?;
    let struct_rows = struct_row_locations
        .get_mut(..count)
        .ok_or_else(|| format!("{WHAT}: the sub-row array is shorter than {count}"))?;

    if !source_validity.is_empty() {
        for (i, &address) in rows.iter().enumerate() {
            let append_index = selected(append_selection, i)?;
            let source_index = selected(source_selection, append_index)?;
            if !is_valid(source_validity, source_index)? {
                // SAFETY: `address` is a row of the block being scattered into.
                let row = unsafe { row_bytes(address, row_width) };
                set_column_invalid(row, column_index)?;
            }
        }
    }
    for (struct_row, &address) in struct_rows.iter_mut().zip(rows) {
        *struct_row = address + offset_in_row as u64;
    }
    Ok(())
}

/// Scatters a fixed-size column into the heap space of each list element.
#[allow(clippy::too_many_arguments)]
pub fn scatter_within_fixed(
    list_entries: &[u8],
    list_selection: &[u32],
    list_validity: &[u64],
    source_entries: &[u8],
    entry_size: usize,
    source_selection: &[u32],
    source_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    heap_locations: &mut [u64],
) -> Result<(), String> {
    const WHAT: &str = "scatter_within_fixed";
    let heap = rows_mut(heap_locations, count, WHAT)?;

    for (i, heap_slot) in heap.iter_mut().enumerate() {
        let append_index = selected(append_selection, i)?;
        let list_index = selected(list_selection, append_index)?;
        if !is_valid(list_validity, list_index)? {
            continue;
        }
        let (list_offset, list_length) = list_entry(list_entries, list_index, WHAT)?;
        if list_length == 0 {
            continue;
        }
        let list_length = as_usize(list_length, "scatter_within_fixed: the list length")?;
        let list_offset = as_usize(list_offset, "scatter_within_fixed: the list offset")?;
        let mask_size = validity_size_in_bytes(list_length);
        let data_size = list_length
            .checked_mul(entry_size)
            .ok_or_else(|| format!("{WHAT}: the element data size overflows"))?;

        // SAFETY: the heap-size pass reserved the mask and the element data.
        let mask = unsafe { heap_bytes_mut(*heap_slot, mask_size) };
        set_all_valid(mask, list_length);
        // SAFETY: the element data follows the mask in the same reservation.
        let data = unsafe { heap_bytes_mut(*heap_slot + mask_size as u64, data_size) };
        *heap_slot += (mask_size + data_size) as u64;

        for child_i in 0..list_length {
            let source_index = selected(source_selection, list_offset + child_i)?;
            if !is_valid(source_validity, source_index)? {
                set_element_invalid(mask, child_i, WHAT)?;
                continue;
            }
            let value = entry(source_entries, source_index, entry_size, WHAT)?;
            data[child_i * entry_size..][..entry_size].copy_from_slice(value);
        }
    }
    Ok(())
}

/// Scatters a `VARCHAR` column into the heap space of each list element.
///
/// Within a list a string is a 4-byte length followed by its characters, not the
/// `string_t` a top-level row holds.
#[allow(clippy::too_many_arguments)]
pub fn scatter_within_string(
    list_entries: &[u8],
    list_selection: &[u32],
    list_validity: &[u64],
    source_entries: &[u8],
    source_selection: &[u32],
    source_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    heap_locations: &mut [u64],
) -> Result<(), String> {
    const WHAT: &str = "scatter_within_string";
    let heap = rows_mut(heap_locations, count, WHAT)?;

    for (i, heap_slot) in heap.iter_mut().enumerate() {
        let append_index = selected(append_selection, i)?;
        let list_index = selected(list_selection, append_index)?;
        if !is_valid(list_validity, list_index)? {
            continue;
        }
        let (list_offset, list_length) = list_entry(list_entries, list_index, WHAT)?;
        if list_length == 0 {
            continue;
        }
        let list_length = as_usize(list_length, "scatter_within_string: the list length")?;
        let list_offset = as_usize(list_offset, "scatter_within_string: the list offset")?;
        let mask_size = validity_size_in_bytes(list_length);
        let data_size = list_length
            .checked_mul(size_of::<u32>())
            .ok_or_else(|| format!("{WHAT}: the element data size overflows"))?;

        // SAFETY: the heap-size pass reserved the mask, the lengths and the
        // characters, in that order.
        let mask = unsafe { heap_bytes_mut(*heap_slot, mask_size) };
        set_all_valid(mask, list_length);
        let data_start = *heap_slot + mask_size as u64;
        // SAFETY: the lengths follow the mask in the same reservation.
        let data = unsafe { heap_bytes_mut(data_start, data_size) };
        let mut characters = data_start + data_size as u64;

        for child_i in 0..list_length {
            let source_index = selected(source_selection, list_offset + child_i)?;
            if !is_valid(source_validity, source_index)? {
                set_element_invalid(mask, child_i, WHAT)?;
                continue;
            }
            let value = entry(source_entries, source_index, STRING_SIZE, WHAT)?;
            let length = u32::from_le_bytes(
                value[..4]
                    .try_into()
                    .map_err(|_| format!("{WHAT}: a string_t is shorter than its length"))?,
            );
            data[child_i * size_of::<u32>()..][..size_of::<u32>()]
                .copy_from_slice(&length.to_le_bytes());
            // SAFETY: the source string owns `length` bytes at its address.
            let string =
                unsafe { heap_bytes(load_u64(value, STRING_HEADER_SIZE)?, length as usize) };
            // SAFETY: the heap-size pass counted this element's characters.
            characters = unsafe { write_heap(characters, string) };
        }
        *heap_slot = characters;
    }
    Ok(())
}

/// Scatters a `STRUCT` into each list element: only the element validity, which
/// is the struct's own, is written here.
#[allow(clippy::too_many_arguments)]
pub fn scatter_within_struct(
    list_entries: &[u8],
    list_selection: &[u32],
    list_validity: &[u64],
    source_selection: &[u32],
    source_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    heap_locations: &mut [u64],
) -> Result<(), String> {
    const WHAT: &str = "scatter_within_struct";
    let heap = rows_mut(heap_locations, count, WHAT)?;

    for (i, heap_slot) in heap.iter_mut().enumerate() {
        let append_index = selected(append_selection, i)?;
        let list_index = selected(list_selection, append_index)?;
        if !is_valid(list_validity, list_index)? {
            continue;
        }
        let (list_offset, list_length) = list_entry(list_entries, list_index, WHAT)?;
        if list_length == 0 {
            continue;
        }
        let list_length = as_usize(list_length, "scatter_within_struct: the list length")?;
        let list_offset = as_usize(list_offset, "scatter_within_struct: the list offset")?;
        let mask_size = validity_size_in_bytes(list_length);

        // SAFETY: the heap-size pass reserved the mask for this row's elements.
        let mask = unsafe { heap_bytes_mut(*heap_slot, mask_size) };
        set_all_valid(mask, list_length);
        *heap_slot += mask_size as u64;

        for child_i in 0..list_length {
            let source_index = selected(source_selection, list_offset + child_i)?;
            if !is_valid(source_validity, source_index)? {
                set_element_invalid(mask, child_i, WHAT)?;
            }
        }
    }
    Ok(())
}

/// Scatters nested `LIST`/`ARRAY` lengths into each list element.
#[allow(clippy::too_many_arguments)]
pub fn scatter_within_collection(
    list_entries: &[u8],
    list_selection: &[u32],
    list_validity: &[u64],
    child_entries: &[u8],
    child_selection: &[u32],
    child_validity: &[u64],
    append_selection: &[u32],
    count: usize,
    heap_locations: &mut [u64],
) -> Result<(), String> {
    const WHAT: &str = "scatter_within_collection";
    let heap = rows_mut(heap_locations, count, WHAT)?;

    for (i, heap_slot) in heap.iter_mut().enumerate() {
        let append_index = selected(append_selection, i)?;
        let list_index = selected(list_selection, append_index)?;
        if !is_valid(list_validity, list_index)? {
            continue;
        }
        let (list_offset, list_length) = list_entry(list_entries, list_index, WHAT)?;
        if list_length == 0 {
            continue;
        }
        let list_length = as_usize(list_length, "scatter_within_collection: the list length")?;
        let list_offset = as_usize(list_offset, "scatter_within_collection: the list offset")?;
        let mask_size = validity_size_in_bytes(list_length);
        let data_size = list_length
            .checked_mul(size_of::<u64>())
            .ok_or_else(|| format!("{WHAT}: the element data size overflows"))?;

        // SAFETY: the heap-size pass reserved the mask and the child lengths.
        let mask = unsafe { heap_bytes_mut(*heap_slot, mask_size) };
        set_all_valid(mask, list_length);
        let data_start = *heap_slot + mask_size as u64;
        // SAFETY: the child lengths follow the mask in the same reservation.
        let data = unsafe { heap_bytes_mut(data_start, data_size) };
        *heap_slot = data_start + data_size as u64;

        for child_i in 0..list_length {
            let child_index = selected(child_selection, list_offset + child_i)?;
            if !is_valid(child_validity, child_index)? {
                set_element_invalid(mask, child_i, WHAT)?;
                continue;
            }
            let (_, child_length) = list_entry(child_entries, child_index, WHAT)?;
            data[child_i * size_of::<u64>()..][..size_of::<u64>()]
                .copy_from_slice(&child_length.to_le_bytes());
        }
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Gather
// ---------------------------------------------------------------------------

/// Gathers a fixed-size column out of the rows.
#[allow(clippy::too_many_arguments)]
pub fn gather_fixed(
    row_locations: &[u64],
    scan_selection: &[u32],
    target_selection: &[u32],
    scan_count: usize,
    target_entries: &mut [u8],
    target_validity: &mut [u64],
    entry_size: usize,
    offset_in_row: usize,
    column_index: usize,
    can_have_null: bool,
) -> Result<(), String> {
    const WHAT: &str = "gather_fixed";
    for i in 0..scan_count {
        let source_index = selected(scan_selection, i)?;
        let target_index = selected(target_selection, i)?;
        let address = *row_locations
            .get(source_index)
            .ok_or_else(|| format!("{WHAT}: the row-location array has no row {source_index}"))?;
        // SAFETY: `address` is a row of the block being scanned.
        let row = unsafe { row_bytes(address, offset_in_row + entry_size) };
        let value = row
            .get(offset_in_row..offset_in_row + entry_size)
            .ok_or_else(|| format!("{WHAT}: column {column_index} does not fit in the row"))?;

        let start = target_index
            .checked_mul(entry_size)
            .ok_or_else(|| format!("{WHAT}: target row {target_index} overflows"))?;
        let destination = target_entries
            .get_mut(start..start + entry_size)
            .ok_or_else(|| {
                format!("{WHAT}: target row {target_index} is outside the entry array")
            })?;
        destination.copy_from_slice(value);

        if can_have_null && !column_is_valid(row, column_index)? {
            set_target_invalid(target_validity, target_index)?;
        }
    }
    Ok(())
}

/// Gathers a `VARCHAR` column: the sixteen bytes of the row's `string_t`, whose
/// pointer still points into the row block.
#[allow(clippy::too_many_arguments)]
pub fn gather_string(
    row_locations: &[u64],
    scan_selection: &[u32],
    target_selection: &[u32],
    scan_count: usize,
    target_entries: &mut [u8],
    target_validity: &mut [u64],
    offset_in_row: usize,
    column_index: usize,
    can_have_null: bool,
) -> Result<(), String> {
    const WHAT: &str = "gather_string";
    for i in 0..scan_count {
        let source_index = selected(scan_selection, i)?;
        let target_index = selected(target_selection, i)?;
        let address = *row_locations
            .get(source_index)
            .ok_or_else(|| format!("{WHAT}: the row-location array has no row {source_index}"))?;
        // SAFETY: `address` is a row of the block being scanned.
        let row = unsafe { row_bytes(address, offset_in_row + STRING_SIZE) };
        let value = row
            .get(offset_in_row..offset_in_row + STRING_SIZE)
            .ok_or_else(|| format!("{WHAT}: column {column_index} does not fit in the row"))?;

        let start = target_index
            .checked_mul(STRING_SIZE)
            .ok_or_else(|| format!("{WHAT}: target row {target_index} overflows"))?;
        let destination = target_entries
            .get_mut(start..start + STRING_SIZE)
            .ok_or_else(|| {
                format!("{WHAT}: target row {target_index} is outside the entry array")
            })?;
        destination.copy_from_slice(value);

        if can_have_null && !column_is_valid(row, column_index)? {
            set_target_invalid(target_validity, target_index)?;
        }
    }
    Ok(())
}

/// Gathers a `LIST` or `ARRAY` column: the list entry, and in `heap_locations`
/// the address its children are stored at.
///
/// Returns the list size after the gathered entries.
#[allow(clippy::too_many_arguments)]
pub fn gather_collection(
    row_locations: &[u64],
    scan_selection: &[u32],
    target_selection: &[u32],
    scan_count: usize,
    target_entries: &mut [u8],
    target_validity: &mut [u64],
    heap_locations: &mut [u64],
    offset_in_row: usize,
    column_index: usize,
    can_have_null: bool,
    list_size_before: u64,
) -> Result<u64, String> {
    const WHAT: &str = "gather_collection";
    let heap = rows_mut(heap_locations, scan_count, WHAT)?;
    let mut target_list_offset = list_size_before;

    for (i, heap_slot) in heap.iter_mut().enumerate() {
        let source_index = selected(scan_selection, i)?;
        let target_index = selected(target_selection, i)?;
        let address = *row_locations
            .get(source_index)
            .ok_or_else(|| format!("{WHAT}: the row-location array has no row {source_index}"))?;
        // SAFETY: `address` is a row of the block being scanned.
        let row = unsafe { row_bytes(address, offset_in_row + 8) };

        if can_have_null && !column_is_valid(row, column_index)? {
            set_target_invalid(target_validity, target_index)?;
            continue;
        }
        let heap_address = load_u64(row, offset_in_row)?;
        // SAFETY: the heap address came out of this row's header, where the
        // scatter wrote the list length.
        let length = load_u64(unsafe { heap_bytes(heap_address, 8) }, 0)?;

        let start = target_index
            .checked_mul(LIST_ENTRY_SIZE)
            .ok_or_else(|| format!("{WHAT}: target row {target_index} overflows"))?;
        let destination = target_entries
            .get_mut(start..start + LIST_ENTRY_SIZE)
            .ok_or_else(|| {
                format!("{WHAT}: target row {target_index} is outside the entry array")
            })?;
        destination[..8].copy_from_slice(&target_list_offset.to_le_bytes());
        destination[8..].copy_from_slice(&length.to_le_bytes());
        *heap_slot = heap_address + 8;
        target_list_offset += length;
    }
    Ok(target_list_offset)
}

/// Gathers a `STRUCT`: its validity bits and the address of each struct's own
/// row, for the recursion to read.
#[allow(clippy::too_many_arguments)]
pub fn gather_struct(
    row_locations: &[u64],
    scan_selection: &[u32],
    target_selection: &[u32],
    scan_count: usize,
    target_validity: &mut [u64],
    offset_in_row: usize,
    column_index: usize,
    can_have_null: bool,
    struct_row_locations: &mut [u64],
) -> Result<(), String> {
    const WHAT: &str = "gather_struct";
    let struct_rows = struct_row_locations
        .get_mut(..row_locations.len())
        .ok_or_else(|| format!("{WHAT}: the sub-row array is shorter than the row array"))?;

    for i in 0..scan_count {
        let source_index = selected(scan_selection, i)?;
        let target_index = selected(target_selection, i)?;
        let address = *row_locations
            .get(source_index)
            .ok_or_else(|| format!("{WHAT}: the row-location array has no row {source_index}"))?;
        // SAFETY: `address` is a row of the block being scanned.
        let row = unsafe { row_bytes(address, offset_in_row) };
        if can_have_null && !column_is_valid(row, column_index)? {
            set_target_invalid(target_validity, target_index)?;
        }
        struct_rows[source_index] = address + offset_in_row as u64;
    }
    Ok(())
}

/// Gathers a fixed-size column out of the heap space of each list element.
///
/// `heap_locations` holds, per scanned row, the address the parent list gather
/// left its children at; `list_entries` and `list_validity` describe the parent
/// list vector, which the gathered rows are indexed into by `target_selection`.
#[allow(clippy::too_many_arguments)]
pub fn gather_within_fixed(
    heap_locations: &[u64],
    scan_count: usize,
    target_selection: &[u32],
    target_entries: &mut [u8],
    target_validity: &mut [u64],
    list_entries: &[u8],
    list_validity: &[u64],
    entry_size: usize,
    list_size_before: u64,
) -> Result<(), String> {
    const WHAT: &str = "gather_within_fixed";
    let mut target_offset = as_usize(list_size_before, "the list size")?;

    for (i, &heap_address) in heap_locations.iter().take(scan_count).enumerate() {
        let target_index = selected(target_selection, i)?;
        if !is_valid(list_validity, target_index)? {
            continue;
        }
        let (_, list_length) = list_entry(list_entries, target_index, WHAT)?;
        if list_length == 0 {
            continue;
        }
        let list_length = as_usize(list_length, "gather_within_fixed: the list length")?;
        let mask_size = validity_size_in_bytes(list_length);
        let data_size = list_length
            .checked_mul(entry_size)
            .ok_or_else(|| format!("{WHAT}: the element data size overflows"))?;

        // SAFETY: the heap address came out of a row the parent list gathered.
        let mask = unsafe { heap_bytes(heap_address, mask_size) };
        let data_start = heap_address + mask_size as u64;
        // SAFETY: the element data follows the mask in the same reservation.
        let data = unsafe { heap_bytes(data_start, data_size) };

        for child_i in 0..list_length {
            let target_child = target_offset
                .checked_add(child_i)
                .ok_or_else(|| format!("{WHAT}: the target index overflows"))?;
            if !element_is_valid(mask, child_i, WHAT)? {
                set_target_invalid(target_validity, target_child)?;
                continue;
            }
            let start = target_child
                .checked_mul(entry_size)
                .ok_or_else(|| format!("{WHAT}: the target index overflows"))?;
            let destination = target_entries
                .get_mut(start..start + entry_size)
                .ok_or_else(|| {
                    format!("{WHAT}: target row {target_child} is outside the entry array")
                })?;
            destination.copy_from_slice(&data[child_i * entry_size..][..entry_size]);
        }
        target_offset += list_length;
    }
    Ok(())
}

/// Gathers a `VARCHAR` column out of the heap space of each list element.
#[allow(clippy::too_many_arguments)]
pub fn gather_within_string(
    heap_locations: &[u64],
    scan_count: usize,
    target_selection: &[u32],
    target_entries: &mut [u8],
    target_validity: &mut [u64],
    list_entries: &[u8],
    list_validity: &[u64],
    list_size_before: u64,
) -> Result<(), String> {
    const WHAT: &str = "gather_within_string";
    let mut target_offset = as_usize(list_size_before, "the list size")?;

    for (i, &heap_address) in heap_locations.iter().take(scan_count).enumerate() {
        let target_index = selected(target_selection, i)?;
        if !is_valid(list_validity, target_index)? {
            continue;
        }
        let (_, list_length) = list_entry(list_entries, target_index, WHAT)?;
        if list_length == 0 {
            continue;
        }
        let list_length = as_usize(list_length, "gather_within_string: the list length")?;
        let mask_size = validity_size_in_bytes(list_length);
        // SAFETY: the heap address came out of a row the parent list gathered.
        let mask = unsafe { heap_bytes(heap_address, mask_size) };
        let data_start = heap_address + mask_size as u64;
        // SAFETY: one 4-byte length per element follows the mask.
        let data = unsafe { heap_bytes(data_start, list_length * size_of::<u32>()) };
        let mut characters = data_start + (list_length * size_of::<u32>()) as u64;

        for child_i in 0..list_length {
            let target_child = target_offset
                .checked_add(child_i)
                .ok_or_else(|| format!("{WHAT}: the target index overflows"))?;
            if !element_is_valid(mask, child_i, WHAT)? {
                set_target_invalid(target_validity, target_child)?;
                continue;
            }
            let start = child_i * size_of::<u32>();
            let length = u32::from_le_bytes(
                data[start..start + size_of::<u32>()]
                    .try_into()
                    .map_err(|_| format!("{WHAT}: a length entry is short"))?,
            );
            let length_usize = length as usize;
            // SAFETY: the element's characters are stored at the heap cursor.
            let value = unsafe { heap_bytes(characters, length_usize) };
            let destination = target_entries
                .get_mut(target_child * STRING_SIZE..target_child * STRING_SIZE + STRING_SIZE)
                .ok_or_else(|| {
                    format!("{WHAT}: target row {target_child} is outside the entry array")
                })?;
            store_string(destination, length, value, characters)?;
            characters += length_usize as u64;
        }
        target_offset += list_length;
    }
    Ok(())
}

/// Writes the `string_t` that `string_t(const char *, uint32_t)` would produce:
/// inlined when it fits, otherwise a prefix and a pointer.
fn store_string(
    destination: &mut [u8],
    length: u32,
    value: &[u8],
    address: u64,
) -> Result<(), String> {
    destination[..4].copy_from_slice(&length.to_le_bytes());
    if length as usize <= STRING_INLINE_LENGTH {
        destination[4..].fill(0);
        destination[4..4 + value.len()].copy_from_slice(value);
        return Ok(());
    }
    let prefix = value
        .get(..4)
        .ok_or_else(|| "store_string: the string is too short for its prefix".to_string())?;
    destination[4..8].copy_from_slice(prefix);
    destination[8..].copy_from_slice(&address.to_le_bytes());
    Ok(())
}

/// Gathers a `STRUCT` out of the heap space of each list element: only the
/// element validity is copied here.
pub fn gather_within_struct(
    heap_locations: &[u64],
    scan_count: usize,
    target_selection: &[u32],
    target_validity: &mut [u64],
    list_entries: &[u8],
    list_validity: &[u64],
    list_size_before: u64,
) -> Result<(), String> {
    const WHAT: &str = "gather_within_struct";
    let mut target_offset = as_usize(list_size_before, "the list size")?;

    for (i, &heap_address) in heap_locations.iter().take(scan_count).enumerate() {
        let target_index = selected(target_selection, i)?;
        if !is_valid(list_validity, target_index)? {
            continue;
        }
        let (_, list_length) = list_entry(list_entries, target_index, WHAT)?;
        if list_length == 0 {
            continue;
        }
        let list_length = as_usize(list_length, "gather_within_struct: the list length")?;
        let mask_size = validity_size_in_bytes(list_length);
        // SAFETY: the heap address came out of a row the parent list gathered.
        let mask = unsafe { heap_bytes(heap_address, mask_size) };

        for child_i in 0..list_length {
            let target_child = target_offset
                .checked_add(child_i)
                .ok_or_else(|| format!("{WHAT}: the target index overflows"))?;
            if !element_is_valid(mask, child_i, WHAT)? {
                set_target_invalid(target_validity, target_child)?;
            }
        }
        target_offset += list_length;
    }
    Ok(())
}

/// Gathers nested `LIST`/`ARRAY` lengths out of the heap space of each list
/// element.
///
/// Returns the target list size and the child list size after the gathered
/// entries, which the caller passes to `ListVector::Reserve`.
#[allow(clippy::too_many_arguments)]
pub fn gather_within_collection(
    heap_locations: &[u64],
    scan_count: usize,
    target_selection: &[u32],
    target_entries: &mut [u8],
    combined_entries: &mut [u8],
    target_validity: &mut [u64],
    list_entries: &[u8],
    list_validity: &[u64],
    list_size_before: u64,
    child_list_size_before: u64,
) -> Result<u64, String> {
    const WHAT: &str = "gather_within_collection";
    let mut target_offset = as_usize(list_size_before, "the list size")?;
    let mut target_child_offset = as_usize(child_list_size_before, "the child list size")?;

    for (i, &heap_address) in heap_locations.iter().take(scan_count).enumerate() {
        let target_index = selected(target_selection, i)?;
        if !is_valid(list_validity, target_index)? {
            continue;
        }
        let combined = combined_entries
            .get_mut(target_index * LIST_ENTRY_SIZE..(target_index + 1) * LIST_ENTRY_SIZE)
            .ok_or_else(|| {
                format!("{WHAT}: combined row {target_index} is outside the entry array")
            })?;
        let combined_offset = target_child_offset;
        combined[..8].copy_from_slice(&combined_offset.to_le_bytes());

        let (_, list_length) = list_entry(list_entries, target_index, WHAT)?;
        if list_length == 0 {
            combined[8..].copy_from_slice(&0u64.to_le_bytes());
            continue;
        }
        let list_length = as_usize(list_length, "gather_within_collection: the list length")?;
        let mask_size = validity_size_in_bytes(list_length);
        let data_size = list_length
            .checked_mul(size_of::<u64>())
            .ok_or_else(|| format!("{WHAT}: the element data size overflows"))?;

        // SAFETY: the heap address came out of a row the parent list gathered.
        let mask = unsafe { heap_bytes(heap_address, mask_size) };
        let data_start = heap_address + mask_size as u64;
        // SAFETY: the child lengths follow the mask in the same reservation.
        let data = unsafe { heap_bytes(data_start, data_size) };

        for child_i in 0..list_length {
            let target_child = target_offset
                .checked_add(child_i)
                .ok_or_else(|| format!("{WHAT}: the target index overflows"))?;
            if !element_is_valid(mask, child_i, WHAT)? {
                set_target_invalid(target_validity, target_child)?;
                continue;
            }
            let child_length = load_u64(data, child_i * size_of::<u64>())?;
            let start = target_child
                .checked_mul(LIST_ENTRY_SIZE)
                .ok_or_else(|| format!("{WHAT}: the target index overflows"))?;
            let destination = target_entries
                .get_mut(start..start + LIST_ENTRY_SIZE)
                .ok_or_else(|| {
                    format!("{WHAT}: target row {target_child} is outside the entry array")
                })?;
            destination[..8].copy_from_slice(&target_child_offset.to_le_bytes());
            destination[8..].copy_from_slice(&child_length.to_le_bytes());
            target_child_offset += as_usize(
                child_length,
                "gather_within_collection: the child list length",
            )?;
        }
        combined[8..].copy_from_slice(&(target_child_offset - combined_offset).to_le_bytes());
        target_offset += list_length;
    }
    let _ = target_offset;
    Ok(target_child_offset as u64)
}
