#[allow(clippy::all)]
pub mod amplifier_module {
    include!("amplifier.module.rs");
}

pub mod conversions;

#[cfg(test)]
mod equivalence_tests;
