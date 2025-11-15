use std::cell::UnsafeCell;

// A simple "fake" mutex that provides interior mutability
// but offers no actual thread-safe synchronization.
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

    // "Locks" the mutex and returns a mutable reference to the data.
    // This is unsafe because it provides no actual synchronization.
    pub fn lock(&self) -> &mut T {
        // SAFETY: This is a "fake" mutex and does not guarantee thread safety.
        // The caller is responsible for ensuring single-threaded access or
        // external synchronization if used in a multi-threaded context.
        unsafe { &mut *self.data.get() }
    }
}

// Implement Sync for UnsafeMutex if T is Sync and you intend to use it
// in a multi-threaded context where you guarantee external synchronization.
// This is a dangerous blanket impl and should be used with extreme caution.
// unsafe impl<T: Sync> Sync for UnsafeMutex<T> {}

// Implement Send for UnsafeMutex if T is Send.
// This is generally safe if UnsafeMutex itself doesn't contain non-Send types.
unsafe impl<T: Send> Send for UnsafeMutex<T> {}
unsafe impl<T: Send> Sync for UnsafeMutex<T> {}
