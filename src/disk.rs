use core::ptr::NonNull;

use crate::{word::Word, Error, Forth, WordFunc};

#[derive(Debug, PartialEq)]
pub enum DiskError {
    OutOfRange,
    InternalDriverError,
}

pub trait DiskDriver {
    fn read(&mut self, idx: u16, dest: NonNull<u8>, len: usize) -> Result<(), DiskError>;
    fn write(&mut self, idx: u16, source: NonNull<u8>, len: usize) -> Result<(), DiskError>;
}

pub trait BorrowDiskMut {
    type Driver: DiskDriver;
    fn borrow_disk_mut(&mut self) -> &mut Disk<Self::Driver>;
}

impl<D: DiskDriver> BorrowDiskMut for Disk<D> {
    type Driver = D;

    fn borrow_disk_mut(&mut self) -> &mut Disk<Self::Driver> {
        self
    }
}

pub struct Disk<D: DiskDriver> {
    // Pair of buffers. The first one is "active", the second is "oldest"
    caches: [Cache; 2],
    size: usize,
    driver: D,
}

fn block<BDM: BorrowDiskMut>(f: &mut Forth<BDM>) -> Result<(), Error> {
    let idx = f.data_stack.try_pop()?;
    let idx = u16::try_from(unsafe { idx.data }).map_err(|_| Error::Disk(DiskError::OutOfRange))?;
    let ptr = f
        .host_ctxt
        .borrow_disk_mut()
        .block(idx)
        .map_err(Error::Disk)?;
    f.data_stack.push(Word::ptr(ptr.as_ptr()))?;
    Ok(())
}

fn buffer<BDM: BorrowDiskMut>(f: &mut Forth<BDM>) -> Result<(), Error> {
    let idx = f.data_stack.try_pop()?;
    let idx = u16::try_from(unsafe { idx.data }).map_err(|_| Error::Disk(DiskError::OutOfRange))?;
    let ptr = f
        .host_ctxt
        .borrow_disk_mut()
        .buffer(idx)
        .map_err(Error::Disk)?;
    f.data_stack.push(Word::ptr(ptr.as_ptr()))?;
    Ok(())
}

fn empty_buffers<BDM: BorrowDiskMut>(f: &mut Forth<BDM>) -> Result<(), Error> {
    f.host_ctxt.borrow_disk_mut().empty_buffers();
    Ok(())
}

fn update<BDM: BorrowDiskMut>(f: &mut Forth<BDM>) -> Result<(), Error> {
    f.host_ctxt.borrow_disk_mut().mark_dirty();
    Ok(())
}

fn flush<BDM: BorrowDiskMut>(f: &mut Forth<BDM>) -> Result<(), Error> {
    f.host_ctxt.borrow_disk_mut().flush().map_err(Error::Disk)?;
    Ok(())
}

impl<BDM> Forth<BDM>
where
    BDM: BorrowDiskMut + 'static,
{
    pub const DISK_BUILTINS: &'static [(&'static str, WordFunc<BDM>)] = &[
        ("block", block),
        ("buffer", buffer),
        ("empty_buffers", empty_buffers),
        ("update", update),
        ("flush", flush),
    ];
}

impl<D> Disk<D>
where
    D: DiskDriver,
{
    pub fn new(caches: [NonNull<u8>; 2], size: usize, driver: D) -> Self {
        for c in caches.iter() {
            unsafe {
                c.as_ptr().write_bytes(b' ', size);
            }
        }
        Self {
            caches: [
                Cache {
                    buf: caches[0],
                    page: PageState::Empty,
                },
                Cache {
                    buf: caches[1],
                    page: PageState::Empty,
                },
            ],
            size,
            driver,
        }
    }

    #[inline]
    fn active_buf(&self) -> NonNull<u8> {
        self.caches[0].buf
    }

    #[inline]
    fn matches_first(&self, idx: u16) -> bool {
        self.caches[0].is_page(idx)
    }

    // returns true if we WOULD need to read
    fn make_space_for_idx(&mut self, idx: u16) -> Result<bool, DiskError> {
        if self.matches_first(idx) {
            return Ok(false);
        }

        // Either the inactive is our target, or we're going to load to that.
        // Switch to active.
        let [a, b] = &mut self.caches;
        core::mem::swap(a, b);

        // If this is already our target, skip read
        if self.caches[0].is_page(idx) {
            return Ok(false);
        }

        // Nope, not our target. Evict the old cache in our new spot
        match self.caches[0].page {
            PageState::Empty => {}
            PageState::Buffer(_) => {}
            PageState::Clean(_) => {}
            PageState::Dirty(i) => {
                self.driver.write(i, self.caches[0].buf, self.size)?;
            }
        }

        Ok(true)
    }

    pub fn flush(&mut self) -> Result<(), DiskError> {
        for c in self.caches.iter_mut() {
            match c.page {
                PageState::Empty => {}
                PageState::Buffer(_) => {}
                PageState::Clean(_) => {}
                PageState::Dirty(idx) => self.driver.write(idx, c.buf, self.size)?,
            }
            c.page = PageState::Empty;
        }
        Ok(())
    }

    pub fn buffer(&mut self, idx: u16) -> Result<NonNull<u8>, DiskError> {
        if self.make_space_for_idx(idx)? {
            // We do need to read, which means this wasn't already the
            // page. Mark it as a new buffer page
            //
            // ELSE: we don't need a read - that means we were already there.
            // Keep disk marked as whatever it was.
            self.caches[0].page = PageState::Buffer(idx);
        }

        Ok(self.active_buf())
    }

    pub fn empty_buffers(&mut self) {
        self.caches.iter_mut().for_each(|c| {
            c.page = PageState::Empty;
        });
    }

    pub fn mark_dirty(&mut self) {
        let next = match self.caches[0].page {
            PageState::Empty => {
                // This is maybe an error?
                PageState::Empty
            }
            PageState::Buffer(i) => PageState::Dirty(i),
            PageState::Clean(i) => PageState::Dirty(i),
            PageState::Dirty(i) => PageState::Dirty(i),
        };
        self.caches[0].page = next;
    }

    pub fn block(&mut self, idx: u16) -> Result<NonNull<u8>, DiskError> {
        if self.make_space_for_idx(idx)? {
            self.driver.read(idx, self.caches[0].buf, self.size)?;
            self.caches[0].page = PageState::Clean(idx);
        }

        Ok(self.active_buf())
    }

    pub fn driver(&mut self) -> &mut D {
        &mut self.driver
    }

    pub fn release(self) -> D {
        self.driver
    }
}

pub struct Cache {
    buf: NonNull<u8>,
    page: PageState,
}

impl Cache {
    fn is_page(&self, idx: u16) -> bool {
        let i = match self.page {
            PageState::Empty => return false,
            PageState::Buffer(i) => i,
            PageState::Clean(i) => i,
            PageState::Dirty(i) => i,
        };

        i == idx
    }
}

#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PageState {
    Empty,
    Buffer(u16),
    Clean(u16),
    Dirty(u16),
}

#[cfg(test)]
pub mod test {
    use core::ptr::NonNull;

    use crate::leakbox::LeakBox;

    use super::{Disk, DiskDriver, DiskError};

    #[derive(Debug, PartialEq)]
    enum Action {
        ReadFrom {
            dest: NonNull<u8>,
            idx: u16,
            len: usize,
        },
        WriteTo {
            src: NonNull<u8>,
            idx: u16,
            len: usize,
        },
    }

    #[derive(Default)]
    struct FakeDisk {
        actions: Vec<Action>,
        error_after: Option<usize>,
    }

    impl DiskDriver for FakeDisk {
        fn read(&mut self, idx: u16, dest: NonNull<u8>, len: usize) -> Result<(), DiskError> {
            match &mut self.error_after {
                Some(0) => return Err(DiskError::InternalDriverError),
                Some(i) => {
                    *i -= 1;
                }
                None => {}
            }
            self.actions.push(Action::ReadFrom { dest, idx, len });
            Ok(())
        }

        fn write(&mut self, idx: u16, source: NonNull<u8>, len: usize) -> Result<(), DiskError> {
            match &mut self.error_after {
                Some(0) => return Err(DiskError::InternalDriverError),
                Some(i) => {
                    *i -= 1;
                }
                None => {}
            }
            self.actions.push(Action::WriteTo {
                src: source,
                idx,
                len,
            });
            Ok(())
        }
    }

    #[test]
    fn smoke() {
        let fake = FakeDisk::default();
        let c1: LeakBox<u8> = LeakBox::new(512);
        let c2: LeakBox<u8> = LeakBox::new(512);
        let caches = [c1.non_null(), c2.non_null()];
        let mut disk = Disk::new(caches, 512, fake);
        assert!(disk.driver().actions.is_empty());

        let buf_01 = disk.block(123).unwrap();
        assert_eq!(
            &core::mem::take(&mut disk.driver().actions),
            &[Action::ReadFrom {
                dest: c2.non_null(),
                idx: 123,
                len: 512
            },]
        );
        assert_eq!(buf_01, c2.non_null());
        disk.mark_dirty();

        let buf_02 = disk.block(124).unwrap();
        assert_eq!(
            &core::mem::take(&mut disk.driver().actions),
            &[Action::ReadFrom {
                dest: c1.non_null(),
                idx: 124,
                len: 512
            },]
        );
        assert_eq!(buf_02, c1.non_null());

        let buf_03 = disk.block(125).unwrap();
        assert_eq!(
            &core::mem::take(&mut disk.driver().actions),
            &[
                Action::WriteTo {
                    src: c2.non_null(),
                    idx: 123,
                    len: 512
                },
                Action::ReadFrom {
                    dest: c2.non_null(),
                    idx: 125,
                    len: 512
                },
            ]
        );
        assert_eq!(buf_03, c2.non_null());

        let buf_04 = disk.block(124).unwrap();
        assert_eq!(&core::mem::take(&mut disk.driver().actions), &[]);
        assert_eq!(buf_04, c1.non_null());
        disk.mark_dirty();

        let buf_05 = disk.block(124).unwrap();
        assert_eq!(&core::mem::take(&mut disk.driver().actions), &[]);
        assert_eq!(buf_05, c1.non_null());
        disk.mark_dirty();

        let buf_06 = disk.buffer(124).unwrap();
        assert_eq!(&core::mem::take(&mut disk.driver().actions), &[]);
        assert_eq!(buf_06, c1.non_null());
        disk.mark_dirty();

        let buf_07 = disk.block(126).unwrap();
        assert_eq!(
            &core::mem::take(&mut disk.driver().actions),
            &[Action::ReadFrom {
                dest: c2.non_null(),
                idx: 126,
                len: 512
            },]
        );
        assert_eq!(buf_07, c2.non_null());

        let buf_08 = disk.block(127).unwrap();
        assert_eq!(
            &core::mem::take(&mut disk.driver().actions),
            &[
                Action::WriteTo {
                    src: c1.non_null(),
                    idx: 124,
                    len: 512
                },
                Action::ReadFrom {
                    dest: c1.non_null(),
                    idx: 127,
                    len: 512
                },
            ]
        );
        assert_eq!(buf_08, c1.non_null());
    }
}
