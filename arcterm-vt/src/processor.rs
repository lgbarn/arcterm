//! VT byte-stream processor — bridges vte::Parser to the Handler trait.

use crate::Handler;

/// Wraps `vte::Parser` and drives a `Handler` implementation from raw PTY bytes.
pub struct Processor {
    parser: vte::Parser,
}

impl Processor {
    pub fn new() -> Self {
        Self {
            parser: vte::Parser::new(),
        }
    }

    pub fn advance<H: Handler>(&mut self, handler: &mut H, bytes: &[u8]) {
        let mut performer = Performer { handler };
        self.parser.advance(&mut performer, bytes);
    }
}

impl Default for Processor {
    fn default() -> Self {
        Self::new()
    }
}

struct Performer<'a, H: Handler> {
    handler: &'a mut H,
}

impl<H: Handler> vte::Perform for Performer<'_, H> {}
