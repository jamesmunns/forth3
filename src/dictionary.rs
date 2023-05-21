use crate::fastr::FaStr;
use crate::{Word, WordFunc};
use core::alloc::Layout;
use core::marker::PhantomData;
use core::ptr::addr_of_mut;
use core::ptr::NonNull;

#[derive(Debug, PartialEq)]
pub enum BumpError {
    OutOfMemory,
    CantAllocUtf8,
}

#[derive(Debug, Clone, Copy)]
#[repr(u16)]
pub enum EntryKind {
    StaticBuiltin,
    RuntimeBuiltin,
    Dictionary,
    #[cfg(feature = "async")]
    AsyncBuiltin,
}

#[repr(C)]
pub struct EntryHeader<T: 'static> {
    pub name: FaStr,
    pub kind: EntryKind, // todo
    pub len: u16,
    pub _pd: PhantomData<T>,
}

#[repr(C)]
pub struct BuiltinEntry<T: 'static> {
    pub hdr: EntryHeader<T>,
    pub func: WordFunc<T>,
}

/// A dictionary entry for an asynchronous builtin word.
///
/// This type is typically created using the [`async_builtin!`
/// macro](crate::async_builtin), and is used with the
/// [`AsyncForth`](crate::AsyncForth) VM type only. See the [documentation for
/// `AsyncForth`](crate::AsyncForth) for details on using asynchronous builtin
/// words.
#[repr(C)]
#[cfg(feature = "async")]
pub struct AsyncBuiltinEntry<T: 'static> {
    pub hdr: EntryHeader<T>,
}

// Starting FORTH: page 220
#[repr(C)]
pub struct DictionaryEntry<T: 'static> {
    pub hdr: EntryHeader<T>,
    pub func: WordFunc<T>,

    /// Link field, points back to the previous entry
    pub(crate) link: Option<NonNull<DictionaryEntry<T>>>,

    /// data OR an array of compiled code.
    /// the first word is the "p(arameter)fa" or "c(ode)fa"
    pub(crate) parameter_field: [Word; 0],
}

pub struct Dictionary<T: 'static> {
    pub(crate) alloc: DictionaryBump,
    pub(crate) tail: Option<NonNull<DictionaryEntry<T>>>,
}

pub(crate) struct EntryBuilder<'dict, T: 'static> {
    dict: &'dict mut Dictionary<T>,
    len: u16,
    base: NonNull<DictionaryEntry<T>>,
    kind: EntryKind,
}

pub(crate) struct DictionaryBump {
    pub(crate) start: *mut u8,
    pub(crate) cur: *mut u8,
    pub(crate) end: *mut u8,
}

/// Iterator over a [`Dictionary`]'s entries.
pub(crate) struct Entries<'dict, T: 'static> {
    next: Option<NonNull<DictionaryEntry<T>>>,
    /// Ensure that the `Entries` iterator is bound to the dictionary's
    /// lifetime.
    _dict: PhantomData<&'dict Dictionary<T>>,
}

#[cfg(feature = "async")]
/// A set of asynchronous builtin words, and a method to dispatch builtin names
/// to [`Future`]s.
///
/// This trait is used along with the [`AsyncForth`] type to
/// allow some builtin words to be implemented by `async fn`s (or [`Future`]s),
/// rather than synchronous functions. See [here][async-vms] for an overview of
/// how asynchronous Forth VMs work.
///
/// # Implementing Async Builtins
///
/// Synchronous builtins are provided to the Forth VM as a static slice of
/// [`BuiltinEntry`]s. These entries allow the VM to lookup builtin words by
/// name, and also contain a function pointer to the host function that
/// implements that builtin. Asynchronous builtins work somewhat differently: a
/// slice of [`AsyncBuiltinEntry`]s is still used in order to define the names
/// of the asynchronous builtin words, but because asynchronous functions return
/// a [`Future`] whose type must be known, an [`AsyncBuiltinEntry`] does *not*
/// contain a function pointer to a host function. Instead, once the name of an
/// async builtin is looked up, it is passed to the
/// [`AsyncBuiltins::dispatch_async`] method, which returns the [`Future`]
/// corresponding to that builtin function.
///
/// This indirection allows the `AsyncBuiltins` trait to erase the various
/// [`Future`] types which are returned by the async builtin functions, allowing
/// the [`AsyncForth`] VM to have only a single additional generic parameter for
/// the `AsyncBuiltins` implementation itself. Without the indirection of
/// [`dispatch_async`], the [`AsyncForth`] VM would need to be generic over
/// *every* possible [`Future`] type that may be returned by an async builtin
/// word, which would be impractical.[^1]
///
/// In order to erase multiple [`Future`] types, one of several approaches may
/// be used:
///
/// - The [`Future`] returned by [`dispatch_async`] can be an [`enum`] of each
///   builtin word's [`Future`] type. This requires all builtin words to be
///   implemented as named [`Future`] types, rather than [`async fn`]s, but
///   does not require heap allocation or unstable Rust features.
/// - The [`Future`] type can be a `Pin<Box<dyn Future<Output = Result<(),
///   Error>> + 'forth>`. This requires heap allocation, but can erase the type
///   of any number of async builtin futures, which may be [`async fn`]s _or_
///   named [`Future`] types.
/// - If using nightly Rust, the
///   [`#![feature(impl_trait_in_assoc_type)]`][63063] unstable feature can be
///   enabled, allowing the [`AsyncBuiltins::Future`] associated type to be
///   `impl Future<Output = Result(), Error> + 'forth`. This does not require
///   heap allocation, and allows the [`dispatch_async`] method to return an
///   [`async`] block [`Future`] which [`match`]es on the builtin's name and
///   calls any number of [`async fn`]s or named [`Future`] types. This is the
///   preferred approach when nightly features may be used.
///
/// Since the [`AsyncBuiltins`] trait is generic over the lifetime for which the
/// [`Forth`] vm is borrowed mutably, the [`AsyncBuiltins::Future`] associated
/// type may also be generic over that lifetime. This allows the returned
/// [`Future`] to borrow the [`Forth`] VM so that its stacks can be mutated
/// while the builtin [`Future`] executes (e.g. the result of the asynchronous
/// operation can be pushed to the VM's `data` stack, et cetera).
///
/// [^1]: If the [`AsyncForth`] type was generic over every possible async
///     builtin future, it would have a large number of generic type parameters
///     which would all need to be filled in by the user. Additionally, because
///     Rust does not allow a type to have a variadic number of generic
///     parameters, there would have to be an arbitrary limit on the maximum
///     number of async builtin words.
///
/// [`AsyncForth`]: crate::AsyncForth
/// [`Future`]: core::future::Future
/// [async-vms]: crate::AsyncForth#asynchronous-forth-vms
/// [`async fn`]: https://doc.rust-lang.org/stable/std/keyword.async.html
/// [`async`]: https://doc.rust-lang.org/stable/std/keyword.async.html
/// [`dispatch_async`]: Self::dispatch_async
/// [`enum`]: https://doc.rust-lang.org/stable/std/keyword.enum.html
/// [`match`]: https://doc.rust-lang.org/stable/std/keyword.match.html
/// [`Forth`]: crate::Forth
/// [63063]: https://github.com/rust-lang/rust/issues/63063
pub trait AsyncBuiltins<'forth, T: 'static> {
    /// The [`Future`] type returned by [`Self::dispatch_async`].
    ///
    /// Since the `AsyncBuiltins` trait is generic over the lifetime of the
    /// [`Forth`](crate::Forth) VM, the [`Future`] type may mutably borrow the
    /// VM. This allows the VM's stacks to be mutated by the async builtin function.
    ///
    /// [`Future`]: core::future::Future
    type Future: core::future::Future<Output = Result<(), crate::Error>>;

    /// A static slice of [`AsyncBuiltinEntry`]s describing the builtins
    /// provided by this implementation of `AsyncBuiltin`s.
    ///
    /// [`AsyncBuiltinEntry`]s may be created using the
    /// [`async_builtin!`](crate::async_builtin) macro.
    const BUILTINS: &'static [AsyncBuiltinEntry<T>];

    /// Dispatch a builtin name (`id`) to an asynchronous builtin [`Future`].
    ///
    /// The returned [`Future`] may borrow the [`Forth`](crate::Forth) VM
    /// provided as an argument to this function, allowing it to mutate the VM's
    /// stacks as it executes.
    ///
    /// This method should return a [`Future`] for each builtin function
    /// definition in [`Self::BUILTINS`]. Typically, this is implemented by
    /// [`match`]ing the provided `id`, and returning the appropriate [`Future`]
    /// for each builtin name. See [the `AsyncBuiltin` trait's
    /// documentation][impling] for details on implementing this method.
    ///
    /// [`Future`]: core::future::Future
    /// [`match`]: https://doc.rust-lang.org/stable/std/keyword.match.html
    /// [impling]: #implementing-async-builtins
    fn dispatch_async(&self, id: &FaStr, forth: &'forth mut crate::Forth<T>) -> Self::Future;
}

impl<T: 'static> DictionaryEntry<T> {
    pub unsafe fn pfa(this: NonNull<Self>) -> NonNull<Word> {
        let ptr = this.as_ptr();
        let pfp: *mut [Word; 0] = addr_of_mut!((*ptr).parameter_field);
        NonNull::new_unchecked(pfp.cast::<Word>())
    }

    pub fn parameters(&self) -> &[Word] {
        let pfp = self.parameter_field.as_ptr();
        unsafe { core::slice::from_raw_parts(pfp, self.hdr.len as usize) }
    }
}

impl<T: 'static> Dictionary<T> {
    pub(crate) fn new(bottom: *mut u8, size: usize) -> Self {
        Self {
            alloc: DictionaryBump::new(bottom, size),
            tail: None,
        }
    }

    pub(crate) fn add_bi_fastr(&mut self, name: FaStr, bi: WordFunc<T>) -> Result<(), BumpError> {
        // Allocate and initialize the dictionary entry
        let dict_base = self.alloc.bump::<DictionaryEntry<T>>()?;
        unsafe {
            dict_base.as_ptr().write(DictionaryEntry {
                hdr: EntryHeader {
                    name,
                    kind: EntryKind::RuntimeBuiltin,
                    len: 0,
                    _pd: PhantomData,
                },
                func: bi,
                link: self.tail.take(),
                parameter_field: [],
            });
        }
        self.tail = Some(dict_base);
        Ok(())
    }

    pub(crate) fn build_entry(&mut self) -> Result<EntryBuilder<'_, T>, BumpError> {
        let base = self.alloc.bump::<DictionaryEntry<T>>()?;
        Ok(EntryBuilder {
            base,
            len: 0,
            dict: self,
            kind: EntryKind::Dictionary,
        })
    }

    pub(crate) fn entries(&self) -> Entries<'_, T> {
        Entries {
            next: self.tail,
            _dict: PhantomData,
        }
    }

    /// Performs a deep copy of all entries in `self` into `other`.
    ///
    /// This is an *O*(*entries*) operation, as it traverses all entries in
    /// `self` and constructs new entries in `other` with the same data. This
    /// means that all pointers in the `other` dictionary should point into
    /// `other`'s bump arena, rather than `self`'s. Changes to bindings in
    /// `self` after a deep copy is performed will not effect bindings in
    /// `other`, and changes to bindings in `other` will not effect the existing
    /// bindings in `self`.
    ///
    /// # Errors
    ///
    /// This method returns an error if `other`'s bump arena lacks sufficient
    /// capacity to store all the entries in `self`.
    pub(crate) fn deep_copy(&self, other: &mut Self) -> Result<(), BumpError> {
        // before doing a partial copy, check that other has room for everything
        let remaining = other.alloc.capacity() - other.alloc.used();
        if remaining < self.alloc.used() {
            return Err(BumpError::OutOfMemory);
        }

        for entry in self.entries() {
            debug_assert!(matches!(entry.hdr.kind, EntryKind::RuntimeBuiltin | EntryKind::Dictionary));
            // TODO(eliza): i think this might be potentially wasteful if the
            // entry's name came from a `'static str` rather than a runtime
            // string...can we track that and avoid interning it into the new dict?
            let name = other.alloc.bump_str(entry.hdr.name.as_str())?;

            // NOTE(eliza): the capacity check means we should be able to unwrap
            // this, I think, but whatever...
            let mut other_entry = other.build_entry()?;
            for word in entry.parameters() {
                other_entry = other_entry.write_word(*word)?;
            }
            other_entry.kind(entry.hdr.kind).finish(name, entry.func);
        }

        Ok(())
    }
}

impl<T> EntryBuilder<'_, T> {
    pub(crate) fn write_word(mut self, word: Word) -> Result<Self, BumpError> {
        self.dict.alloc.bump_write(word)?;
        self.len += 1;
        Ok(self)
    }

    fn kind(self, kind: EntryKind) -> Self {
        Self { kind, ..self }
    }

    pub(crate) fn finish(self, name: FaStr, func: WordFunc<T>) {
        unsafe {
            self.base.as_ptr().write(DictionaryEntry {
                hdr: EntryHeader {
                    name,
                    kind: self.kind,
                    len: self.len,
                    _pd: PhantomData
                },
                // TODO: Should arrays push length and ptr? Or just ptr?
                //
                // TODO: Should we look up `(variable)` for consistency?
                // Use `find_word`?
                func,

                // Don't link until we know we have a "good" entry!
                link: self.dict.tail.take(),
                parameter_field: [],
            });
        }
        self.dict.tail = Some(self.base);
    }
}

impl DictionaryBump {
    fn new(bottom: *mut u8, size: usize) -> Self {
        let end = bottom.wrapping_add(size);
        debug_assert!(end >= bottom);
        Self {
            end,
            start: bottom,
            cur: bottom,
        }
    }

    pub fn bump_str(&mut self, s: &str) -> Result<FaStr, BumpError> {
        debug_assert!(!s.is_empty());

        let len = s.len().min(31);
        let astr = &s.as_bytes()[..len];

        if !astr.iter().all(|b| b.is_ascii()) {
            return Err(BumpError::CantAllocUtf8);
        }
        let stir = self.bump_u8s(len).ok_or(BumpError::OutOfMemory)?.as_ptr();
        for (i, ch) in astr.iter().enumerate() {
            unsafe {
                stir.add(i).write(ch.to_ascii_lowercase());
            }
        }
        unsafe { Ok(FaStr::new(stir, len)) }
    }

    pub fn bump_u8s(&mut self, n: usize) -> Option<NonNull<u8>> {
        if n == 0 {
            return None;
        }

        let req = self.cur.wrapping_add(n);

        if req > self.end {
            None
        } else {
            let ptr = self.cur;
            self.cur = req;
            Some(unsafe { NonNull::new_unchecked(ptr) })
        }
    }

    pub fn bump_u8(&mut self) -> Option<NonNull<u8>> {
        if self.cur >= self.end {
            None
        } else {
            let ptr = self.cur;
            self.cur = self.cur.wrapping_add(1);
            Some(unsafe { NonNull::new_unchecked(ptr) })
        }
    }

    pub fn bump<T: Sized>(&mut self) -> Result<NonNull<T>, BumpError> {
        let offset = self.cur.align_offset(Layout::new::<T>().align());

        // Zero out any padding bytes!
        unsafe {
            self.cur.write_bytes(0x00, offset);
        }

        let align_cur = self.cur.wrapping_add(offset);
        let new_cur = align_cur.wrapping_add(Layout::new::<T>().size());

        if new_cur > self.end {
            Err(BumpError::OutOfMemory)
        } else {
            self.cur = new_cur;
            Ok(unsafe { NonNull::new_unchecked(align_cur.cast()) })
        }
    }

    pub fn bump_write<T: Sized>(&mut self, val: T) -> Result<(), BumpError> {
        let nnt = self.bump::<T>()?;
        unsafe {
            nnt.as_ptr().write(val);
        }
        Ok(())
    }

    /// Is the given pointer within the dictionary range?
    pub fn contains(&self, ptr: *mut ()) -> bool {
        let pau = ptr as usize;
        let sau = self.start as usize;
        let eau = self.end as usize;
        (pau >= sau) && (pau < eau)
    }

    pub fn capacity(&self) -> usize {
        (self.end as usize) - (self.start as usize)
    }

    pub fn used(&self) -> usize {
        (self.cur as usize) - (self.start as usize)
    }
}

// === impl Entries ===

impl<'dict, T: 'static> Iterator for Entries<'dict, T> {
    type Item = &'dict DictionaryEntry<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.next.take()?;
        let entry = unsafe {
            // Safety: `self.next` must be a pointer into the VM's dictionary
            // entries. The caller who constructs a `Entries` iterator is
            // responsible for ensuring this.
            entry.as_ref()
        };
        self.next = entry.link;
        Some(entry)
    }
}

#[cfg(test)]
pub mod test {
    use core::mem::size_of;
    use std::alloc::Layout;

    use crate::{
        dictionary::{DictionaryBump, DictionaryEntry, BuiltinEntry},
        leakbox::LeakBox,
        Word,
    };

    #[cfg(feature = "async")]
    use super::AsyncBuiltinEntry;

    use super::EntryHeader;

    #[test]
    fn sizes() {
        assert_eq!(size_of::<EntryHeader<()>>(), 3 * size_of::<usize>());
        assert_eq!(size_of::<BuiltinEntry<()>>(), 4 * size_of::<usize>());
        #[cfg(feature = "async")]
        assert_eq!(size_of::<AsyncBuiltinEntry<()>>(), 3 * size_of::<usize>());
    }

    #[test]
    fn do_a_bump() {
        let payload: LeakBox<u8> = LeakBox::new(256);

        let mut bump = DictionaryBump::new(payload.ptr(), payload.len());

        // Be annoying
        let _b = bump.bump_u8().unwrap();

        // ALLOT 10
        let d = bump.bump::<DictionaryEntry<()>>().unwrap();
        assert_eq!(
            d.as_ptr()
                .align_offset(Layout::new::<DictionaryEntry<()>>().align()),
            0
        );

        let walign = Layout::new::<DictionaryEntry<()>>().align();
        for _w in 0..10 {
            let w = bump.bump::<Word>().unwrap();
            assert_eq!(w.as_ptr().align_offset(walign), 0);
        }
    }
}
