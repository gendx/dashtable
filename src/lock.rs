// This code is forked from https://docs.rs/dashmap/7.0.0-rc2/src/dashmap/util.rs.html,
// available under the MIT license.

use parking_lot::RawRwLock;

pub type RwLock<T> = detail::RwLock<RawRwLock, T>;
pub type RwLockReadGuardDetached<'a> = detail::RwLockReadGuardDetached<'a, RawRwLock>;
pub type RwLockWriteGuardDetached<'a> = detail::RwLockWriteGuardDetached<'a, RawRwLock>;

mod detail {
    use lock_api::RawRwLock;
    use std::marker::PhantomData;
    use std::mem::ManuallyDrop;
    use std::ops::{Deref, DerefMut};

    pub struct RwLock<R, T> {
        inner: lock_api::RwLock<R, T>,
    }

    impl<R: RawRwLock, T> RwLock<R, T> {
        pub fn new(t: T) -> Self {
            Self {
                inner: lock_api::RwLock::new(t),
            }
        }

        pub fn get_mut(&mut self) -> &mut T {
            self.inner.get_mut()
        }

        pub fn read(&self) -> RwLockReadGuard<'_, R, T> {
            // SAFETY: The shared guard derived from the raw lock is
            // encapsulated into the resulting read guard.
            let raw = unsafe { self.inner.raw() };
            let guard = raw.lock_shared();
            RwLockReadGuard {
                rwlock: &self.inner,
                guard: ManuallyDrop::new(guard),
                _phantom: PhantomData,
            }
        }

        pub fn write(&self) -> RwLockWriteGuard<'_, R, T> {
            // SAFETY: The exclusive guard derived from the raw lock is
            // encapsulated into the resulting write guard.
            let raw = unsafe { self.inner.raw() };
            let guard = raw.lock_exclusive();
            RwLockWriteGuard {
                rwlock: &self.inner,
                guard: ManuallyDrop::new(guard),
                _phantom: PhantomData,
            }
        }
    }

    pub struct RwLockReadGuard<'a, R: RawRwLock, T: 'a> {
        rwlock: &'a lock_api::RwLock<R, T>,
        guard: ManuallyDrop<R::SharedGuard>,
        _phantom: PhantomData<&'a T>,
    }

    impl<'a, R: RawRwLock, T: 'a> Drop for RwLockReadGuard<'a, R, T> {
        fn drop(&mut self) {
            // SAFETY: It is safe to unlock the raw lock, see below.
            let rwlock = unsafe { self.rwlock.raw() };
            // SAFETY: self.guard isn't used again, as we are in the Drop
            // implementation.
            let guard = unsafe { ManuallyDrop::take(&mut self.guard) };
            // SAFETY: All references to the locked data derived from this read
            // guard are now out of scope, so it's safe to unlock. Indeed, the
            // Deref implementation binds resulting references to this guard's
            // lifetime.
            unsafe {
                rwlock.unlock_shared(guard);
            }
        }
    }

    impl<'a, R: RawRwLock, T: 'a> Deref for RwLockReadGuard<'a, R, T> {
        type Target = T;

        fn deref(&self) -> &T {
            // SAFETY: Shared access is guaranteed for the whole output
            // lifetime.
            unsafe { &*self.rwlock.data_ptr() }
        }
    }

    impl<'a, R: RawRwLock, T: 'a> RwLockReadGuard<'a, R, T> {
        /// Separates the data from this [`RwLockReadGuard`].
        ///
        /// # Safety
        ///
        /// The data must not outlive the [`RwLockReadGuardDetached`].
        pub unsafe fn detach(self) -> (RwLockReadGuardDetached<'a, R>, &'a T) {
            // Inhibit this guard's Drop implementation, to avoid unlocking
            // twice.
            let mut this = ManuallyDrop::new(self);
            // SAFETY: The caller promises that the returned reference isn't
            // used beyond the detached guard's lifetime.
            let data = unsafe { &*this.rwlock.data_ptr() };
            // SAFETY: `this.guard` isn't used again, as `this` is wrapped in a
            // ManuallyDrop.
            let guard = unsafe { ManuallyDrop::take(&mut this.guard) };
            let guard = RwLockReadGuardDetached {
                // SAFETY: The caller promises that the returned reference isn't
                // used beyond the detached guard's lifetime.
                rwlock: unsafe { this.rwlock.raw() },
                guard: ManuallyDrop::new(guard),
            };
            (guard, data)
        }
    }

    /// A [`RwLockReadGuard`] without the data.
    pub struct RwLockReadGuardDetached<'a, R: RawRwLock> {
        rwlock: &'a R,
        guard: ManuallyDrop<R::SharedGuard>,
    }

    impl<R: RawRwLock> Drop for RwLockReadGuardDetached<'_, R> {
        fn drop(&mut self) {
            // SAFETY: self.guard isn't used again, as we are in the Drop
            // implementation.
            let guard = unsafe { ManuallyDrop::take(&mut self.guard) };
            // SAFETY: All references to the locked data derived from this read
            // guard are now out of scope - as promised by the caller of
            // RwLockReadGuard::detach() - so it's safe to unlock.
            unsafe {
                self.rwlock.unlock_shared(guard);
            }
        }
    }

    pub struct RwLockWriteGuard<'a, R: RawRwLock, T: 'a> {
        rwlock: &'a lock_api::RwLock<R, T>,
        guard: ManuallyDrop<R::ExclusiveGuard>,
        _phantom: PhantomData<&'a mut T>,
    }

    impl<'a, R: RawRwLock, T: 'a> Drop for RwLockWriteGuard<'a, R, T> {
        fn drop(&mut self) {
            // SAFETY: It is safe to unlock the raw lock, see below.
            let rwlock = unsafe { self.rwlock.raw() };
            // SAFETY: self.guard isn't used again, as we are in the Drop
            // implementation.
            let guard = unsafe { ManuallyDrop::take(&mut self.guard) };
            // SAFETY: All references to the locked data derived from this write
            // guard are now out of scope, so it's safe to unlock. Indeed, the
            // Deref and DerefMut implementations bind resulting references to
            // this guard's lifetime.
            unsafe {
                rwlock.unlock_exclusive(guard);
            }
        }
    }

    impl<'a, R: RawRwLock, T: 'a> Deref for RwLockWriteGuard<'a, R, T> {
        type Target = T;

        fn deref(&self) -> &T {
            // SAFETY: Exclusive access is guaranteed for the whole output
            // lifetime.
            unsafe { &*self.rwlock.data_ptr() }
        }
    }

    impl<'a, R: RawRwLock, T: 'a> DerefMut for RwLockWriteGuard<'a, R, T> {
        fn deref_mut(&mut self) -> &mut T {
            // SAFETY: Exclusive access is guaranteed for the whole output
            // lifetime.
            unsafe { &mut *self.rwlock.data_ptr() }
        }
    }

    impl<'a, R: RawRwLock, T: 'a> RwLockWriteGuard<'a, R, T> {
        /// Separates the data from this [`RwLockWriteGuard`].
        ///
        /// # Safety
        ///
        /// The data must not outlive the [`RwLockWriteGuardDetached`].
        pub unsafe fn detach(self) -> (RwLockWriteGuardDetached<'a, R>, &'a mut T) {
            // Inhibit this guard's Drop implementation, to avoid unlocking
            // twice.
            let mut this = ManuallyDrop::new(self);
            // SAFETY: The caller promises that the returned reference isn't
            // used beyond the detached guard's lifetime.
            let data = unsafe { &mut *this.rwlock.data_ptr() };
            // SAFETY: `this.guard` isn't used again, as `this` is wrapped in a
            // ManuallyDrop.
            let guard = unsafe { ManuallyDrop::take(&mut this.guard) };
            let guard = RwLockWriteGuardDetached {
                // SAFETY: The caller promises that the returned reference isn't
                // used beyond the detached guard's lifetime.
                rwlock: unsafe { this.rwlock.raw() },
                guard: ManuallyDrop::new(guard),
            };
            (guard, data)
        }
    }

    /// A [`RwLockWriteGuard`] without the data.
    pub struct RwLockWriteGuardDetached<'a, R: RawRwLock> {
        rwlock: &'a R,
        guard: ManuallyDrop<R::ExclusiveGuard>,
    }

    impl<R: RawRwLock> Drop for RwLockWriteGuardDetached<'_, R> {
        fn drop(&mut self) {
            // SAFETY: self.guard isn't used again, as we are in the Drop
            // implementation.
            let guard = unsafe { ManuallyDrop::take(&mut self.guard) };
            // SAFETY: All references to the locked data derived from this write
            // guard are now out of scope - as promised by the caller of
            // RwLockWriteGuard::detach() - so it's safe to unlock.
            unsafe {
                self.rwlock.unlock_exclusive(guard);
            }
        }
    }
}
