//! Minimal cross-platform dynamic library loading (no external crates).
//!
//! Provides `DynLib` for loading shared libraries and resolving symbols at
//! runtime. Used by the driver layer to load `libcuda.so.1` / `nvcuda.dll`.
//! Ported from strata-inference's proven `backend/dl.rs`.

use std::ffi::CStr;
use std::os::raw::{c_char, c_void};

/// Handle to a dynamically loaded shared library.
pub(crate) struct DynLib {
    handle: *mut c_void,
}

// SAFETY: The library handle is a process-global resource; sharing across
// threads is safe as long as callers ensure symbol usage is thread-safe
// (which CUDA guarantees for its driver API).
unsafe impl Send for DynLib {}
unsafe impl Sync for DynLib {}

impl DynLib {
    /// Open a shared library by name.
    ///
    /// On Unix, wraps `dlopen` with `RTLD_NOW | RTLD_LOCAL`.
    /// On Windows, wraps `LoadLibraryA`.
    pub(crate) fn open(name: &CStr) -> Result<Self, String> {
        #[cfg(unix)]
        {
            // SAFETY: name is a valid C string. dlopen with RTLD_NOW resolves
            // all symbols immediately; RTLD_LOCAL keeps them in this handle.
            let handle = unsafe { dlopen(name.as_ptr(), RTLD_NOW | RTLD_LOCAL) };
            if handle.is_null() {
                let err = unsafe { dlerror() };
                let msg = if err.is_null() {
                    "unknown dlopen error".to_string()
                } else {
                    unsafe { CStr::from_ptr(err) }
                        .to_string_lossy()
                        .into_owned()
                };
                return Err(msg);
            }
            Ok(Self { handle })
        }

        #[cfg(windows)]
        {
            let handle = unsafe { LoadLibraryA(name.as_ptr()) };
            if handle.is_null() {
                return Err(format!("LoadLibraryA failed for {:?}", name));
            }
            Ok(Self {
                handle: handle as *mut c_void,
            })
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = name;
            Err("dynamic library loading not supported on this platform".to_string())
        }
    }

    /// Look up a symbol by name, returning a raw pointer.
    ///
    /// # Safety
    ///
    /// The caller must ensure the returned pointer is cast to the correct
    /// function signature before use.
    pub(crate) unsafe fn sym(&self, name: &CStr) -> Result<*mut c_void, String> {
        #[cfg(unix)]
        {
            // SAFETY: handle is a live dlopen handle; name is a valid C
            // string; dlerror is called to clear and then read the
            // thread-local error state.
            unsafe {
                // Clear any previous error.
                dlerror();
                let ptr = dlsym(self.handle, name.as_ptr());
                let err = dlerror();
                if !err.is_null() {
                    let msg = CStr::from_ptr(err).to_string_lossy().into_owned();
                    return Err(msg);
                }
                Ok(ptr)
            }
        }

        #[cfg(windows)]
        {
            // SAFETY: handle is a live LoadLibraryA handle; name is a valid
            // C string.
            unsafe {
                let ptr = GetProcAddress(self.handle as _, name.as_ptr());
                if ptr.is_null() {
                    return Err(format!("GetProcAddress failed for {:?}", name));
                }
                Ok(ptr as *mut c_void)
            }
        }

        #[cfg(not(any(unix, windows)))]
        {
            let _ = name;
            Err("dynamic library loading not supported on this platform".to_string())
        }
    }
}

impl Drop for DynLib {
    fn drop(&mut self) {
        if !self.handle.is_null() {
            #[cfg(unix)]
            unsafe {
                dlclose(self.handle);
            }

            #[cfg(windows)]
            unsafe {
                FreeLibrary(self.handle as _);
            }
        }
    }
}

// --- Unix (Linux + macOS) bindings ---

#[cfg(unix)]
const RTLD_NOW: i32 = 2;
#[cfg(unix)]
const RTLD_LOCAL: i32 = 0;

#[cfg(unix)]
extern "C" {
    fn dlopen(filename: *const c_char, flags: i32) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
    fn dlclose(handle: *mut c_void) -> i32;
    fn dlerror() -> *const c_char;
}

// --- Windows bindings ---

#[cfg(windows)]
extern "system" {
    fn LoadLibraryA(name: *const c_char) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const c_char) -> *mut c_void;
    fn FreeLibrary(module: *mut c_void) -> i32;
}
