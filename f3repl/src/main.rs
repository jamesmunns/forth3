use std::io::{stdin, stdout, Write};

use forth3::{
    disk::{BorrowDiskMut, Disk, BinDisk},
    leakbox::{LBForth, LBForthParams, LeakBox},
    Forth,
};

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
    let caches = [c1.as_non_null(), c2.as_non_null()];
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
