#[allow(warnings)]
mod bindings;

use amplifier_guest::{Tool, ToolSpec, ToolResult, Value};

#[derive(Default)]
struct InfiniteLoopTool;

impl Tool for InfiniteLoopTool {
    fn name(&self) -> &str {
        "infinite-loop"
    }

    fn get_spec(&self) -> ToolSpec {
        // Enter an infinite loop — epoch interruption should terminate this.
        loop {
            std::hint::black_box(());
        }
    }

    fn execute(&self, _input: Value) -> Result<ToolResult, String> {
        // Also an infinite loop in execute, for completeness.
        loop {
            std::hint::black_box(());
        }
    }
}

amplifier_guest::export_tool!(InfiniteLoopTool);
