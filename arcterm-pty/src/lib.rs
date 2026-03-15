//! arcterm-pty — PTY allocation and shell spawning.

pub mod session;

pub use session::{PtyError, PtySession};
