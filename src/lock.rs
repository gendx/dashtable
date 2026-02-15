// This code is forked from https://docs.rs/dashmap/7.0.0-rc2/src/dashmap/util.rs.html,
// available under the MIT license.

use parking_lot::RawRwLock;

pub type RwLock<T> = lock_api::RwLock<RawRwLock, T>;
pub type RwLockReadGuardDetached<'a> = detail::RwLockReadGuardDetached<'a, RawRwLock>;
pub type RwLockWriteGuardDetached<'a> = detail::RwLockWriteGuardDetached<'a, RawRwLock>;

mod detail {
    use lock_api::{RawRwLock, RwLockReadGuard, RwLockWriteGuard};
    use std::marker::PhantomData;
    use std::mem::ManuallyDrop;

    /// A [`RwLockReadGuard`], without the data
    pub(crate) struct RwLockReadGuardDetached<'a, R: RawRwLock> {
        lock: &'a R,
        _marker: PhantomData<R::GuardMarker>,
    }

    impl<R: RawRwLock> Drop for RwLockReadGuardDetached<'_, R> {
        fn drop(&mut self) {
            // Safety: An RwLockReadGuardDetached always holds a shared lock.
            unsafe {
                self.lock.unlock_shared();
            }
        }
    }

    impl<'a, R: RawRwLock> RwLockReadGuardDetached<'a, R> {
        /// Separates the data from the [`RwLockReadGuard`]
        ///
        /// # Safety
        ///
        /// The data must not outlive the detached guard
        pub(crate) unsafe fn detach_from<T>(guard: RwLockReadGuard<'a, R, T>) -> (Self, &'a T) {
            let rwlock = RwLockReadGuard::rwlock(&ManuallyDrop::new(guard));
            // Safety: There will be no concurrent writes as we are "forgetting" the
            // existing guard, with the safety assumption that the caller will
            // not drop the new detached guard early.
            let data = unsafe { &*rwlock.data_ptr() };
            let guard = RwLockReadGuardDetached {
                // Safety: We are imitating the original RwLockReadGuard. It's the callers
                // responsibility to not drop the guard early.
                lock: unsafe { rwlock.raw() },
                _marker: PhantomData,
            };
            (guard, data)
        }
    }

    /// A [`RwLockWriteGuard`], without the data
    pub(crate) struct RwLockWriteGuardDetached<'a, R: RawRwLock> {
        lock: &'a R,
        _marker: PhantomData<R::GuardMarker>,
    }

    impl<R: RawRwLock> Drop for RwLockWriteGuardDetached<'_, R> {
        fn drop(&mut self) {
            // Safety: An RwLockWriteGuardDetached always holds an exclusive lock.
            unsafe {
                self.lock.unlock_exclusive();
            }
        }
    }

    impl<'a, R: RawRwLock> RwLockWriteGuardDetached<'a, R> {
        /// Separates the data from the [`RwLockWriteGuard`]
        ///
        /// # Safety
        ///
        /// The data must not outlive the detached guard
        pub(crate) unsafe fn detach_from<T>(
            guard: RwLockWriteGuard<'a, R, T>,
        ) -> (Self, &'a mut T) {
            let rwlock = RwLockWriteGuard::rwlock(&ManuallyDrop::new(guard));
            // Safety: There will be no concurrent reads/writes as we are "forgetting" the
            // existing guard, with the safety assumption that the caller will
            // not drop the new detached guard early.
            let data = unsafe { &mut *rwlock.data_ptr() };
            let guard = RwLockWriteGuardDetached {
                // Safety: We are imitating the original RwLockWriteGuard. It's the callers
                // responsibility to not drop the guard early.
                lock: unsafe { rwlock.raw() },
                _marker: PhantomData,
            };
            (guard, data)
        }
    }
}
