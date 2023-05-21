use std::{sync::atomic::{Ordering, AtomicUsize}, future::Future, pin::Pin, io::{Write, stdout}};
use forth3::{
    leakbox::{AsyncLBForth, LBForthParams},
    dictionary::{AsyncBuiltinEntry, AsyncBuiltins, EntryHeader},fastr::FaStr,
    Forth, word::Word
};
use tokio::io::{stdin, AsyncWriteExt, AsyncBufReadExt, BufReader};

#[derive(Clone)]
struct AsyncDispatcher;
impl<'forth> AsyncBuiltins<'forth, TokioContext> for AsyncDispatcher {
    type Future = Pin<Box<dyn Future<Output = Result<(), forth3::Error>> + 'forth>>;

    // https://spf.sourceforge.net/docs/intro.en.html#task
    const BUILTINS: &'static [AsyncBuiltinEntry<TokioContext>] = &[
        forth3::async_builtin!("sleep"),
        forth3::async_builtin!("spawn"),
        forth3::async_builtin!("join"),
    ];

    fn dispatch_async(
        &self,
        id: &FaStr,
        forth: &'forth mut Forth<TokioContext>,
    ) -> Self::Future {
        static TASKS: AtomicUsize = AtomicUsize::new(1);
        match id.as_str() {
            "sleep" => {
                Box::pin(async move {
                    // Get value from top of stack
                    let ms: usize = forth.data_stack.try_pop()?.try_into()?;
                    tokio::time::sleep(tokio::time::Duration::from_millis(ms as u64)).await;
                    Ok(())
                })
            },
            "spawn" => {
                // XXX(eliza): this doesn't technically need to be an async
                // builtin but i'm lazy and i didn't want to have to redefine
                // all the default builtins...
                Box::pin(async move {
                    let w: Word = forth.data_stack.try_pop()?;
                    let hdr = unsafe {
                        w.ptr.cast::<EntryHeader<TokioContext>>().as_ref().unwrap()
                    };
                    let t0 = forth.host_ctxt.t0;
                    let mut child = AsyncLBForth::new_child(PARAMS, TokioContext {
                        join_handles: Vec::new(),
                        t0,
                    }, &*forth, AsyncDispatcher);
                    child.forth.input_mut().fill(hdr.name.as_str()).unwrap();
                    let tid = TASKS.fetch_add(1, Ordering::Relaxed);
                    tokio::task::spawn_local(async move {
                        let forth = &mut child.forth;
                        match forth.process_line().await {
                            Ok(()) => {
                                print!("[t{tid} {:?}] {}", t0.elapsed(), forth.output().as_str());
                            }
                            Err(e) => {
                                println!();
                                println!("t{tid}: Input failed. Error: {:?}", e);
                                println!("t{tid}: Unprocessed tokens:");
                                while let Some(tok) = forth.input_mut().cur_word() {
                                    print!("'{}', ", tok);
                                    forth.input_mut().advance();
                                }
                                println!();
                            }
                        }

                        println!("[t{tid} {:?}] done.", t0.elapsed());
                        drop(child);
                        // TODO(eliza): joinhandle
                    });

                    println!("[t{tid} {:?}] started.", t0.elapsed());
                    Ok(())
                })
            },
            "join" => {
                todo!("eliza");
            }
            id => panic!("Unknown async builtin {id}")
        }
    }
}

struct TokioContext {
    join_handles: Vec<tokio::task::JoinHandle<()>>,
    t0: tokio::time::Instant,
}

const PARAMS: LBForthParams = LBForthParams {
    data_stack_elems: 1024,
    return_stack_elems: 1024,
    control_stack_elems: 64,
    input_buf_elems: 1024,
    output_buf_elems: 4096,
    dict_buf_elems: 16 * 1024,
};

#[tokio::main(flavor = "current_thread")]
async fn main() {
    // Construct a local task set that can run `!Send` futures, as the forth
    // dictionary is !Send.
    let local = tokio::task::LocalSet::new();
    println!("async words:\n\tsleep (ms --)\n\tspawn (xt --)");

    local.run_until(async {
        let t0 = tokio::time::Instant::now();
        let mut lbf = AsyncLBForth::from_params(PARAMS, TokioContext { 
            join_handles: Vec::new(),
            t0,
        }, Forth::FULL_BUILTINS, AsyncDispatcher);
        let forth = &mut lbf.forth;

        let mut inp = String::new();
        let mut stdin = BufReader::new(stdin());
        loop {
            print!("[t0 {:?}] > ", t0.elapsed());
            stdout().flush().unwrap();
            stdin.read_line(&mut inp).await.unwrap();
            forth.input_mut().fill(&inp).unwrap();
            match forth.process_line().await {
                Ok(()) => {
                    print!("[t0 {:?}] {}", t0.elapsed(), forth.output().as_str());
                }
                Err(e) => {
                    println!();
                    println!("t0: Input failed. Error: {:?}", e);
                    println!("t0: Unprocessed tokens:");
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
