use calypso_base::init_trie;
use calypso_base::{
    span::{Span, Spanned},
    streams::{Stream, StringStream},
};
use calypso_diagnostic::{diagnostic::LabelStyle, error::Result as CalResult, gen_error, FileMgr};

use radix_trie::Trie;

use std::sync::Arc;

pub mod types;
pub use types::*;

mod helpers;
use helpers::*;

use std::ops::Deref;
use std::ops::DerefMut;

pub type Token<'lex> = Spanned<(TokenType, Lexeme<'lex>)>;
pub type Lexeme<'lex> = &'lex str;

#[derive(Debug, Clone)]
pub struct Lexer<'lex> {
    stream: StringStream<'lex>,
    source_id: usize,
    files: Arc<FileMgr>,
    start: Span,
}

impl<'lex> Deref for Lexer<'lex> {
    type Target = StringStream<'lex>;

    fn deref(&self) -> &Self::Target {
        &self.stream
    }
}

impl<'lex> DerefMut for Lexer<'lex> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.stream
    }
}

init_trie!(pub KEYWORD_TRIE: Keyword => {
    "is"     => KwIs,
    "isa"    => KwIsa,
    "bool"   => KwBoolTy,
    "sint"   => KwSintTy,
    "uint"   => KwUintTy,
    "float"  => KwFloatTy,
    "string" => KwStringTy,
    "char"   => KwCharTy,
    "tuple"  => KwTupleTy,
    "array"  => KwArrayTy,
    "false"  => KwFalse,
    "true"   => KwTrue,
    "if"     => KwIf,
    "else"   => KwElse,
    "for"    => KwFor,
    "in"     => KwIn,
    "loop"   => KwLoop,
    "while"  => KwWhile,
    "match"  => KwMatch,
    "ret"    => KwRet,
    "break"  => KwBreak,
    "fn"     => KwFn,
    "native" => KwNative,
    "mod"    => KwMod,
    "use"    => KwUse,
    "import" => KwImport,
    "pub"    => KwPub,
    "let"    => KwLet,
    "mut"    => KwMut,
    "undef"  => KwUndef,
    "null"   => KwNull,
    "del"    => KwDel,
    "as"     => KwAs
});

impl<'lex> Lexer<'lex> {
    pub fn new(source_id: usize, source: &'lex str, files: Arc<FileMgr>) -> Self {
        Self {
            stream: StringStream::new(source),
            source_id,
            files,
            start: Span::default(),
        }
    }

    pub fn scan(&mut self) -> CalResult<Token<'lex>> {
        self.skip_whitespace()?;
        self.current_to_start();

        if self.is_at_end() {
            return Ok(self.new_token(TokenType::Eof));
        }

        // We've already checked if we're at the end (which is when it gives none), so
        // unwrapping should be safe here.
        let span = self.next().unwrap();
        let ch = span.value_owned();

        // Is valid character for identifier's first character
        if is_ident_start(&span) {
            return self.handle_identifier();
        } /*else if ch == '\'' {
              return self.handle_char_literal();
          }*/

        use TokenType::*;

        let token_type = match ch {
            '<' if self.next_if_eq(&'>').is_some() => GreaterLess,
            '<' if self.next_if_eq(&'<').is_some() => {
                if self.next_if_eq(&'=').is_some() {
                    ShlAssign
                } else {
                    Shl
                }
            }
            '<' if self.next_if_eq(&'=').is_some() => LessEqual,
            '<' => Less,

            '>' if self.next_if_eq(&'>').is_some() => {
                if self.next_if_eq(&'=').is_some() {
                    ShrAssign
                } else {
                    Shr
                }
            }
            '>' if self.next_if_eq(&'=').is_some() => GreaterEqual,
            '>' => Greater,

            '=' if self.next_if_eq(&'=').is_some() => BoolEqual,
            '=' => Equal,

            '!' if self.next_if_eq(&'=').is_some() => NotEqual,
            '!' => Bang,

            '|' if self.next_if_eq(&'|').is_some() => BoolOr,
            '|' if self.next_if_eq(&'=').is_some() => PipeAssign,
            '|' => Pipe,

            '&' if self.next_if_eq(&'&').is_some() => BoolAnd,
            '&' if self.next_if_eq(&'=').is_some() => AndAssign,
            '&' => And,

            '+' if self.next_if_eq(&'=').is_some() => PlusAssign,
            '+' => Plus,

            '-' if self.next_if_eq(&'=').is_some() => MinusAssign,
            '-' => Minus,

            '*' if self.next_if_eq(&'*').is_some() => {
                if self.next_if_eq(&'=').is_some() {
                    ExpAssign
                } else {
                    Exp
                }
            }
            '*' if self.next_if_eq(&'=').is_some() => StarAssign,
            '*' => Star,

            '/' if self.next_if_eq(&'=').is_some() => SlashAssign,
            '/' => Slash,

            '%' if self.next_if_eq(&'=').is_some() => RemAssign,
            '%' => Rem,

            '^' if self.next_if_eq(&'=').is_some() => CaretAssign,
            '^' => Caret,

            '~' => Tilde,

            '(' => LeftParen,
            ')' => RightParen,

            '{' => LeftBrace,
            '}' => RightBrace,

            '[' => LeftBracket,
            ']' => RightBracket,

            ',' => Comma,
            ';' => Semi,

            '.' if self.next_if_eq(&'.').is_some() => {
                if self.next_if_eq(&'=').is_some() {
                    RangeInc
                } else {
                    Range
                }
            }
            '.' => Dot,

            // `'_' => Under` is already taken care of by idents
            '#' if self.next_if_eq(&'!').is_some() => HashBang,
            '#' => Hash,

            '$' => {
                if self.handle_escape_character()? {
                    HashBang
                } else {
                    Hash
                }
            }

            // Unexpected character
            ch => gen_error!(self => {
                E0003;
                labels: [
                    LabelStyle::Primary =>
                        (self.source_id, self.new_span());
                        format!("did not expect `{}` here", ch)
                ]
            } as TokenType)?,
        };

        Ok(self.new_token(token_type))
    }

    /*
    pub fn scan(&mut self) -> CalResult<Token<'lex>> {
        self.skip_whitespace()?;
        self.current_to_start();

        if self.is_at_end() {
            return Ok(self.new_token(TokenType::Eof));
        }

        // We've already checked if we're at the end (which is when it gives None), so
        // unwrapping should be safe here.
        let ch = self.advance().unwrap();



        // TODO: literals
        /*if ch == '0' {
            let peek = self.peek();
            if peek.is_some() {
                self.advance();
            }
            let radix = match peek {
                Some('x') => Radix::Hexadecimal,
                Some('o') => Radix::Octal,
                Some('b') => Radix::Binary,
                Some('E') | Some('e') => Radix::Decimal,
                None => Radix::Decimal,
                _ => {
                    let diagnostic = Diagnostic::new(
                        Span::new(self.start(), self.current() - self.start()),
                        self.buffer(),
                        self.source_name.clone(),
                        format!("invalid string base `{}`", peek.unwrap()),
                        4, // Invalid string base.
                    );
                    return Err(diagnostic.into());
                }
            };
            ch = self.advance();
        }*/

        use TokenType::*;

        let token_type = match ch {
            '<' if self.match_next('<') => {
                if self.match_next('=') {
                    ShlAssign
                } else {
                    Shl
                }
            }
            '<' if self.match_next('=') => LessEqual,
            '<' => Less,

            '>' if self.match_next('>') => {
                if self.match_next('=') {
                    ShrAssign
                } else {
                    Shr
                }
            }
            '>' if self.match_next('=') => GreaterEqual,
            '>' => Greater,

            '=' if self.match_next('=') => BoolEqual,
            '=' => Equal,

            '!' if self.match_next('=') => NotEqual,
            '!' => Bang,

            '|' if self.match_next('|') => BoolOr,
            '|' if self.match_next('=') => PipeAssign,
            '|' => Pipe,

            '&' if self.match_next('&') => BoolAnd,
            '&' if self.match_next('=') => AndAssign,
            '&' => And,

            '+' if self.match_next('=') => PlusAssign,
            '+' => Plus,

            '-' if self.match_next('=') => MinusAssign,
            '-' => Minus,

            '*' if self.match_next('*') => {
                if self.match_next('=') {
                    ExpAssign
                } else {
                    Exp
                }
            }
            '*' if self.match_next('=') => StarAssign,
            '*' => Star,

            '/' if self.match_next('=') => SlashAssign,
            '/' => Slash,

            '%' if self.match_next('=') => RemAssign,
            '%' => Rem,

            '^' if self.match_next('=') => CaretAssign,
            '^' => Caret,

            '~' => Tilde,

            '(' => LeftParen,
            ')' => RightParen,

            '{' => LeftBrace,
            '}' => RightBrace,

            '[' => LeftBracket,
            ']' => RightBracket,

            ',' => Comma,
            ';' => Semi,

            '.' if self.match_next('.') => {
                if self.match_next('=') {
                    RangeClosed
                } else {
                    Range
                }
            }
            '.' => Dot,

            // `'_' => Under` is already taken care of by idents
            '#' if self.match_next('!') => HashBang,
            '#' => Hash,

            // Unexpected character
            ch => {
                let diagnostic = DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                    .diag(code!(E0003))
                    .label(
                        LabelStyle::Primary,
                        format!("did not expect `{}` here", ch),
                        self.new_span(),
                        self.source_id,
                    )
                    .build();
                return Err(diagnostic.into());
            }
        };

        Ok(self.new_token(token_type))
    }
    */
}

impl<'lex> Lexer<'lex> {
    fn skip_whitespace(&mut self) -> CalResult<()> {
        self.handle_dangling_comment_ends()?;
        while !self.is_at_end()
            && (self.handle_comment()
                || self.handle_multiline_comment()?
                || self.next_if(is_whitespace).is_some())
        {
            self.handle_dangling_comment_ends()?;
        }
        Ok(())
    }

    fn handle_comment(&mut self) -> bool {
        // xx -> 11 -> 1
        // x/ -> 10 -> 1
        // /x -> 01 -> 1
        // // -> 00 -> 0
        if self.peek_eq(&'/') != Some(true) || self.peek2_eq(&'/') != Some(true) {
            return false;
        }
        // A comment goes until the end of the line,
        // so gorge all the characters until we get to the newline
        // (or the end, when it automatically stops gorging).
        self.gorge_while(|spanned, _| spanned != &'\n');
        true
    }

    fn handle_multiline_comment(&mut self) -> CalResult<bool> {
        // xx -> 11 -> 1
        // x* -> 10 -> 1
        // /x -> 01 -> 1
        // /* -> 00 -> 0
        if self.peek_eq(&'/') != Some(true) || self.peek2_eq(&'*') != Some(true) {
            return Ok(false);
        }
        self.current_to_start();
        self.next();
        self.next();
        let mut stack = vec![self.new_span()];

        loop {
            let span = self.peek();
            let ch = span.map(|sp| sp.value_owned());
            if span.is_none() {
                if stack.len() == 1 {
                    gen_error!(self => {
                        E0002;
                        labels: [
                            LabelStyle::Primary =>
                                (self.source_id, stack.pop().unwrap());
                                "this multi-line comment's beginning has no corresponding end"
                        ]
                    } as ())?
                }
                return Ok(false);
            }

            if ch == Some('/') && self.peek2_eq(&'*') == Some(true) {
                self.current_to_start();
                self.next();
                self.next();
                stack.push(self.new_span());
            } else if ch == Some('*') && self.peek2_eq(&'/') == Some(true) {
                self.current_to_start();
                self.next();
                self.next();
                // I don't think this is needed -- so there's an assert
                // so that if this is an edge case, it's detected more easily.
                assert!(!stack.is_empty());
                // if stack.is_empty() {
                //     gen_error!(self => {
                //         E0001;
                //         labels: [
                //             LabelStyle::Primary =>
                //                 (self.source_id, self.new_span());
                //                 "this multi-line comment's end has no corresponding beginning"
                //         ]
                //     } as ())?
                // }
                stack.pop();
            } else {
                self.next();
            }

            if stack.is_empty() && !self.is_at_end() {
                break;
            }

            if self.is_at_end() && !stack.is_empty() {
                gen_error!(self => {
                    E0002;
                    labels: [
                        LabelStyle::Primary =>
                            (self.source_id, stack.pop().unwrap());
                            "this multi-line comment's beginning has no corresponding end"
                    ]
                } as ())?
            }
        }

        Ok(true)
    }

    fn handle_dangling_comment_ends(&mut self) -> CalResult<()> {
        if self.peek_eq(&'*') == Some(true) && self.peek2_eq(&'/') == Some(true) {
            self.current_to_start();
            self.next();
            self.next();
            gen_error!(self => {
                E0001;
                labels: [
                    LabelStyle::Primary =>
                        (self.source_id, self.new_span());
                        "this multi-line comment's end has no corresponding beginning"
                ]
            } as ())?
        }
        Ok(())
    }

    fn handle_identifier(&mut self) -> CalResult<Token<'lex>> {
        let mut token_type = TokenType::Ident;

        // `_` is not an ident on its own, but all other [A-Za-z]{1} idents are.
        if self.prev().unwrap() == &'_' && self.peek_cond(is_ident_continue) != Some(true) {
            return Ok(self.new_token(TokenType::Under));
        }

        // Gorge while the character is a valid identifier character.
        self.gorge_while(|sp, _| is_ident_continue(sp));

        let keyword = KEYWORD_TRIE.get(&self.slice(self.new_span()).to_string());

        if let Some(&keyword) = keyword {
            token_type = TokenType::Keyword(keyword);
        }

        Ok(self.new_token(token_type))
    }

    fn handle_escape_character(&mut self) -> CalResult<bool> {
        let saved_start = self.current();
        self.current_to_start();
        if self.next_if_eq(&'\\').is_some() {
            match self.peek().map(|v| v.value_owned()) {
                Some('n') | Some('r') | Some('t') | Some('\\') | Some('0') | Some('\'')
                | Some('"') => {
                    self.next();
                }
                Some('x') => self.handle_hex_escape()?,
                Some('u') => (),
                Some(ch) => {
                    if is_whitespace_ch(ch) {
                        gen_error!(self => {
                            E0008;
                            labels: [
                                LabelStyle::Primary =>
                                    (self.source_id, self.new_span());
                                    "expected an escape sequence here"
                            ]
                        } as ())?
                    }
                    self.next();
                    gen_error!(self => {
                        E0006, ch = ch;
                        labels: [
                            LabelStyle::Primary =>
                                (self.source_id, self.new_span());
                                "this escape sequence is unknown"
                        ]
                    } as ())?
                }
                None => gen_error!(self => {
                        E0007;
                        labels: [
                            LabelStyle::Primary =>
                                (self.source_id, self.new_span());
                                "expected an escape sequence here"
                        ]
                    } as ())?,
            }
            self.start = saved_start;
            return Ok(true);
        }

        // We don't care *what* sequence was found, just if there was one.
        Ok(false)
    }

    fn handle_hex_escape(&mut self) -> CalResult<()> {
        // Handle the `x` in `\x41`
        self.next();
        self.current_to_start();
        for i in 1..=2 {
            let sp = self.peek();
            if sp.is_none() || is_whitespace(sp.unwrap()) {
                if i == 1 {
                    gen_error!(self => {
                        E0004;
                        labels: [
                            LabelStyle::Primary =>
                                (self.source_id, self.new_span());
                                "expected two hexadecimal digits here"
                        ]
                    } as ())?
                } else if i == 2 {
                    gen_error!(self => {
                        E0009;
                        labels: [
                            LabelStyle::Primary =>
                                (self.source_id, self.new_span());
                                "found only one hexadecimal digit here"
                        ],
                        notes: [
                            format!(
                                "perhaps you meant to use `\\x0{}`?",
                                self.prev().unwrap().value_owned()
                            )
                        ]
                    } as ())?
                } else {
                    return Ok(());
                }
            }
            let sp = *sp.unwrap();
            let ch = sp.value_owned();

            if ch.is_ascii_hexdigit() {
                self.next();
            } else {
                self.set_start(sp.span());
                gen_error!(self => {
                    E0005, ch = ch;
                    labels: [
                        LabelStyle::Primary =>
                            (self.source_id, self.new_span());
                            "found an invalid digit here"
                    ]
                } as ())?
            }
        }
        Ok(())
    }
}

impl<'lex> Lexer<'lex> {
    /// Set the `start` span to the span of the next character or the empty span of the EOF.
    fn current_to_start(&mut self) {
        self.start = self.current();
    }

    fn set_start(&mut self, start: Span) {
        self.start = start;
    }

    /// Get the span of the next character or the empty span of the EOF.
    fn current(&self) -> Span {
        self.peek()
            .map(|sp| sp.span())
            .unwrap_or_else(|| Span::new_shrunk(self.stream[..].len()))
    }

    fn new_span(&self) -> Span {
        self.start.until(self.current())
    }

    fn new_token(&self, r#type: TokenType) -> Token<'lex> {
        let span = self.new_span();
        Token::new(span, (r#type, self.slice(span)))
    }
}

/*

impl<'lex> Lexer<'lex> {


    fn handle_escape_character(&mut self) -> CalResult<bool> {

                Some('u') => {
                    self.advance();
                    self.current_to_start();
                    match self.peek() {
                        Some(ch) if is_whitespace(ch) => {
                            let diagnostic =
                                DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                    .diag(code!(E0012))
                                    .label(
                                        LabelStyle::Primary,
                                        "this should be an opening curly bracket",
                                        self.new_span(),
                                        self.source_id,
                                    )
                                    .build();
                            return Err(diagnostic.into());
                        }
                        None => {
                            let diagnostic =
                                DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                    .diag(code!(E0011))
                                    .label(
                                        LabelStyle::Primary,
                                        "this should be an opening curly bracket",
                                        self.new_span(),
                                        self.source_id,
                                    )
                                    .build();
                            return Err(diagnostic.into());
                        }
                        _ => (),
                    }
                    if !self.match_next('{') {
                        self.advance();
                        let diagnostic =
                            DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                .diag(code!(E0010, ch = self.last().unwrap()))
                                .label(
                                    LabelStyle::Primary,
                                    "this should be an opening curly bracket",
                                    self.new_span(),
                                    self.source_id,
                                )
                                .build();
                        return Err(diagnostic.into());
                    }

                    let mut count = 0;
                    while self.peek() != Some('}') && !self.is_at_end() {
                        self.current_to_start();
                        let ch = self.peek().unwrap();
                        if count == 6 {
                            break;
                        } else if ch.is_whitespace() {
                            let diagnostic =
                                DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                    .diag(code!(E0018))
                                    .label(
                                        LabelStyle::Primary,
                                        "expected a hexadecimal digit here",
                                        self.new_span(),
                                        self.source_id,
                                    )
                                    .build();
                            return Err(diagnostic.into());
                        } else if !ch.is_ascii_hexdigit() {
                            let diagnostic =
                                DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                    .diag(code!(E0014, ch = ch))
                                    .label(
                                        LabelStyle::Primary,
                                        "found an invalid digit here. perhaps you meant to have a `}` here?",
                                        self.new_span(),
                                        self.source_id,
                                    )
                                    .build();
                            return Err(diagnostic.into());
                        }
                        self.advance();
                        count += 1;
                    }

                    if count == 0 {
                        let diagnostic =
                            DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                .diag(code!(E0019))
                                .label(
                                    LabelStyle::Primary,
                                    "expected at least one hex digit here",
                                    self.new_span(),
                                    self.source_id,
                                )
                                .note("if you wanted a null byte, you can use `\\u{0}` or `\\0`")
                                .build();
                        return Err(diagnostic.into());
                    }
                    self.current_to_start();

                    if self.is_at_end() {
                        let diagnostic =
                            DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                .diag(code!(E0015))
                                .label(
                                    LabelStyle::Primary,
                                    "expected a closing curly bracket here",
                                    self.new_span(),
                                    self.source_id,
                                )
                                .build();
                        return Err(diagnostic.into());
                    }

                    let ch = self.peek().unwrap();
                    if is_whitespace(ch) {
                        self.current_to_start();
                        let diagnostic =
                            DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                .diag(code!(E0017))
                                .label(
                                    LabelStyle::Primary,
                                    "expected a closing curly bracket here",
                                    self.new_span(),
                                    self.source_id,
                                )
                                .build();
                        return Err(diagnostic.into());
                    } else if !self.match_next('}') {
                        let diagnostic =
                            DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                .diag(code!(E0016, ch = ch))
                                .label(
                                    LabelStyle::Primary,
                                    "expected a closing curly bracket here",
                                    self.new_span(),
                                    self.source_id,
                                )
                                .build();
                        return Err(diagnostic.into());
                    }
                }
                Some(ch) => {
                    if is_whitespace(ch) {
                        let diagnostic =
                            DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                                .diag(code!(E0008))
                                .label(
                                    LabelStyle::Primary,
                                    "expected an escape sequence here",
                                    self.new_span(),
                                    self.source_id,
                                )
                                .build();
                        return Err(diagnostic.into());
                    }
                    self.advance();
                    let diagnostic =
                        DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                            .diag(code!(E0006, ch = ch))
                            .label(
                                LabelStyle::Primary,
                                "this escape sequence is unknown",
                                self.new_span(),
                                self.source_id,
                            )
                            .build();
                    return Err(diagnostic.into());
                }
                None => {
                    let diagnostic =
                        DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                            .diag(code!(E0007))
                            .label(
                                LabelStyle::Primary,
                                "expected an escape sequence here",
                                self.new_span(),
                                self.source_id,
                            )
                            .build();
                    return Err(diagnostic.into());
                }
            };
            self.set_start(start);
            return Ok(true);
        }

        // We don't care *what* sequence was found, just if there was one.
        Ok(false)
    }

    fn handle_char_literal(&mut self) -> CalResult<Token<'lex>> {
        let mut chs_found = 0;
        while self.peek() != Some('\'') && !self.is_at_end() {
            if self.handle_escape_character()? {
                chs_found += 1;
            } else if is_valid_for_char_literal(self.peek().unwrap()) {
                self.advance();
                chs_found += 1;
            } else {
                let diagnostic = DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                    .diag(code!(E0020))
                    .label(
                        LabelStyle::Primary,
                        "the character after this one is invalid here; it must be escaped",
                        self.new_span(),
                        self.source_id,
                    )
                    .build();
                return Err(diagnostic.into());
            }
        }

        if chs_found > 1 {
            let start = self.start();
            self.set_start(start + 2);
            self.advance();
            let diagnostic = DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                .diag(code!(E0021))
                .label(
                    LabelStyle::Primary,
                    "expected just a `'` here",
                    self.new_span(),
                    self.source_id,
                )
                .build();
            return Err(diagnostic.into());
        } else if chs_found == 0 {
            let diagnostic = DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                .diag(code!(E0022))
                .label(
                    LabelStyle::Primary,
                    "expected at least one character here",
                    self.new_span(),
                    self.source_id,
                )
                .build();
            return Err(diagnostic.into());
        }

        if !self.match_next('\'') {
            self.current_to_start();
            self.advance();
            let diagnostic = DiagnosticBuilder::new(Severity::Error, Arc::clone(&self.files))
                .diag(code!(E0023))
                .label(
                    LabelStyle::Primary,
                    "expected a single quote here",
                    self.new_span(),
                    self.source_id,
                )
                .build();
            return Err(diagnostic.into());
        }

        Ok(self.new_token(TokenType::CharLiteral))
    }
}
*/

/*
    fn number(&mut self) -> Result<Token<'lex>, ()> {
        let radix = if self.last() == '0' {
            if self.peek().is_ascii_digit() {
                self.advance();
                Radix::Decimal
            } else if self.peek() == '\0' {
                Radix::Decimal
            } else {
                let ch = self.peek();
                self.advance();
                match ch {
                    'b' => Radix::Binary,
                    'x' => Radix::Hexadecimal,
                    'o' => Radix::Octal,
                    'e' | '.' => {
                        self.backup();
                        Radix::Decimal
                    }
                    _ => {
                        println!("Invalid number base.");
                        return Err(());
                    }
                }
            }
        } else {
            Radix::Decimal
        };

        while !self.is_at_end() {
            let ch = self.peek();
            if ch == '\n' || ch == '.' || ch == 'e' || ch == 'E' {
                break;
            }
            if is_valid_digit_for_radix(ch, radix) && is_valid_for_any_radix(ch) {
                self.advance();
            } else if !is_valid_for_any_radix(ch) {
                break;
            } else {
                println!("Invalid digit for number.");
                return Err(());
            }
        }

        Ok(
            // Is a float literal
            if self.peek() == '.' {
                if radix != Radix::Decimal {
                    println!("Cannot have a float with a non-10 base.");
                    return Err(());
                }
                // Consume the `.`.
                self.advance();

                if !self.peek().is_ascii_digit() {
                    println!("Expected decimal component of float");
                    return Err(());
                }

                while !self.is_at_end() {
                    let ch = self.peek();
                    if ch == '\n' || ch == 'E' || ch == 'e' {
                        break;
                    }
                    if ch.is_ascii_digit() {
                        self.advance();
                    } else {
                        println!("Invalid digit for number.");
                        return Err(());
                    }
                }

                // Has exponent
                if self.peek() == 'E' || self.peek() == 'e' {
                    // Consume the `E` or `e`.
                    self.advance();

                    if !self.peek().is_ascii_digit() {
                        println!("Expected exponent");
                        return Err(());
                    }

                    while !self.is_at_end() {
                        let ch = self.peek();
                        if ch == '\n' {
                            break;
                        }
                        if ch.is_ascii_digit() {
                            self.advance();
                        } else {
                            println!("Invalid digit for number.");
                            return Err(());
                        }
                    }
                }

                self.new_token(TokenType::FloatLiteral)
            } else if self.peek() == 'e' || self.peek() == 'E' {
                // Has exponent
                // Consume the `E` or `e`.
                self.advance();

                if !self.peek().is_ascii_digit() {
                    println!("Expected exponent");
                    return Err(());
                }

                while !self.is_at_end() {
                    let ch = self.peek();
                    if ch == '\n' {
                        break;
                    }
                    if ch.is_ascii_digit() {
                        self.advance();
                    } else {
                        println!("Invalid digit for number.");
                        return Err(());
                    }
                }

                self.new_token(TokenType::FloatLiteral)
            } else {
                self.new_token(TokenType::IntLiteral(radix))
            },
        )
    }

    fn string(&mut self) -> Result<Token<'lex>, ()> {
        while self.peek() != '"' && !self.is_at_end() {
            if self.peek() == '\n' {
                println!("Found a newline inside a string.");
                return Err(());
            }
            if !self.escape_character()? {
                self.advance();
            };
        }

        if self.is_at_end() {
            println!("Unterminated string.");
            return Err(());
        }

        // Closing quote
        self.advance();
        Ok(self.new_token(TokenType::StringLiteral))
    }
*/
