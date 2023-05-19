use super::*;

pub struct AsyncForth<T: 'static, D> {
    vm: Forth<T>,
    dispatcher: D,
}

impl<T, D> AsyncForth<T, D>
where
    T: 'static,
    D: for<'forth> DispatchAsync<'forth, T>,
{
    pub unsafe fn new(
        dstack_buf: (*mut Word, usize),
        rstack_buf: (*mut Word, usize),
        cstack_buf: (*mut CallContext<T>, usize),
        dict_buf: (*mut u8, usize),
        input: WordStrBuf,
        output: OutputBuf,
        host_ctxt: T,
        sync_builtins: &'static [BuiltinEntry<T>],
        dispatcher: D,
    ) -> Result<Self, Error> {
        let vm = Forth::new_async(dstack_buf, rstack_buf, cstack_buf, dict_buf, input, output, host_ctxt, sync_builtins, D::ASYNC_BUILTINS)?;
        Ok(Self { vm, dispatcher })
    }

    pub fn output(&self) -> &OutputBuf {
        &self.vm.output
    }

    pub fn output_mut(&mut self) -> &mut OutputBuf {
        &mut self.vm.output
    }

    pub fn input_mut(&mut self) -> &mut WordStrBuf {
        &mut self.vm.input
    }

    pub fn add_sync_builtin_static_name(
        &mut self,
        name: &'static str,
        bi: WordFunc<T>,
    ) -> Result<(), Error> {
        self.vm.add_builtin_static_name(name, bi)
    }

    pub fn add_sync_builtin(&mut self, name: &str, bi: WordFunc<T>) -> Result<(), Error> {
        self.vm.add_builtin(name, bi)
    }

    #[cfg(test)]
    pub(crate) fn vm_mut(&mut self) -> &mut Forth<T> {
        &mut self.vm
    }

    pub async fn process_line(&mut self) -> Result<(), Error> {
        let res = async {
            loop {
                match self.vm.start_processing_line()? {
                    ProcessAction::Done => {
                        self.vm.output.push_str("ok.\n")?;
                        break Ok(());
                    },
                    ProcessAction::Continue => {},
                    ProcessAction::Execute =>
                        while self.async_pig().await? != Step::Done {},
                }
            }
        }.await;
        match res {
            Ok(_) => Ok(()),
            Err(e) => {
                self.vm.data_stack.clear();
                self.vm.return_stack.clear();
                self.vm.call_stack.clear();
                Err(e)
            }
        }
    }

    // Single step execution (async version).
    async fn async_pig(&mut self) -> Result<Step, Error> {
        let Self { ref mut vm, ref dispatcher } = self;

        let top = match vm.call_stack.try_peek() {
            Ok(t) => t,
            Err(StackError::StackEmpty) => return Ok(Step::Done),
            Err(e) => return Err(Error::Stack(e)),
        };

        let kind = unsafe { top.eh.as_ref().kind };
        let res = unsafe { match kind {
            EntryKind::StaticBuiltin => (top.eh.cast::<BuiltinEntry<T>>().as_ref().func)(vm),
            EntryKind::RuntimeBuiltin => (top.eh.cast::<BuiltinEntry<T>>().as_ref().func)(vm),
            EntryKind::Dictionary => (top.eh.cast::<DictionaryEntry<T>>().as_ref().func)(vm),
            EntryKind::AsyncBuiltin => {
                dispatcher.dispatch_async(&top.eh.as_ref().name, vm).await
            },
        }};

        match res {
            Ok(_) => {
                let _ = vm.call_stack.pop();
            }
            Err(Error::PendingCallAgain) => {
                // ok, just don't pop
            }
            Err(e) => return Err(e),
        }

        Ok(Step::NotDone)
    }
}
