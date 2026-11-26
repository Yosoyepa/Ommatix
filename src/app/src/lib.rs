//! The parts of the composition root that other crates may test against: the operator's
//! local policy and stop, and the operator's grants file. Everything else stays in the
//! `ommatix` binary.

pub mod grants;
pub mod policy;
