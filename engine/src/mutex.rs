use std::cell::UnsafeCell;

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

    #[allow(clippy::mut_from_ref)]
    pub fn lock(&self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

unsafe impl<T: Send> Send for UnsafeMutex<T> {}
unsafe impl<T: Send> Sync for UnsafeMutex<T> {}
