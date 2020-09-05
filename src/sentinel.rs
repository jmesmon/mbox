//! Sentinel-terminated types.

use libc::{strlen, c_char};
use stable_deref_trait::StableDeref;

use std::mem::{forget, size_of};
use std::str::Utf8Error;
use std::ops::{Deref, DerefMut};
#[cfg(feature="std")] use std::ffi::CStr;
use std::default::Default;
use std::ptr::{null, null_mut, write, copy_nonoverlapping};
use std::convert::{AsRef, AsMut};
use std::borrow::{Borrow, BorrowMut};
use std::iter::once;
use std::hash::{Hash, Hasher};

use internal::gen_malloc;
use mbox::MBox;

#[cfg(test)] use internal::DropCounter;

/// Implemented for types which has a sentinel value.
pub trait Sentinel: Eq {
    /// Obtains the sentinel value. This method should return a constant.
    fn sentinel() -> Self;
}

impl<T> Sentinel for *const T {
    fn sentinel() -> Self {
        null()
    }
}

impl<T> Sentinel for *mut T {
    fn sentinel() -> Self {
        null_mut()
    }
}

impl<T: Eq> Sentinel for Option<T> {
    fn sentinel() -> Self {
        None
    }
}

macro_rules! impl_zero_for_sentinel {
    ($($ty:ty)+) => {
        $(impl Sentinel for $ty {
            fn sentinel() -> Self {
                0
            }
        })+
    }
}

impl_zero_for_sentinel!(u8 i8 u16 i16 u32 i32 u64 i64 usize isize);

/// A `malloc`-backed array with an explicit sentinel at the end.
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct MArray<T: Sentinel>(MBox<[T]>);

/// A `malloc`-backed null-terminated string (similar to `CString`).
#[derive(Clone, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct MString(MBox<str>);

impl<T: Sentinel> MArray<T> {
    /// Constructs a new malloc-backed slice from a pointer to the null-terminated array.
    pub unsafe fn from_raw(base: *mut T) -> MArray<T> {
        assert!(size_of::<T>() != 0, "zero-sized arrays cannot be null-terminated");
        let mut len = 0;
        while *base.offset(len) != T::sentinel() {
            len += 1;
        }
        MArray(MBox::from_raw_parts(base, (len + 1) as usize))
    }

    /// Converts into an `MBox` including the sentinel.
    pub fn into_mbox_with_sentinel(self) -> MBox<[T]> {
        self.0
    }

    /// Converts into an `MBox` excluding the sentinel.
    pub fn into_mbox(mut self) -> MBox<[T]> {
        let ptr = (*self.0).as_mut_ptr();
        let len = self.0.len() - 1;
        forget(self);
        unsafe { MBox::from_raw_parts(ptr, len) }
    }
}

impl<T: Sentinel + Clone> MArray<T> {
    /// Creates a null-terminated array from the clone of a slice.
    pub fn from_slice(slice: &[T]) -> MArray<T> {
        MArray(slice.iter().cloned().chain(once(T::sentinel())).collect())
    }
}

impl MString {
    /// Constructs a new malloc-backed string from a null-terminated C string. The string must be
    /// valid UTF-8.
    pub unsafe fn from_raw_unchecked(base: *mut c_char) -> MString {
        let len = strlen(base);
        MString(MBox::from_raw_utf8_parts_unchecked(base as *mut u8, len + 1))
    }

    /// Constructs a new malloc-backed string from a null-terminated C string. Errors with
    /// `Utf8Error` if the string is not in valid UTF-8.
    pub unsafe fn from_raw(base: *mut c_char) -> Result<MString, Utf8Error> {
        let len = strlen(base);
        let mbox = MBox::from_raw_utf8_parts(base as *mut u8, len + 1)?;
        Ok(MString(mbox))
    }

    pub fn into_bytes(self) -> MArray<u8> {
        MArray(self.0.into_bytes())
    }

    /// Converts into an `MBox` including the sentinel.
    pub fn into_mbox_with_sentinel(self) -> MBox<str> {
        self.0
    }

    /// Converts into an `MBox` excluding the sentinel.
    pub fn into_mbox(self) -> MBox<str> {
        unsafe {
            MBox::from_utf8_unchecked(self.into_bytes().into_mbox())
        }
    }

    /// Converts to a C string. This allows users to borrow an MString in FFI code.
    #[cfg(all(feature="std"))]
    pub fn as_c_str(&self) -> &CStr {
        unsafe {
            CStr::from_bytes_with_nul_unchecked(self.0.as_bytes())
        }
    }

    /// Obtains the raw bytes including the sentinel.
    pub fn as_bytes_with_sentinel(&self) -> &[u8] {
        self.0.as_bytes()
    }

    /// Creates a null-terminated string from the clone of a string.
    pub fn from_str(string: &str) -> MString {
        unsafe {
            let len = string.len();
            let ptr = gen_malloc(len + 1);
            copy_nonoverlapping(string.as_ptr(), ptr, len);
            *ptr.add(len) = 0;
            MString(MBox::from_raw_utf8_parts_unchecked(ptr, len + 1))
        }
    }
}

impl<T: Sentinel> Deref for MArray<T> {
    type Target = [T];
    fn deref(&self) -> &[T] {
        let actual_len = self.0.len() - 1;
        &self.0[.. actual_len]
    }
}

impl Deref for MString {
    type Target = str;
    fn deref(&self) -> &str {
        let actual_len = self.0.len() - 1;
        &self.0[.. actual_len]

    }
}

unsafe impl<T: Sentinel> StableDeref for MArray<T> {}
unsafe impl StableDeref for MString {}

impl<T: Sentinel> DerefMut for MArray<T> {
    fn deref_mut(&mut self) -> &mut [T] {
        let actual_len = self.0.len() - 1;
        &mut self.0[.. actual_len]

    }
}

impl Hash for MString {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

impl<T: Sentinel + Hash> Hash for MArray<T> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.deref().hash(state);
    }
}

impl DerefMut for MString {
    fn deref_mut(&mut self) -> &mut str {
        let actual_len = self.0.len() - 1;
        &mut self.0[.. actual_len]

    }
}

impl<T: Sentinel> Default for MArray<T> {
    fn default() -> Self {
        unsafe {
            let arr = gen_malloc(1);
            write(arr, T::sentinel());
            MArray(MBox::from_raw_parts(arr, 1))
        }
    }
}

impl Default for MString {
    fn default() -> Self {
        unsafe {
            let arr = gen_malloc(1);
            *arr = 0;
            MString(MBox::from_raw_utf8_parts_unchecked(arr, 1))
        }
    }
}

impl<T: Sentinel> AsRef<[T]> for MArray<T> {
    fn as_ref(&self) -> &[T] {
        self
    }
}

impl<T: Sentinel> AsMut<[T]> for MArray<T> {
    fn as_mut(&mut self) -> &mut [T] {
        self
    }
}

impl<T: Sentinel> Borrow<[T]> for MArray<T> {
    fn borrow(&self) -> &[T] {
        self
    }
}

impl<T: Sentinel> BorrowMut<[T]> for MArray<T> {
    fn borrow_mut(&mut self) -> &mut [T] {
        self
    }
}

impl AsRef<str> for MString {
    fn as_ref(&self) -> &str {
        self
    }
}

impl AsMut<str> for MString {
    fn as_mut(&mut self) -> &mut str {
        self
    }
}

impl Borrow<str> for MString {
    fn borrow(&self) -> &str {
        self
    }
}

impl BorrowMut<str> for MString {
    fn borrow_mut(&mut self) -> &mut str {
        self
    }
}

#[cfg(feature="std")]
impl AsRef<CStr> for MString {
    fn as_ref(&self) -> &CStr {
        self.as_c_str()
    }
}

#[test]
fn test_array() {
    unsafe {
        let src = gen_malloc::<u8>(6);
        *src.offset(0) = 56;
        *src.offset(1) = 18;
        *src.offset(2) = 200;
        *src.offset(3) = 0;
        *src.offset(4) = 105;
        *src.offset(5) = 0;

        let mut array = MArray::from_raw(src);
        assert_eq!(&*array, &[56u8, 18, 200]);
        array[1] = 19;
        assert_eq!(*src.offset(1), 19);
    }
}

#[test]
fn test_array_with_drop() {
    let counter = DropCounter::default();
    unsafe {
        let src = gen_malloc::<Option<DropCounter>>(3);
        write(src.offset(0), Some(counter.clone()));
        write(src.offset(1), Some(counter.clone()));
        write(src.offset(2), None);

        counter.assert_eq(0);
        let array = MArray::from_raw(src);
        assert_eq!(array.len(), 2);
        array[0].as_ref().unwrap().assert_eq(0);
        array[1].as_ref().unwrap().assert_eq(0);
    }
    counter.assert_eq(2);
}

#[test]
fn test_string() {
    unsafe {
        let src = gen_malloc::<c_char>(5);
        *src.offset(0) = 0x61;
        *src.offset(1) = -0x19;
        *src.offset(2) = -0x6c;
        *src.offset(3) = -0x4e;
        *src.offset(4) = 0;

        let string = MString::from_raw_unchecked(src);
        assert_eq!(&*string, "a甲");
    }
}

#[test]
fn test_non_utf8_string() {
    unsafe {
        let src = gen_malloc::<c_char>(2);
        *src.offset(0) = -1;
        *src.offset(1) = 0;

        let string = MString::from_raw(src);
        assert!(string.is_err());

        let src2 = gen_malloc::<c_char>(2);
        *src2.offset(0) = 1;
        *src2.offset(1) = 0;

        let string2 = MString::from_raw(src2);
        assert_eq!(string2.unwrap().deref(), "\u{1}");
    }
}

#[cfg(feature="std")]
#[test]
fn test_c_str() {
    unsafe {
        let src = gen_malloc::<c_char>(2);
        *src.offset(0) = 1;
        *src.offset(1) = 0;
        let string = MString::from_raw_unchecked(src);
        let c_str = string.as_c_str();
        assert_eq!(c_str, CStr::from_ptr(b"\x01\x00".as_ptr() as *const c_char));
    }
}

#[test]
fn test_array_into_mbox() {
    let first = MArray::from_slice(&[123, 456, 789]);
    let second = first.clone();

    assert_eq!(&*first.into_mbox(), &[123, 456, 789]);
    assert_eq!(&*second.into_mbox_with_sentinel(), &[123, 456, 789, 0]);
}

#[test]
fn test_string_into_mbox() {
    let first = MString::from_str("abcde");
    let second = first.clone();

    assert_eq!(first.as_bytes(), b"abcde");
    assert_eq!(&*first.into_mbox(), "abcde");
    assert_eq!(second.as_bytes_with_sentinel(), b"abcde\0");
    assert_eq!(&*second.into_mbox_with_sentinel(), "abcde\0");
}

#[test]
fn test_default_array() {
    let arr = MArray::<u64>::default();
    assert_eq!(arr.into_mbox_with_sentinel(), MBox::from_slice(&[0u64]));
}

#[test]
fn test_default_string() {
    let string = MString::default();
    assert_eq!(string.into_mbox_with_sentinel(), MBox::from_str("\0"));
}

#[cfg(feature="std")]
#[test]
fn test_hash_string() {
    use std::collections::HashSet;

    let mut hs: HashSet<MString> = HashSet::new();
    hs.insert(MString::from_str("a"));
    hs.insert(MString::from_str("bcd"));

    let hs = hs;
    assert!(hs.contains("bcd"));
    assert!(!hs.contains("ef"));
    assert!(hs.contains("a"));
}

#[cfg(feature="std")]
#[test]
fn test_hash_array() {
    use std::collections::HashSet;

    let mut hs: HashSet<MArray<u8>> = HashSet::new();
    hs.insert(MArray::from_slice(b"a"));
    hs.insert(MArray::from_slice(b"bcd"));

    let hs = hs;
    assert!(hs.contains(&b"bcd"[..]));
    assert!(!hs.contains(&b"ef"[..]));
    assert!(hs.contains(&b"a"[..]));
}

