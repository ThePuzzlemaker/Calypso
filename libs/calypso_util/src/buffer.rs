use calypso_base::span::Span;
use calypso_diagnostic::{diagnostic, error};
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Buffer<'buf> {
    buffer: &'buf [char],
    start: usize,
    current: usize,
}

impl<'buf> Buffer<'buf> {
    pub fn new(buffer: &'buf [char]) -> Self {
        Self {
            buffer,
            start: 0,
            current: 0,
        }
    }

    pub fn start(&self) -> usize {
        self.start
    }

    pub fn current(&self) -> usize {
        self.current
    }

    pub fn buffer(&self) -> &'buf [char] {
        self.buffer
    }

    pub fn is_at_end(&self) -> bool {
        self.current >= self.buffer.len()
    }

    pub fn peek(&self) -> Option<char> {
        self.buffer.get(self.current).copied()
    }

    pub fn peek_next(&self) -> Option<char> {
        self.buffer.get(self.current + 1).copied()
    }

    pub fn peek_2(&self) -> Option<char> {
        self.buffer.get(self.current + 2).copied()
    }

    pub fn last(&self) -> Option<char> {
        if self.current == 0 {
            None
        } else {
            self.buffer.get(self.current - 1).copied()
        }
    }

    pub fn advance(&mut self) -> Option<char> {
        self.current += 1;
        self.buffer.get(self.current - 1).copied()
    }

    pub fn backtrack(&mut self) -> Option<char> {
        if self.current == 0 {
            None
        } else {
            self.current -= 1;
            self.buffer.get(self.current).copied()
        }
    }

    pub fn match_next(&mut self, expected: char) -> bool {
        let ch = self.peek();
        if ch.is_none() {
            return false;
        }
        if self.is_at_end() || ch.unwrap() != expected {
            false
        } else {
            self.advance();
            true
        }
    }

    pub fn match_next_if<P>(&mut self, predicate: P) -> bool
    where
        P: Fn(char) -> bool,
    {
        let ch = self.peek();
        if ch.is_none() {
            return false;
        }
        if self.is_at_end() || !predicate(ch.unwrap()) {
            false
        } else {
            self.advance();
            true
        }
    }

    pub fn set_start(&mut self, new_start: usize) {
        self.start = new_start;
    }

    pub fn current_to_start(&mut self) {
        self.start = self.current;
    }

    pub fn consume(
        &mut self,
        expected: char,
        message: String,
        eid: u16,
        source_name: String,
    ) -> error::Result<()> {
        if self.match_next(expected) {
            self.advance();
            Ok(())
        } else {
            let diagnostic = diagnostic::Diagnostic::new(
                Span::new(self.start, self.current - self.start),
                self.buffer,
                source_name,
                message,
                eid,
            );
            Err(error::ErrorKind::Diagnostic(diagnostic).into())
        }
    }

    pub fn consume_if<P>(
        &mut self,
        predicate: P,
        message: String,
        eid: u16,
        source_name: String,
    ) -> error::Result<()>
    where
        P: Fn(char) -> bool,
    {
        if self.match_next_if(predicate) {
            self.advance();
            Ok(())
        } else {
            let diagnostic = diagnostic::Diagnostic::new(
                Span::new(self.start, self.current - self.start),
                self.buffer,
                source_name,
                message,
                eid,
            );
            Err(error::ErrorKind::Diagnostic(diagnostic).into())
        }
    }

    pub fn gorge(&mut self, expected: char) {
        loop {
            if !self.match_next(expected) {
                break;
            }
        }
    }

    pub fn gorge_while<P>(&mut self, predicate: P)
    where
        P: Fn(char) -> bool,
    {
        loop {
            if !self.match_next_if(&predicate) {
                break;
            }
        }
    }

    pub fn slice(&self, lower: usize, upper: usize) -> &'buf [char] {
        &self.buffer[lower..upper]
    }
}
