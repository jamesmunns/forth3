use std::{
    io::{stdin, stdout, Write},
    ptr::NonNull,
};

use forth3::{
    disk::{BorrowDiskMut, Disk, DiskError},
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

struct ReplContext {
    disk: Disk<BinDisk>,
}

impl BorrowDiskMut for ReplContext {
    type Driver = BinDisk;

    fn borrow_disk_mut(&mut self) -> &mut Disk<Self::Driver> {
        &mut self.disk
    }
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
    let mut lbf = LBForth::from_params(params, ReplContext { disk }, Forth::FULL_BUILTINS);
    let forth = &mut lbf.forth;
    for (name, bif) in forth3::Forth::<ReplContext>::DISK_BUILTINS {
        forth.add_builtin_static_name(name, *bif).unwrap();
    }

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
