use std::{
    io::{stdin, stdout, Write},
    ptr::NonNull,
};

use forth3::{
    disk::{Disk, DiskError},
    leakbox::{LBForth, LBForthParams, LeakBox},
    Forth,
};

struct BinDisk;

impl forth3::disk::DiskDriver for BinDisk {
    fn read(&mut self, idx: u16, dest: NonNull<u8>, len: usize) -> Result<(), DiskError> {
        match std::fs::read(&format!("./disk/{:05}.bin", idx)) {
            Ok(v) => {
                let cap_len = v.len().min(len);
                unsafe {
                    dest.as_ptr().copy_from_nonoverlapping(v.as_ptr(), cap_len);
                    if cap_len < v.len() {
                        dest.as_ptr()
                            .add(cap_len)
                            .write_bytes(b'x', v.len() - cap_len);
                    }
                }
            }
            Err(_) => {
                let mut val = core::iter::repeat(b' ').take(len).collect::<Vec<u8>>();
                self.write(idx, NonNull::new(val.as_mut_ptr().cast()).unwrap(), len)?;
                unsafe {
                    dest.as_ptr().copy_from_nonoverlapping(val.as_ptr(), len);
                }
            }
        }
        Ok(())
    }

    fn write(&mut self, idx: u16, source: NonNull<u8>, len: usize) -> Result<(), DiskError> {
        std::fs::create_dir_all("./disk").map_err(|_| DiskError::InternalDriverError)?;
        let name = format!("./disk/{:05}.bin", idx);
        let _ = std::fs::remove_file(&name);
        std::fs::write(&name, unsafe {
            core::slice::from_raw_parts(source.as_ptr(), len)
        })
        .map_err(|_| DiskError::InternalDriverError)?;
        Ok(())
    }
}

fn block(f: &mut Forth<Disk<BinDisk>>) -> Result<(), forth3::Error> {
    let idx = f.data_stack.try_pop()?;
    let idx = u16::try_from(unsafe { idx.data })
        .map_err(|_| forth3::Error::Disk(DiskError::OutOfRange))?;
    let ptr = f.host_ctxt.block(idx).map_err(forth3::Error::Disk)?;
    f.data_stack.push(forth3::word::Word::ptr(ptr.as_ptr()))?;
    Ok(())
}

fn buffer(f: &mut Forth<Disk<BinDisk>>) -> Result<(), forth3::Error> {
    let idx = f.data_stack.try_pop()?;
    let idx = u16::try_from(unsafe { idx.data })
        .map_err(|_| forth3::Error::Disk(DiskError::OutOfRange))?;
    let ptr = f.host_ctxt.buffer(idx).map_err(forth3::Error::Disk)?;
    f.data_stack.push(forth3::word::Word::ptr(ptr.as_ptr()))?;
    Ok(())
}

fn empty_buffers(f: &mut Forth<Disk<BinDisk>>) -> Result<(), forth3::Error> {
    f.host_ctxt.empty_buffers();
    Ok(())
}

fn update(f: &mut Forth<Disk<BinDisk>>) -> Result<(), forth3::Error> {
    f.host_ctxt.mark_dirty();
    Ok(())
}

fn flush(f: &mut Forth<Disk<BinDisk>>) -> Result<(), forth3::Error> {
    f.host_ctxt.flush().map_err(forth3::Error::Disk)?;
    Ok(())
}

fn main() {
    let c1: LeakBox<u8> = LeakBox::new(512);
    let c2: LeakBox<u8> = LeakBox::new(512);
    let caches = [c1.non_null(), c2.non_null()];
    let disk = Disk::new(caches, 512, BinDisk);

    let params = LBForthParams {
        data_stack_elems: 1024,
        return_stack_elems: 1024,
        control_stack_elems: 64,
        input_buf_elems: 1024,
        output_buf_elems: 4096,
        dict_buf_elems: 16 * 1024,
    };
    let mut lbf = LBForth::from_params(params, disk, Forth::FULL_BUILTINS);
    let forth = &mut lbf.forth;
    forth.add_builtin_static_name("block", block).unwrap();
    forth.add_builtin_static_name("buffer", buffer).unwrap();
    forth
        .add_builtin_static_name("empty-buffers", empty_buffers)
        .unwrap();
    forth.add_builtin_static_name("update", update).unwrap();
    forth.add_builtin_static_name("flush", flush).unwrap();

    let mut inp = String::new();
    loop {
        print!("> ");
        stdout().flush().unwrap();
        stdin().read_line(&mut inp).unwrap();
        forth.input.fill(&inp).unwrap();
        match forth.process_line() {
            Ok(()) => {
                print!("{}", forth.output.as_str());
            }
            Err(e) => {
                println!();
                println!("Input failed. Error: {:?}", e);
                println!("Unprocessed tokens:");
                while let Some(tok) = forth.input.cur_word() {
                    print!("'{}', ", tok);
                    forth.input.advance();
                }
                println!();
            }
        }

        inp.clear();
        forth.output.clear();
    }
}
