use std::{ffi::c_void, mem, ptr::NonNull};

use v8::{Global, Isolate, Value};

#[repr(transparent)]
#[derive(Debug)]
pub struct ObscuredContextScope(NonNull<c_void>);

impl ObscuredContextScope {
    pub const fn new(ctx_scope: &mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) -> Self {
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

pub struct ValueArena {
    values: Vec<Option<ObscuredGlobal<Value>>>,
    vacancies: Vec<usize>,
}

impl ValueArena {
    #[inline(always)]
    pub const fn new() -> Self {
        Self {
            values: vec![],
            vacancies: vec![],
        }
    }

    /// Allocate value, returning the index.
    ///
    /// # Safety
    /// The vacancies vector must be truthful.
    pub fn alloc(&mut self, value: Global<Value>) -> usize {
        let global = ObscuredGlobal::new(value);

        if let Some(vacancy) = self.vacancies.pop() {
            let Some(vacant) = self.values.get_mut(vacancy) else {
                unsafe { core::hint::unreachable_unchecked() }
            };
            vacant.replace(global);
            vacancy
        } else {
            let id = self.values.len();
            self.values.push(Some(global));
            id
        }
    }

    /// Deallocate object of index `index`, if exists.
    ///
    /// If the object exists, `true` is returned; `false` otherwise.
    pub async fn dealloc(
        &mut self,
        ctx_scope: &tokio::sync::Mutex<ObscuredContextScope>,
        index: usize,
    ) -> bool {
        if let Some(item) = self.values.get_mut(index) {
            let Some(glob) = item.take() else {
                return false;
            };
            {
                let mut holder = ctx_scope.lock().await;
                let _ = glob.take(holder.get());
            }

            // once we're done, we need to mark it as vacant
            self.vacancies.push(index);
            true
        } else {
            false
        }
    }

    pub async fn with<R>(
        &self,
        ctx_scope: &tokio::sync::Mutex<ObscuredContextScope>,
        index: usize,
        callback: impl FnOnce(&Value) -> R,
    ) -> Option<R> {
        if let Some(Some(item)) = self.values.get(index) {
            let mut holder = ctx_scope.lock().await;
            Some(item.with(holder.get(), callback))
        } else {
            None
        }
    }
}

pub struct IsolateState {
    pub ctx_scope: tokio::sync::Mutex<ObscuredContextScope>,
    pub arena: tokio::sync::Mutex<ValueArena>,
}

impl IsolateState {
    #[inline(always)]
    pub const fn new(ctx_scope: &mut v8::ContextScope<'_, '_, v8::HandleScope<'_>>) -> Self {
        Self {
            ctx_scope: tokio::sync::Mutex::const_new(ObscuredContextScope::new(ctx_scope)),
            arena: tokio::sync::Mutex::const_new(ValueArena::new()),
        }
    }

    pub async fn close(&self) {
        let mut arena = self.arena.lock().await;
        let mut ctx_scope = self.ctx_scope.lock().await;
        arena.values.drain(..).for_each(|item| {
            if let Some(global) = item {
                // drop the global
                let _ = global.take(ctx_scope.get());
            }
        });
    }
}
