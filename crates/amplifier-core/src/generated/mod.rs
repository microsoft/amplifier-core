#[allow(clippy::all)]
mod amplifier_module {
    include!("amplifier.module.rs");
}

pub use amplifier_module::*;

mod equivalence_tests;
