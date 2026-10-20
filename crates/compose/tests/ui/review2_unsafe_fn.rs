// Review 2: an `unsafe fn` composable isn't rejected. Its restart function, a safe `fn` pointer
// the runtime calls from safe code (`Composition::recompose`) whenever the composable's state
// changes, calls the `unsafe fn` again with the kept arguments, without an `unsafe` block and
// without any caller upholding its safety contract: safe code can then reach UB (here a read
// through an address that may dangle by then). rustc reports nothing, not even
// `unsafe_op_in_unsafe_fn`: the call is in macro-generated code.
use compose::runtime::*;

#[composable]
unsafe fn Peek(address: usize) {
    let trigger = remember(|| mutable_state_of(0u32));
    let _ = trigger.get();
    let _ = unsafe { std::ptr::read_volatile(address as *const u8) };
}

fn main() {}
