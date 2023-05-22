use std::{
    alloc::{GlobalAlloc, Layout, System},
    cell::UnsafeCell,
    mem::MaybeUninit,
    ptr::NonNull,
};

use crate::{
    dictionary::{BuiltinEntry, DropDict, OwnedDict, Dictionary}, input::WordStrBuf, output::OutputBuf, word::Word, CallContext, Forth,
};

#[cfg(feature = "async")]
use crate::{AsyncForth, dictionary::{AsyncBuiltins}};

// Helper type that will un-leak the buffer once it is dropped.
pub struct LeakBox<T> {
    ptr: *mut UnsafeCell<MaybeUninit<T>>,
    len: usize,
}

impl<T> LeakBox<T> {
    pub fn new(len: usize) -> Self {
        Self {
            ptr: unsafe {
                System
                    .alloc(Layout::array::<UnsafeCell<MaybeUninit<T>>>(len).unwrap())
                    .cast()
            },
            len,
        }
    }

    pub fn ptr(&self) -> *mut T {
        self.ptr.cast()
    }

    pub fn len(&self) -> usize {
        self.len
    }
}

impl<T> Drop for LeakBox<T> {
    fn drop(&mut self) {
        unsafe {
            System.dealloc(
                self.ptr.cast(),
                Layout::array::<UnsafeCell<MaybeUninit<T>>>(self.len).unwrap(),
            )
        }
    }
}

pub struct LBForthParams {
    pub data_stack_elems: usize,
    pub return_stack_elems: usize,
    pub control_stack_elems: usize,
    pub input_buf_elems: usize,
    pub output_buf_elems: usize,
    pub dict_buf_elems: usize,
}

#[derive(Copy, Clone)]
struct LeakBoxDict;

impl Default for LBForthParams {
    fn default() -> Self {
        Self {
            data_stack_elems: 256,
            return_stack_elems: 256,
            control_stack_elems: 256,
            input_buf_elems: 256,
            output_buf_elems: 256,
            dict_buf_elems: 4096,
        }
    }
}

pub struct LBForth<T: 'static> {
    pub forth: Forth<T>,
    _payload_dstack: LeakBox<Word>,
    _payload_rstack: LeakBox<Word>,
    _payload_cstack: LeakBox<CallContext<T>>,
    _input_buf: LeakBox<u8>,
    _output_buf: LeakBox<u8>,
}

#[cfg(feature = "async")]
pub struct AsyncLBForth<T: 'static, A> {
    pub forth: AsyncForth<T, A>,
    _payload_dstack: LeakBox<Word>,
    _payload_rstack: LeakBox<Word>,
    _payload_cstack: LeakBox<CallContext<T>>,
    _input_buf: LeakBox<u8>,
    _output_buf: LeakBox<u8>,
}

impl<T: 'static> LBForth<T> {
    pub fn from_params(
        params: LBForthParams,
        host_ctxt: T,
        builtins: &'static [BuiltinEntry<T>],
    ) -> Self {
        let _payload_dstack: LeakBox<Word> = LeakBox::new(params.data_stack_elems);
        let _payload_rstack: LeakBox<Word> = LeakBox::new(params.return_stack_elems);
        let _payload_cstack: LeakBox<CallContext<T>> = LeakBox::new(params.control_stack_elems);
        let _input_buf: LeakBox<u8> = LeakBox::new(params.input_buf_elems);
        let _output_buf: LeakBox<u8> = LeakBox::new(params.output_buf_elems);

        let input = WordStrBuf::new(_input_buf.ptr(), _input_buf.len());
        let output = OutputBuf::new(_output_buf.ptr(), _output_buf.len());
        let forth = unsafe {
            Forth::<T>::new(
                (_payload_dstack.ptr(), _payload_dstack.len()),
                (_payload_rstack.ptr(), _payload_rstack.len()),
                (_payload_cstack.ptr(), _payload_cstack.len()),
                alloc_dict(params.dict_buf_elems),
                input,
                output,
                host_ctxt,
                builtins,
            )
            .unwrap()
        };

        Self {
            forth,
            _payload_dstack,
            _payload_rstack,
            _payload_cstack,
            _input_buf,
            _output_buf,
        }
    }

    pub fn fork_with_params(&mut self, params: LBForthParams, host_ctxt: T) -> Self {
        let _payload_dstack: LeakBox<Word> = LeakBox::new(params.data_stack_elems);
        let _payload_rstack: LeakBox<Word> = LeakBox::new(params.return_stack_elems);
        let _payload_cstack: LeakBox<CallContext<T>> = LeakBox::new(params.control_stack_elems);
        let _input_buf: LeakBox<u8> = LeakBox::new(params.input_buf_elems);
        let _output_buf: LeakBox<u8> = LeakBox::new(params.output_buf_elems);

        let my_new_dict = alloc_dict(params.dict_buf_elems);
        let new_dict = alloc_dict(params.dict_buf_elems);

        let input = WordStrBuf::new(_input_buf.ptr(), _input_buf.len());
        let output = OutputBuf::new(_output_buf.ptr(), _output_buf.len());
        let forth = unsafe { 
            self.forth.fork(
                my_new_dict,
                new_dict,
                (_payload_dstack.ptr(), _payload_dstack.len()),
                (_payload_rstack.ptr(), _payload_rstack.len()),
                (_payload_cstack.ptr(), _payload_cstack.len()),
                input,
                output,
                host_ctxt,
            ).unwrap()
        };
        Self {
            forth,
            _payload_dstack,
            _payload_rstack,
            _payload_cstack,
            _input_buf,
            _output_buf,
        }
    }
}

#[cfg(feature = "async")]
impl<T, D> AsyncLBForth<T, D>
where
    T: 'static,
    D: for<'forth> AsyncBuiltins<'forth, T>,
{
    pub fn from_params(
        params: LBForthParams,
        host_ctxt: T,
        sync_builtins: &'static [BuiltinEntry<T>],
        dispatcher: D
    ) -> Self {
        let _payload_dstack: LeakBox<Word> = LeakBox::new(params.data_stack_elems);
        let _payload_rstack: LeakBox<Word> = LeakBox::new(params.return_stack_elems);
        let _payload_cstack: LeakBox<CallContext<T>> = LeakBox::new(params.control_stack_elems);
        let _input_buf: LeakBox<u8> = LeakBox::new(params.input_buf_elems);
        let _output_buf: LeakBox<u8> = LeakBox::new(params.output_buf_elems);

        let input = WordStrBuf::new(_input_buf.ptr(), _input_buf.len());
        let output = OutputBuf::new(_output_buf.ptr(), _output_buf.len());
        let forth = unsafe {
            AsyncForth::<T, D>::new(
                (_payload_dstack.ptr(), _payload_dstack.len()),
                (_payload_rstack.ptr(), _payload_rstack.len()),
                (_payload_cstack.ptr(), _payload_cstack.len()),
                alloc_dict(params.dict_buf_elems),
                input,
                output,
                host_ctxt,
                sync_builtins,
                dispatcher,
            )
            .unwrap()
        };

        Self {
            forth,
            _payload_dstack,
            _payload_rstack,
            _payload_cstack,
            _input_buf,
            _output_buf,
        }
    }

    pub fn fork_with_params(&mut self, params: LBForthParams, host_ctxt: T) -> Self
    where D: Clone {
        let _payload_dstack: LeakBox<Word> = LeakBox::new(params.data_stack_elems);
        let _payload_rstack: LeakBox<Word> = LeakBox::new(params.return_stack_elems);
        let _payload_cstack: LeakBox<CallContext<T>> = LeakBox::new(params.control_stack_elems);
        let _input_buf: LeakBox<u8> = LeakBox::new(params.input_buf_elems);
        let _output_buf: LeakBox<u8> = LeakBox::new(params.output_buf_elems);

        let my_new_dict = alloc_dict(params.dict_buf_elems);
        let new_dict = alloc_dict(params.dict_buf_elems);

        let input = WordStrBuf::new(_input_buf.ptr(), _input_buf.len());
        let output = OutputBuf::new(_output_buf.ptr(), _output_buf.len());
        let forth = unsafe { 
            self.forth.fork(
                my_new_dict,
                new_dict,
                (_payload_dstack.ptr(), _payload_dstack.len()),
                (_payload_rstack.ptr(), _payload_rstack.len()),
                (_payload_cstack.ptr(), _payload_cstack.len()),
                input,
                output,
                host_ctxt,
            ).unwrap()
        };
        Self {
            forth,
            _payload_dstack,
            _payload_rstack,
            _payload_cstack,
            _input_buf,
            _output_buf,
        }
    }
}

impl DropDict for LeakBoxDict {
    unsafe fn drop_dict(ptr: NonNull<u8>, layout: Layout) {
        System.dealloc(ptr.cast().as_ptr(), layout)
    }
}

fn alloc_dict<T>(size: usize) -> OwnedDict<T> {
    let layout = match Dictionary::<T>::layout(size) {
        Ok(layout) => layout,
        Err(error) => panic!("Dictionary size {size} too large to allocate: {error}"),
    };
    let ptr = unsafe { NonNull::new(System.alloc(layout)).unwrap().cast() };
    OwnedDict::new::<LeakBoxDict>(ptr, size)
}