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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_creates_mutex_with_data() {
        let mutex = UnsafeMutex::new(42);
        assert_eq!(*mutex.lock(), 42);
    }

    #[test]
    fn lock_returns_mutable_reference() {
        let mutex = UnsafeMutex::new(String::from("hello"));
        let data = mutex.lock();
        data.push_str(" world");
        assert_eq!(mutex.lock().as_str(), "hello world");
    }

    #[test]
    fn debug_format_shows_type_name() {
        let mutex = UnsafeMutex::new(42);
        let debug_str = format!("{:?}", mutex);
        assert!(debug_str.contains("UnsafeMutex"));
    }

    #[test]
    fn send_trait_is_implemented() {
        fn assert_send<T: Send>() {}
        assert_send::<UnsafeMutex<i32>>();
    }

    #[test]
    fn sync_trait_is_implemented() {
        fn assert_sync<T: Sync>() {}
        assert_sync::<UnsafeMutex<i32>>();
    }

    #[test]
    fn multiple_locks_return_same_data() {
        let mutex = UnsafeMutex::new(vec![1, 2, 3]);
        mutex.lock().push(4);
        assert_eq!(mutex.lock().len(), 4);
        mutex.lock().pop();
        assert_eq!(mutex.lock().len(), 3);
    }
}
