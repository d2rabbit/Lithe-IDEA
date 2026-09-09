//! External editor extension package inspection shared by both platform hosts.

mod vsix;

pub(crate) use vsix::{inspect_vsix, VsixInspectRequest};
