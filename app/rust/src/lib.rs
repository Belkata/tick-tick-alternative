pub mod api;
mod changes;
#[rustfmt::skip]
mod frb_generated;
pub mod google;
pub mod kotlin;
mod phone;

uniffi::setup_scaffolding!();
