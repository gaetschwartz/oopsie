//! A scanner for `format!` strings that reports which arguments the string
//! consumes, accepting and rejecting exactly what rustc's format parser does.

#[cfg(test)]
mod tests;

/// The arguments a format string consumes.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct FormatArgUsage<'a> {
    /// Positional arguments consumed: one past the highest index referenced
    /// by an implicit `{}`, an explicit `{i}`, a `.*` precision or an `i$` count.
    pub positional: usize,
    /// Names referenced as `{name…}` or as a `name$` count, in order of
    /// appearance, possibly repeated.
    pub names: Vec<&'a str>,
}

impl FormatArgUsage<'_> {
    pub fn references(&self, name: &str) -> bool {
        self.names.contains(&name)
    }

    pub const fn is_empty(&self) -> bool {
        self.positional == 0 && self.names.is_empty()
    }
}

/// Why a format string is malformed, with the byte offset where parsing failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FormatStrError {
    /// A `}` that neither closes a placeholder nor is part of a `}}` escape.
    UnmatchedClose { at: usize },
    /// Any other malformation.
    Malformed { at: usize },
}

/// Scan `s` (an already-unescaped string literal value) as a `format!` string.
pub fn format_arg_usage(s: &str) -> Result<FormatArgUsage<'_>, FormatStrError> {
    let mut scanner = Scanner {
        s,
        pos: 0,
        next_implicit: 0,
        usage: FormatArgUsage::default(),
    };
    scanner.run()?;
    Ok(scanner.usage)
}

struct Scanner<'a> {
    s: &'a str,
    pos: usize,
    next_implicit: usize,
    usage: FormatArgUsage<'a>,
}

enum Position<'a> {
    Index(usize),
    Name(&'a str),
}

fn is_id_start(c: char) -> bool {
    c == '_' || unicode_ident::is_xid_start(c)
}

impl<'a> Scanner<'a> {
    fn peek(&self) -> Option<char> {
        self.s[self.pos..].chars().next()
    }

    fn peek2(&self) -> Option<char> {
        let mut chars = self.s[self.pos..].chars();
        chars.next();
        chars.next()
    }

    fn eat(&mut self, c: char) -> bool {
        let hit = self.peek() == Some(c);
        if hit {
            self.pos += c.len_utf8();
        }
        hit
    }

    fn eat_any(&mut self, cs: &[char]) -> bool {
        cs.iter().any(|&c| self.eat(c))
    }

    const fn malformed(&self) -> FormatStrError {
        FormatStrError::Malformed { at: self.pos }
    }

    fn run(&mut self) -> Result<(), FormatStrError> {
        while let Some(c) = self.peek() {
            let at = self.pos;
            self.pos += c.len_utf8();
            match c {
                '{' if self.eat('{') => {}
                '{' => self.argument()?,
                '}' if self.eat('}') => {}
                '}' => return Err(FormatStrError::UnmatchedClose { at }),
                _ => {}
            }
        }
        Ok(())
    }

    fn use_index(&mut self, i: usize) {
        self.usage.positional = self.usage.positional.max(i + 1);
    }

    const fn next_implicit(&mut self) -> usize {
        let i = self.next_implicit;
        self.next_implicit += 1;
        i
    }

    fn argument(&mut self) -> Result<(), FormatStrError> {
        let position = self.position()?;
        self.ws();
        self.spec()?;
        match position {
            None => {
                let i = self.next_implicit();
                self.use_index(i);
            }
            Some(Position::Index(i)) => self.use_index(i),
            Some(Position::Name(name)) => self.usage.names.push(name),
        }
        self.ws();
        if self.eat('}') {
            Ok(())
        } else {
            Err(self.malformed())
        }
    }

    fn position(&mut self) -> Result<Option<Position<'a>>, FormatStrError> {
        if let Some(i) = self.integer()? {
            return Ok(Some(Position::Index(i)));
        }
        let word = self.word()?;
        Ok((!word.is_empty()).then_some(Position::Name(word)))
    }

    fn ws(&mut self) {
        while let Some(c) = self.peek()
            && c.is_whitespace()
        {
            self.pos += c.len_utf8();
        }
    }

    fn spec(&mut self) -> Result<(), FormatStrError> {
        if !self.eat(':') {
            return Ok(());
        }
        if let (Some(fill), Some('<' | '>' | '^')) = (self.peek(), self.peek2()) {
            self.pos += fill.len_utf8();
        }
        self.eat_any(&['<', '>', '^']);
        self.eat_any(&['+', '-']);
        self.eat('#');
        if self.eat('0') && self.eat('$') {
            self.use_index(0);
        } else {
            self.count()?;
        }
        if self.eat('.') {
            if self.eat('*') {
                let i = self.next_implicit();
                self.use_index(i);
            } else {
                self.count()?;
            }
        }
        if self.eat_any(&['x', 'X']) {
            self.eat('?');
        } else if !self.eat('?') {
            self.word()?;
        }
        Ok(())
    }

    fn count(&mut self) -> Result<(), FormatStrError> {
        if let Some(i) = self.integer()? {
            if self.eat('$') {
                self.use_index(i);
            }
            return Ok(());
        }
        let start = self.pos;
        let word = self.word()?;
        if word.is_empty() {
            return Ok(());
        }
        if self.eat('$') {
            self.usage.names.push(word);
        } else {
            self.pos = start;
        }
        Ok(())
    }

    /// A run of ASCII digits, which must fit a `u16` as rustc requires.
    fn integer(&mut self) -> Result<Option<usize>, FormatStrError> {
        let start = self.pos;
        let digits = self.s[start..]
            .bytes()
            .take_while(u8::is_ascii_digit)
            .count();
        if digits == 0 {
            return Ok(None);
        }
        self.pos += digits;
        self.s[start..self.pos]
            .parse::<u16>()
            .ok()
            .map(|i| Some(usize::from(i)))
            .ok_or(FormatStrError::Malformed { at: start })
    }

    /// An identifier or keyword; empty if none starts here. A lone `_` is
    /// malformed wherever rustc reads a word, even where it then backtracks.
    fn word(&mut self) -> Result<&'a str, FormatStrError> {
        let start = self.pos;
        if !self.peek().is_some_and(is_id_start) {
            return Ok("");
        }
        let rest = &self.s[start..];
        let len = rest
            .char_indices()
            .skip(1)
            .find(|&(_, c)| !unicode_ident::is_xid_continue(c))
            .map_or(rest.len(), |(i, _)| i);
        self.pos += len;
        let word = &self.s[start..self.pos];
        if word == "_" {
            return Err(FormatStrError::Malformed { at: start });
        }
        Ok(word)
    }
}
