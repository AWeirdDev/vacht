use std::{ffi::c_void, mem, ptr::NonNull};

use v8::{Global, Isolate};

#[repr(transparent)]
#[derive(Debug)]
pub struct ObscuredContextScope(NonNull<c_void>);

impl ObscuredContextScope {
    pub fn new(ctx_scope: &mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) -> Self {
        let t = Self(unsafe {
            NonNull::new_unchecked(
                ctx_scope as *mut v8::ContextScope<'_, '_, v8::HandleScope<'_>> as _,
            )
        });

        t
    }

    #[inline(always)]
    pub unsafe fn get_unchecked(
        &self,
    ) -> &mut v8::ContextScope<'static, 'static, v8::HandleScope<'static>> {
        unsafe {
            &mut *(self.0.as_ptr()
                as *mut v8::ContextScope<'static, 'static, v8::HandleScope<'static>>)
        }
    }

    #[inline(always)]
    pub fn get(
        &mut self, // this ensures there's only ONE mut holder
    ) -> &mut v8::ContextScope<'static, 'static, v8::HandleScope<'static>> {
        unsafe { self.get_unchecked() }
    }
}

unsafe impl Send for ObscuredContextScope {}
unsafe impl Sync for ObscuredContextScope {}

impl Drop for ObscuredContextScope {
    fn drop(&mut self) {
        let _ = self.get();
    }
}

/// [`Global`] with [`Send`] and [`Sync`].
///
/// # Dropping
/// Dropping is **NOT** implemented.
/// You must drop it yourself using [`ObscuredGlobal::take`].
#[repr(transparent)]
#[derive(Debug)]
pub struct ObscuredGlobal<T>(NonNull<T>);

impl<T> ObscuredGlobal<T> {
    #[inline(always)]
    pub fn new(gl: Global<T>) -> Self {
        Self(gl.into_raw())
    }

    #[inline(always)]
    #[must_use]
    pub fn take(self, isolate: &mut Isolate) -> Global<T> {
        unsafe { Global::from_raw(isolate, self.0) }
    }

    #[inline(always)]
    #[must_use]
    pub fn with<R>(&self, isolate: &mut Isolate, f: impl FnOnce(&T) -> R) -> R {
        let glob = unsafe { Global::from_raw(isolate, self.0) };
        let res = f(glob.open(isolate));

        // v8 literally does this
        mem::forget(glob);

        res
    }
}

unsafe impl<T> Send for ObscuredGlobal<T> {}
unsafe impl<T> Sync for ObscuredGlobal<T> {}

impl<T> ToString for ObscuredGlobal<T> {
    #[inline(always)]
    fn to_string(&self) -> String {
        format!("{:p}", self.0)
    }
}

pub struct IsolateState {
    pub ctx_scope: tokio::sync::Mutex<ObscuredContextScope>,
}

impl IsolateState {
    pub fn new(ctx_scope: &mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) -> Self {
        Self {
            ctx_scope: tokio::sync::Mutex::new(ObscuredContextScope::new(ctx_scope)),
        }
    }
}
