#![allow(clippy::not_unsafe_ptr_arg_deref)]

//! C++-facing bridge for the serializer module.
//!
//! The Rust `BinarySerializer`/`BinaryDeserializer` cannot be constructed from
//! C++ directly (their constructors take a `dyn WriteStream`/`dyn ReadStream`),
//! so this module exposes concrete, Crubit-bridgeable structs that own a
//! callback-backed stream. The C++ side fills the callback with a thunk into
//! its own `WriteStream`/`ReadStream`.
//!
//! Panics raised by the Rust side are caught here and stored in a thread-local
//! slot, so the C++ shim can rethrow them as `SerializationException`.

use std::panic::{AssertUnwindSafe, catch_unwind};

use crate::serializer::{
    BinaryDeserializer, BinarySerializer, Deserializer, SerializationOptions, Serializer, idx_t,
};

// --- thread-local pending error slot --------------------------------------

thread_local! {
    static PENDING_ERROR: std::cell::RefCell<Option<String>> = const { std::cell::RefCell::new(None) };
    static LAST_STRING: std::cell::RefCell<Option<Vec<u8>>> = const { std::cell::RefCell::new(None) };
}

fn set_pending_error(msg: String) {
    PENDING_ERROR.with(|slot| {
        *slot.borrow_mut() = Some(msg);
    });
}

fn extract_panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "Rust serializer panicked".to_string()
    }
}

pub fn has_pending_error() -> bool {
    PENDING_ERROR.with(|slot| slot.borrow().is_some())
}

pub fn pending_error_ptr() -> *const u8 {
    PENDING_ERROR.with(|slot| {
        slot.borrow()
            .as_ref()
            .map(|s| s.as_ptr())
            .unwrap_or(std::ptr::null())
    })
}

pub fn pending_error_len() -> u64 {
    PENDING_ERROR.with(|slot| slot.borrow().as_ref().map(|s| s.len() as u64).unwrap_or(0))
}

pub fn clear_pending_error() {
    PENDING_ERROR.with(|slot| {
        *slot.borrow_mut() = None;
    });
}

pub fn last_string_ptr() -> *const u8 {
    CppBinaryDeserializer::last_string_ptr()
}

// --- callback streams ------------------------------------------------------

struct CallbackWriteStream {
    ctx: usize,
    write_fn: usize,
}

impl crate::serializer::write_stream::WriteStream for CallbackWriteStream {
    fn write_data(&mut self, buffer: *const u8, write_size: idx_t) {
        if write_size == 0 {
            return;
        }
        // SAFETY: `write_fn` is a valid C function pointer provided by the
        // C++ side (a thunk with C linkage), and `ctx` is the C++ stream it
        // expects.
        let f: extern "C" fn(usize, *const u8, idx_t) =
            unsafe { std::mem::transmute(self.write_fn) };
        f(self.ctx, buffer, write_size);
    }
}

struct CallbackReadStream {
    ctx: usize,
    read_fn: usize,
}

impl crate::serializer::read_stream::ReadStream for CallbackReadStream {
    fn read_data(&mut self, buffer: *mut u8, read_size: idx_t) {
        if read_size == 0 {
            return;
        }
        // SAFETY: `read_fn` is a valid C function pointer provided by the
        // C++ side (a thunk with C linkage), and `ctx` is the C++ stream it
        // expects.
        let f: extern "C" fn(usize, *mut u8, idx_t) = unsafe { std::mem::transmute(self.read_fn) };
        f(self.ctx, buffer, read_size);
    }
}

// --- CppBinarySerializer ---------------------------------------------------

pub struct CppBinarySerializer {
    stream_ptr: usize,
    serializer_ptr: usize,
}

impl CppBinarySerializer {
    pub fn new(write_ctx: usize, write_fn: usize) -> Self {
        let stream = Box::new(CallbackWriteStream {
            ctx: write_ctx,
            write_fn,
        });
        // SAFETY: Box allocation is stable; we leak it manually and reclaim in
        // Drop. The serializer borrows this allocation for its whole lifetime.
        let stream_ptr = Box::into_raw(stream) as usize;
        let stream_ref: &'static mut CallbackWriteStream =
            unsafe { &mut *(stream_ptr as *mut CallbackWriteStream) };
        let serializer = Box::new(BinarySerializer::new(
            stream_ref,
            SerializationOptions::default(),
        ));
        let serializer_ptr = Box::into_raw(serializer) as usize;
        Self {
            stream_ptr,
            serializer_ptr,
        }
    }

    fn serializer(&mut self) -> &mut BinarySerializer<'static> {
        // SAFETY: `serializer_ptr` points to a live Box allocation created in
        // `new` and not yet freed (Drop runs after the last use).
        unsafe { &mut *(self.serializer_ptr as *mut BinarySerializer<'static>) }
    }

    pub fn on_property_begin(&mut self, field_id: u16, tag: *const u8) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_property_begin(field_id, tag);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_property_end(&mut self) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_property_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_optional_property_begin(&mut self, field_id: u16, tag: *const u8, present: bool) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_optional_property_begin(field_id, tag, present);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_optional_property_end(&mut self, present: bool) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_optional_property_end(present);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_object_begin(&mut self) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_object_begin();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_object_end(&mut self) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_object_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_list_begin(&mut self, count: u64) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_list_begin(count);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_list_end(&mut self) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_list_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_nullable_begin(&mut self, present: bool) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_nullable_begin(present);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_nullable_end(&mut self) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.on_nullable_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_null(&mut self) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_null();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_bool(&mut self, value: bool) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_bool(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_char(&mut self, value: i8) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_char(value as u8 as char);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_int8(&mut self, value: i8) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_int8(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_uint8(&mut self, value: u8) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_uint8(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_int16(&mut self, value: i16) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_int16(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_uint16(&mut self, value: u16) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_uint16(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_int32(&mut self, value: i32) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_int32(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_uint32(&mut self, value: u32) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_uint32(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_int64(&mut self, value: i64) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_int64(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_uint64(&mut self, value: u64) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_uint64(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_hugeint(&mut self, upper: i64, lower: u64) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_hugeint(crate::serializer::hugeint_t { upper, lower });
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_uhugeint(&mut self, upper: u64, lower: u64) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_uhugeint(crate::serializer::uhugeint_t { upper, lower });
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_float(&mut self, value: f32) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_float(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_double(&mut self, value: f64) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_double(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_string_t(&mut self, data: *const u8, size: usize) {
        let s = self.serializer();
        let st = crate::serializer::string_t {
            data: if size == 0 {
                Vec::new()
            } else {
                // SAFETY: caller guarantees `data` points to `size` readable
                // bytes.
                unsafe { std::slice::from_raw_parts(data, size).to_vec() }
            },
            size,
        };
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_string_t(st);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_string(&mut self, data: *const u8, len: u64) {
        // Preserve raw bytes (binary-safe): delegate to write_string_t instead
        // of going through `string`/`String` (which would UTF-8
        // lossy-convert).
        self.write_string_t(data, len as usize);
    }

    pub fn write_cstr(&mut self, value: *const u8) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_cstr(value);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn write_data_ptr(&mut self, ptr: *const u8, count: u64) {
        let s = self.serializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            s.write_data_ptr(ptr, count);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }
}

impl Drop for CppBinarySerializer {
    fn drop(&mut self) {
        if self.serializer_ptr != 0 {
            // SAFETY: `serializer_ptr` is a live Box allocation from `new`.
            unsafe {
                drop(Box::from_raw(
                    self.serializer_ptr as *mut BinarySerializer<'static>,
                ));
            }
            self.serializer_ptr = 0;
        }
        if self.stream_ptr != 0 {
            // SAFETY: `stream_ptr` is a live Box allocation from `new`.
            unsafe {
                drop(Box::from_raw(self.stream_ptr as *mut CallbackWriteStream));
            }
            self.stream_ptr = 0;
        }
    }
}

// --- CppBinaryDeserializer -------------------------------------------------

pub struct CppBinaryDeserializer {
    stream_ptr: usize,
    deserializer_ptr: usize,
}

impl CppBinaryDeserializer {
    pub fn new(read_ctx: usize, read_fn: usize) -> Self {
        let stream = Box::new(CallbackReadStream {
            ctx: read_ctx,
            read_fn,
        });
        // SAFETY: Box allocation is stable; we leak it manually and reclaim in
        // Drop. The deserializer borrows this allocation for its whole
        // lifetime.
        let stream_ptr = Box::into_raw(stream) as usize;
        let stream_ref: &'static mut CallbackReadStream =
            unsafe { &mut *(stream_ptr as *mut CallbackReadStream) };
        let deserializer = Box::new(BinaryDeserializer::new(stream_ref));
        let deserializer_ptr = Box::into_raw(deserializer) as usize;
        Self {
            stream_ptr,
            deserializer_ptr,
        }
    }

    fn deserializer(&mut self) -> &mut BinaryDeserializer<'static> {
        // SAFETY: `deserializer_ptr` points to a live Box allocation created
        // in `new` and not yet freed (Drop runs after the last use).
        unsafe { &mut *(self.deserializer_ptr as *mut BinaryDeserializer<'static>) }
    }

    pub fn on_object_begin(&mut self) {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            d.on_object_begin();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_object_end(&mut self) {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            d.on_object_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_list_begin(&mut self) -> u64 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.on_list_begin()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn on_list_end(&mut self) {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            d.on_list_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_nullable_begin(&mut self) -> bool {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.on_nullable_begin()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                false
            }
        }
    }

    pub fn on_nullable_end(&mut self) {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            d.on_nullable_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_property_begin(&mut self, field_id: u16, tag: *const u8) -> bool {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.on_property_begin(field_id, tag)));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                false
            }
        }
    }

    pub fn on_property_end(&mut self) {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            d.on_property_end();
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn on_optional_property_begin(&mut self, field_id: u16, tag: *const u8) -> bool {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            d.on_optional_property_begin(field_id, tag)
        }));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                false
            }
        }
    }

    pub fn on_optional_property_end(&mut self, present: bool) {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| {
            d.on_optional_property_end(present);
        }));
        if let Err(p) = res {
            set_pending_error(extract_panic_message(p));
        }
    }

    pub fn peek_field(&mut self) -> u16 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.peek_field()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_bool(&mut self) -> bool {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_bool()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                false
            }
        }
    }

    pub fn read_char(&mut self) -> u8 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_char()));
        match res {
            Ok(v) => v as u8,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_int8(&mut self) -> i8 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_int8()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_uint8(&mut self) -> u8 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_uint8()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_int16(&mut self) -> i16 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_int16()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_uint16(&mut self) -> u16 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_uint16()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_int32(&mut self) -> i32 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_int32()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_uint32(&mut self) -> u32 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_uint32()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_int64(&mut self) -> i64 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_int64()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_uint64(&mut self) -> u64 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_uint64()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn read_float(&mut self) -> f32 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_float()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0.0
            }
        }
    }

    pub fn read_double(&mut self) -> f64 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_double()));
        match res {
            Ok(v) => v,
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0.0
            }
        }
    }

    pub fn read_hugeint(&mut self, out_upper: *mut i64, out_lower: *mut u64) -> bool {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_hugeint()));
        match res {
            Ok(v) => {
                // SAFETY: caller provides writable out pointers.
                unsafe {
                    *out_upper = v.upper;
                    *out_lower = v.lower;
                }
                true
            }
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                false
            }
        }
    }

    pub fn read_uhugeint(&mut self, out_upper: *mut u64, out_lower: *mut u64) -> bool {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_uhugeint()));
        match res {
            Ok(v) => {
                // SAFETY: caller provides writable out pointers.
                unsafe {
                    *out_upper = v.upper;
                    *out_lower = v.lower;
                }
                true
            }
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                false
            }
        }
    }

    pub fn read_string(&mut self) -> u64 {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_string_t()));
        match res {
            Ok(v) => {
                let bytes = v.get_data().to_vec();
                let n = bytes.len() as u64;
                LAST_STRING.with(|slot| {
                    *slot.borrow_mut() = Some(bytes);
                });
                n
            }
            Err(p) => {
                set_pending_error(extract_panic_message(p));
                0
            }
        }
    }

    pub fn last_string_ptr() -> *const u8 {
        LAST_STRING.with(|slot| {
            slot.borrow()
                .as_ref()
                .map(|v| v.as_ptr())
                .unwrap_or(std::ptr::null())
        })
    }

    pub fn read_data_into(&mut self, out: *mut u8, count: u64) {
        let d = self.deserializer();
        let res = catch_unwind(AssertUnwindSafe(|| d.read_data_ptr(count)));
        match res {
            Ok(v) => {
                // SAFETY: caller provides `count` writable bytes at `out`.
                unsafe {
                    std::ptr::copy_nonoverlapping(v.as_ptr(), out, count as usize);
                }
            }
            Err(p) => {
                set_pending_error(extract_panic_message(p));
            }
        }
    }

    pub fn nesting_level(&mut self) -> u64 {
        self.deserializer().nesting_level()
    }

    pub fn set_nesting_level(&mut self, level: u64) {
        self.deserializer().set_nesting_level(level);
    }
}

impl Drop for CppBinaryDeserializer {
    fn drop(&mut self) {
        if self.deserializer_ptr != 0 {
            // SAFETY: `deserializer_ptr` is a live Box allocation from `new`.
            unsafe {
                drop(Box::from_raw(
                    self.deserializer_ptr as *mut BinaryDeserializer<'static>,
                ));
            }
            self.deserializer_ptr = 0;
        }
        if self.stream_ptr != 0 {
            // SAFETY: `stream_ptr` is a live Box allocation from `new`.
            unsafe {
                drop(Box::from_raw(self.stream_ptr as *mut CallbackReadStream));
            }
            self.stream_ptr = 0;
        }
    }
}

pub fn cpp_binary_serializer_new(write_ctx: usize, write_fn: usize) -> usize {
    let obj = CppBinarySerializer::new(write_ctx, write_fn);
    Box::into_raw(Box::new(obj)) as usize
}

pub fn cpp_binary_serializer_delete(ptr: usize) {
    if ptr != 0 {
        // SAFETY: `ptr` came from `cpp_binary_serializer_new`.
        unsafe {
            drop(Box::from_raw(ptr as *mut CppBinarySerializer));
        }
    }
}

pub fn cpp_binary_deserializer_new(read_ctx: usize, read_fn: usize) -> usize {
    let obj = CppBinaryDeserializer::new(read_ctx, read_fn);
    Box::into_raw(Box::new(obj)) as usize
}

pub fn cpp_binary_deserializer_delete(ptr: usize) {
    if ptr != 0 {
        // SAFETY: `ptr` came from `cpp_binary_deserializer_new`.
        unsafe {
            drop(Box::from_raw(ptr as *mut CppBinaryDeserializer));
        }
    }
}
