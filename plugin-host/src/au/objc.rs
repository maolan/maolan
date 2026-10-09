//! Low-level Objective-C runtime and Apple Block ABI shims shared by the AU
//! backends. Hand-rolled FFI consistent with `gui_cocoa.rs`; the unsafe
//! surface for message sending and block construction lives here.
//!
//! Block literals follow the documented ABI (see `Block.h` in libSystem):
//! isa, flags, reserved, invoke, descriptor, then captures. We use one fixed
//! layout whose single capture is a `usize` context pointer (usually a leaked
//! `Box` owned by Rust), so a single static descriptor of the matching size
//! serves all blocks. `_Block_copy` moves the literal to the heap; callers
//! balance with `block_release` unless the callee consumed the reference.

use std::ffi::{CString, c_char, c_void};

pub(crate) type Sel = *const c_char;
pub(crate) type Class = *mut c_void;
pub(crate) type Id = *mut c_void;

#[link(name = "objc")]
unsafe extern "C" {
    fn objc_msgSend();
    fn sel_registerName(name: *const c_char) -> Sel;
    fn objc_getClass(name: *const c_char) -> Class;
}

#[cfg(target_arch = "x86_64")]
#[link(name = "objc")]
unsafe extern "C" {
    fn objc_msgSend_fpret();
}

#[link(name = "AppKit", kind = "framework")]
unsafe extern "C" {}

#[link(name = "CoreAudioKit", kind = "framework")]
unsafe extern "C" {}

unsafe extern "C" {
    static _NSConcreteStackBlock: c_void;
    fn _Block_copy(block: *const c_void) -> *mut c_void;
    fn _Block_release(block: *const c_void);
}

pub(crate) fn msg_send_addr() -> usize {
    objc_msgSend as *const () as usize
}

pub(crate) fn sel(name: &str) -> Sel {
    let c = CString::new(name).expect("selector has no nul");
    unsafe { sel_registerName(c.as_ptr()) }
}

pub(crate) fn class(name: &str) -> Class {
    let c = CString::new(name).expect("class name has no nul");
    unsafe { objc_getClass(c.as_ptr()) }
}

pub(crate) fn ns_string_to_string(s: Id) -> String {
    if s.is_null() {
        return String::new();
    }
    unsafe {
        let ptr = send0(s, sel("UTF8String")) as *const c_char;
        if ptr.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(ptr).to_string_lossy().into_owned()
    }
}

// Typed objc_msgSend shims. Transmuting the symbol to a signature whose
// argument/return types exactly match the callee is the standard way to call
// ObjC methods from Rust. On arm64 `objc_msgSend` handles all scalar returns;
// on x86_64 floating-point returns must go through `objc_msgSend_fpret`.
macro_rules! shim_fpret {
    ($name:ident, () -> $ret:ty) => {
        pub(crate) unsafe fn $name(obj: Id, selector: Sel) -> $ret {
            #[cfg(target_arch = "x86_64")]
            let addr = objc_msgSend_fpret as *const () as usize;
            #[cfg(not(target_arch = "x86_64"))]
            let addr = msg_send_addr();
            let f: unsafe extern "C" fn(Id, Sel) -> $ret = unsafe { std::mem::transmute(addr) };
            unsafe { f(obj, selector) }
        }
    };
}

// Concrete shims (macro above kept simple by instantiating per shape).
macro_rules! def_shims {
    ($(fn $name:ident($($p:ident : $t:ty),*) -> $ret:ty;)*) => {
        $(
            pub(crate) unsafe fn $name(obj: Id, selector: Sel, $($p: $t),*) -> $ret {
                let f: unsafe extern "C" fn(Id, Sel, $($t),*) -> $ret =
                    unsafe { std::mem::transmute(msg_send_addr()) };
                unsafe { f(obj, selector, $($p),*) }
            }
        )*
    };
}

def_shims! {
    fn send0() -> Id;
    fn send1(a: usize) -> Id;
    fn send0_isize() -> isize;
    fn send0_usize() -> usize;
    fn send1_usize(a: usize) -> usize;
    fn send2_bool_ret(a: usize, b: usize) -> u8;
}

shim_fpret!(send0_f32, () -> f32);
shim_fpret!(send0_f64, () -> f64);

pub(crate) unsafe fn send0_unit(obj: Id, selector: Sel) {
    let f: unsafe extern "C" fn(Id, Sel) = unsafe { std::mem::transmute(msg_send_addr()) };
    unsafe { f(obj, selector) };
}

pub(crate) unsafe fn send1_unit(obj: Id, selector: Sel, a: usize) {
    let f: unsafe extern "C" fn(Id, Sel, usize) = unsafe { std::mem::transmute(msg_send_addr()) };
    unsafe { f(obj, selector, a) };
}

pub(crate) unsafe fn send2_f32_unit(obj: Id, selector: Sel, a: f32, b: usize) {
    let f: unsafe extern "C" fn(Id, Sel, f32, usize) =
        unsafe { std::mem::transmute(msg_send_addr()) };
    unsafe { f(obj, selector, a, b) };
}

pub(crate) unsafe fn send1_bool(obj: Id, selector: Sel, a: usize) -> bool {
    let f: unsafe extern "C" fn(Id, Sel, usize) -> u8 =
        unsafe { std::mem::transmute(msg_send_addr()) };
    unsafe { f(obj, selector, a) != 0 }
}

/// `-[NSApplication isRunning]`.
pub(crate) unsafe fn send0_running(obj: Id) -> bool {
    let f: unsafe extern "C" fn(Id, Sel) -> u8 = unsafe { std::mem::transmute(msg_send_addr()) };
    unsafe { f(obj, sel("isRunning")) != 0 }
}

/// Block literal with a single erased context capture.
#[repr(C)]
pub(crate) struct ErasedBlock {
    pub isa: *mut c_void,
    pub flags: i32,
    pub reserved: i32,
    pub invoke: *mut c_void,
    pub descriptor: *const BlockDescriptor,
    pub context: usize,
}

#[repr(C)]
pub(crate) struct BlockDescriptor {
    pub reserved: usize,
    pub size: usize,
}

static BLOCK_DESCRIPTOR: BlockDescriptor = BlockDescriptor {
    reserved: 0,
    size: std::mem::size_of::<ErasedBlock>(),
};

/// Heap-copy a stack block literal carrying `context`. The context is an
/// erased pointer owned by the caller; the copy shares it verbatim (no copy/
/// dispose helpers are registered, so `_Block_copy` memcpys the literal only).
pub(crate) unsafe fn block_copy_with_context(invoke: *mut c_void, context: usize) -> *mut c_void {
    let literal = ErasedBlock {
        isa: &raw const _NSConcreteStackBlock as *mut c_void,
        flags: 0,
        reserved: 0,
        invoke,
        descriptor: &raw const BLOCK_DESCRIPTOR,
        context,
    };
    unsafe { _Block_copy((&literal as *const ErasedBlock).cast()) }
}

/// Recover the context pointer stored in a block literal passed to an invoke
/// function.
///
/// # Safety
/// `block` must be a block created by `block_copy_with_context`.
pub(crate) unsafe fn block_context<T>(block: *mut c_void) -> *mut T {
    unsafe { (*(block as *mut ErasedBlock)).context as *mut T }
}

pub(crate) unsafe fn block_release(block: *mut c_void) {
    unsafe { _Block_release(block) };
}
