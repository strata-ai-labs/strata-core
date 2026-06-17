//! Engine control-plane bootstrap and row codecs.

mod bootstrap;
pub(crate) mod records;

pub(crate) use bootstrap::{bootstrap_or_load, ControlPlane};
