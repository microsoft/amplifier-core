#[allow(clippy::all)]
pub mod amplifier_module {
    include!("amplifier.module.rs");
}

#[cfg(test)]
mod equivalence_tests;
