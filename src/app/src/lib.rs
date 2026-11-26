//! The parts of the composition root that other crates may test against: the operator's
//! local policy and stop, the operator's grants file, and how the operator's input files are
//! opened. Everything else stays in the `ommatix` binary.

pub mod grants;
pub mod operator_file;
pub mod policy;
