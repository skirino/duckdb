//! Raw buffer copies for the buffer module.

use super::idx_t;

/// Copies `len` bytes from `src` (starting at `src_offset`) into `dst`
/// (starting at `dst_offset`), with bounds checks. Overlapping spans are
/// refused.
pub fn copy_range(
    dst: &mut [u8],
    dst_offset: idx_t,
    src: &[u8],
    src_offset: idx_t,
    len: idx_t,
) -> Result<(), String> {
    let len = len as usize;
    let dst_offset = dst_offset as usize;
    let src_offset = src_offset as usize;

    let dst_end = dst_offset
        .checked_add(len)
        .ok_or_else(|| "block_copy: destination range overflows".to_string())?;
    if dst_end > dst.len() {
        return Err(format!(
            "block_copy: destination buffer holds {} bytes but the copy needs {dst_end}",
            dst.len()
        ));
    }
    let src_end = src_offset
        .checked_add(len)
        .ok_or_else(|| "block_copy: source range overflows".to_string())?;
    if src_end > src.len() {
        return Err(format!(
            "block_copy: source buffer holds {} bytes but the copy needs {src_end}",
            src.len()
        ));
    }

    // Refuse overlapping spans (same allocation).
    let dst_ptr = dst.as_ptr() as usize + dst_offset;
    let src_ptr = src.as_ptr() as usize + src_offset;
    if dst_ptr < src_ptr + len && src_ptr < dst_ptr + len {
        return Err("block_copy: overlapping source and destination spans".to_string());
    }

    dst[dst_offset..dst_end].copy_from_slice(&src[src_offset..src_end]);
    Ok(())
}

/// Copies `len` bytes from the start of `src` into the start of `dst`.
pub fn copy_block(dst: &mut [u8], src: &[u8], len: idx_t) -> Result<(), String> {
    copy_range(dst, 0, src, 0, len)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copy_block_copies_exact_bytes() {
        let src = [1u8, 2, 3, 4];
        let mut dst = [0u8; 4];
        assert!(copy_block(&mut dst, &src, 4).is_ok());
        assert_eq!(dst, src);
    }

    #[test]
    fn copy_block_refuses_short_source() {
        let src = [1u8, 2];
        let mut dst = [0u8; 4];
        assert!(copy_block(&mut dst, &src, 4).is_err());
    }

    #[test]
    fn copy_range_uses_offsets() {
        let src = [10u8, 20, 30, 40];
        let mut dst = [0u8; 4];
        assert!(copy_range(&mut dst, 1, &src, 2, 2).is_ok());
        assert_eq!(dst, [0, 30, 40, 0]);
    }

    // Overlapping spans are refused; exercised from C++ where the same buffer
    // can be passed through two slices.
}
