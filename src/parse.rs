use core::{ffi::CStr, mem, ptr::NonNull, str};

#[derive(Debug)]
pub(crate) struct ByteParser<'a>(&'a [u32]);

#[derive(Debug)]
pub(crate) enum ParseStrError {
    NotNullTerminated,
    Utf8Error(str::Utf8Error),
}

/// Attempts to convert a byte slice representing a `CStr` into a proper `str`
pub(crate) fn parse_str(bytes: &[u8]) -> Result<&str, ParseStrError> {
    let c_str =
        CStr::from_bytes_until_nul(bytes).map_err(|_err| ParseStrError::NotNullTerminated)?;
    c_str.to_str().map_err(ParseStrError::Utf8Error)
}

/// Coerces a `u32` slice into a `u8` slice
fn u32_to_u8_slice(bytes: &[u32]) -> &[u8] {
    unsafe {
        NonNull::slice_from_raw_parts(
            NonNull::from(bytes).as_non_null_ptr().cast(),
            mem::size_of_val(bytes) / mem::size_of::<u8>(),
        )
        .as_ref()
    }
}

pub(crate) fn u8_to_u32_slice(bytes: &[u8]) -> Option<&[u32]> {
    let ptr = NonNull::from(bytes).as_non_null_ptr().cast::<u32>();
    if ptr.as_ptr().is_aligned() && mem::size_of_val(bytes) % mem::size_of::<u32>() == 0 {
        Some(unsafe {
            NonNull::slice_from_raw_parts(ptr, mem::size_of_val(bytes) / mem::size_of::<u32>())
                .as_ref()
        })
    } else {
        None
    }
}

impl<'a> ByteParser<'a> {
    /// Creates a parser wrapping around the provided bytes
    pub(crate) const fn new(bytes: &'a [u32]) -> Self {
        Self(bytes)
    }

    /// Extracts a big-endian `u32` from a sequence of bytes
    ///
    /// Returns `None` if there are not enough bytes to form a `u32`
    pub(crate) fn consume_u32_be(&mut self) -> Option<u32> {
        self.0.take_first().map(|x| u32::from_be(*x))
    }

    /// Extracts a null-terminated string (as a Rust `str`) from a sequence of bytes
    ///
    /// Returns any errors if string conversion fails
    #[expect(clippy::unwrap_in_result)]
    pub(crate) fn consume_str(&mut self) -> Result<&str, ParseStrError> {
        let string = parse_str(u32_to_u8_slice(self.0))?;
        #[expect(clippy::expect_used)]
        self.0
            .take(
                ..(string
                    .len()
                    .checked_add(1)
                    .expect("The null byte should have already been found"))
                .div_ceil(mem::size_of::<u32>()),
            )
            .expect("CStr should not go past the end of the slice");
        Ok(string)
    }

    /// Extracts a slice of bytes from a sequence of bytes
    ///
    /// Returns `None` if insufficient bytes are available
    pub(crate) fn consume_bytes(&mut self, len: usize) -> Option<&[u8]> {
        self.0.take(..len.div_ceil(mem::size_of::<u32>())).map(|x| {
            #[expect(clippy::expect_used)]
            u32_to_u8_slice(x)
                .get(..len)
                .expect("Should have at least `len` elements")
        })
    }

    /// Returns whether or not the bytes are exhausted
    pub(crate) const fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}
