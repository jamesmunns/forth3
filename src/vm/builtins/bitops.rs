use crate::{Forth, Word, Error};

impl<T: 'static> Forth<T> {
    pub fn bitand(&mut self) -> Result<(), Error> {
        let a = self.data_stack.try_pop()?;
        let b = self.data_stack.try_pop()?;
        let val = Word::data(unsafe { a.data & b.data });
        self.data_stack.push(val)?;
        Ok(())
    }

    pub fn bitor(&mut self) -> Result<(), Error> {
        let a = self.data_stack.try_pop()?;
        let b = self.data_stack.try_pop()?;
        let val = Word::data(unsafe { a.data | b.data });
        self.data_stack.push(val)?;
        Ok(())
    }

    pub fn bitxor(&mut self) -> Result<(), Error> {
        let a = self.data_stack.try_pop()?;
        let b = self.data_stack.try_pop()?;
        let val = Word::data(unsafe { a.data ^ b.data });
        self.data_stack.push(val)?;
        Ok(())
    }

    pub fn bitshl(&mut self) -> Result<(), Error> {
        let a = self.data_stack.try_pop()?;
        let b = self.data_stack.try_pop()?;
        let val = Word::data(unsafe { b.data << a.data });
        self.data_stack.push(val)?;
        Ok(())
    }

    pub fn bitshr(&mut self) -> Result<(), Error> {
        let a = self.data_stack.try_pop()?;
        let b = self.data_stack.try_pop()?;
        let val = Word::data(unsafe { b.data >> a.data });
        self.data_stack.push(val)?;
        Ok(())
    }
}
