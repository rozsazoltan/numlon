use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;

pub fn wide_null(value: impl AsRef<OsStr>) -> Vec<u16> {
    value.as_ref().encode_wide().chain(std::iter::once(0)).collect()
}

pub fn str_wide_null(value: &str) -> Vec<u16> {
    OsStr::new(value).encode_wide().chain(std::iter::once(0)).collect()
}

pub fn copy_wide_truncated<const N: usize>(target: &mut [u16; N], value: &str) {
    target.fill(0);
    let encoded = str_wide_null(value);
    let max = target.len().saturating_sub(1);
    let count = encoded.len().saturating_sub(1).min(max);
    target[..count].copy_from_slice(&encoded[..count]);
}
