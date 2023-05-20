use std::io::{stdin, stdout, Write};

use forth3::{
    leakbox::{AsyncLBForth, LBForthParams},
    dictionary::{AsyncBuiltinEntry, AsyncBuiltins},fastr::FaStr,
    Forth,
};
use std::{future::Future, pin::Pin};


struct AsyncDispatcher;
impl<'forth> AsyncBuiltins<'forth, ()> for AsyncDispatcher {
    type Future = Pin<Box<dyn Future<Output = Result<(), forth3::Error>> + 'forth>>;

    const BUILTINS: &'static [AsyncBuiltinEntry<()>] = &[
        forth3::async_builtin!("sleep"),
        forth3::async_builtin!("sapwn"),
    ];

    fn dispatch_async(
        &self,
        id: &FaStr,
        forth: &'forth mut Forth<()>,
    ) -> Self::Future {
        match id.as_str() {
            "sleep" => {
                // Get value from top of stack
                let secs: usize = forth.data_stack.pop().unwrap().try_into().unwrap();
                Box::pin(async move {
                    tokio::time::sleep(tokio::time::Duration::from_secs(secs as u64)).await;
                    Ok(())
                })
            },
            "spawn" => {
                todo!("eliza: implement this!")
            }
            id => panic!("Unknown async builtin {id}")
        }
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let params = LBForthParams {
        data_stack_elems: 1024,
        return_stack_elems: 1024,
        control_stack_elems: 64,
        input_buf_elems: 1024,
        output_buf_elems: 4096,
        dict_buf_elems: 16 * 1024,
    };

    // Construct a local task set that can run `!Send` futures, as the forth
    // dictionary is !Send.
    let local = tokio::task::LocalSet::new();

    local.run_until(async {
        let t0 = tokio::time::Instant::now();
        let mut lbf = AsyncLBForth::from_params(params, (), Forth::FULL_BUILTINS, AsyncDispatcher);
        let forth = &mut lbf.forth;

        let mut inp = String::new();
        loop {
            print!("[{:?}] > ", t0.elapsed());
            stdout().flush().unwrap();
            stdin().read_line(&mut inp).unwrap();
            forth.input_mut().fill(&inp).unwrap();
            match forth.process_line().await {
                Ok(()) => {
                    print!("[{:?}] {}", t0.elapsed(), forth.output().as_str());
                }
                Err(e) => {
                    println!();
                    println!("Input failed. Error: {:?}", e);
                    println!("Unprocessed tokens:");
                    while let Some(tok) = forth.input_mut().cur_word() {
                        print!("'{}', ", tok);
                        forth.input_mut().advance();
                    }
                    println!();
                }
            }

            inp.clear();
            forth.output_mut().clear();
        }   
        Ok::<(), ()>(())
    }).await.expect("task panicked!")
}
