use std::cell::UnsafeCell;
// use std::hash::{Hash, Hasher};

#[derive(Debug)]
pub struct UnsafeMutex<T> {
    data: UnsafeCell<T>,
}

impl<T> UnsafeMutex<T> {
    pub fn new(data: T) -> Self {
        UnsafeMutex {
            data: UnsafeCell::new(data),
        }
    }

    pub fn lock(&self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

unsafe impl<T: Send> Send for UnsafeMutex<T> {}
unsafe impl<T: Send> Sync for UnsafeMutex<T> {}
