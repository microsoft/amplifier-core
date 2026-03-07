#[allow(warnings)]
mod bindings;

use amplifier_guest::{ContextManager, Value};
use std::sync::Mutex;

static MESSAGES: Mutex<Vec<Value>> = Mutex::new(Vec::new());

#[derive(Default)]
struct MemoryContext;

impl ContextManager for MemoryContext {
    fn add_message(&self, message: Value) -> Result<(), String> {
        MESSAGES
            .lock()
            .map_err(|e| format!("poisoned mutex: {e}"))?
            .push(message);
        Ok(())
    }

    fn get_messages(&self) -> Result<Vec<Value>, String> {
        Ok(MESSAGES
            .lock()
            .map_err(|e| format!("poisoned mutex: {e}"))?
            .clone())
    }

    fn get_messages_for_request(&self, _request: Value) -> Result<Vec<Value>, String> {
        // No budget trimming — return all messages.
        self.get_messages()
    }

    fn set_messages(&self, messages: Vec<Value>) -> Result<(), String> {
        let mut store = MESSAGES
            .lock()
            .map_err(|e| format!("poisoned mutex: {e}"))?;
        *store = messages;
        Ok(())
    }

    fn clear(&self) -> Result<(), String> {
        MESSAGES
            .lock()
            .map_err(|e| format!("poisoned mutex: {e}"))?
            .clear();
        Ok(())
    }
}

amplifier_guest::export_context!(MemoryContext);
