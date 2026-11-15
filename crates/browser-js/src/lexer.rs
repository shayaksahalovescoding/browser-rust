#[derive(Debug, Clone, PartialEq)]
pub enum Token {
    Ident(String),
    Number(f64),
    String(String),
    Let,
    Const,
    Var,
    Function,
    Return,
    If,
    Else,
    While,
    For,
    True,
    False,
    Null,
    Undefined,
    Typeof,
    Break,
    Continue,
    LParen,
    RParen,
    LBrace,
    RBrace,
    LBracket,
    RBracket,
    Semicolon,
    Comma,
    Dot,
    Colon,
    Question,
    Arrow, // =>
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    PlusPlus,
    MinusMinus,
    Eq,
    PlusEq,
    MinusEq,
    EqEq,
    EqEqEq,
    BangEq,
    BangEqEq,
    Lt,
    Le,
    Gt,
    Ge,
    Bang,
    AndAnd,
    OrOr,
    Eof,
}

pub fn tokenize(src: &str) -> Result<Vec<Token>, String> {
    let mut tokens = Vec::new();
    let bytes = src.as_bytes();
    let mut i = 0;

    while i < bytes.len() {
        let b = bytes[i];
        if b.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        // Line comment
        if i + 1 < bytes.len() && b == b'/' && bytes[i + 1] == b'/' {
            while i < bytes.len() && bytes[i] != b'\n' {
                i += 1;
            }
            continue;
        }
        // Block comment
        if i + 1 < bytes.len() && b == b'/' && bytes[i + 1] == b'*' {
            i += 2;
            while i + 1 < bytes.len() && !(bytes[i] == b'*' && bytes[i + 1] == b'/') {
                i += 1;
            }
            i = (i + 2).min(bytes.len());
            continue;
        }

        // String literal ('...' or "..." or `...`)
        if b == b'\'' || b == b'"' || b == b'`' {
            let quote = b;
            i += 1;
            let mut s = String::new();
            while i < bytes.len() && bytes[i] != quote {
                if bytes[i] == b'\\' && i + 1 < bytes.len() {
                    i += 1;
                    match bytes[i] {
                        b'n' => s.push('\n'),
                        b't' => s.push('\t'),
                        b'r' => s.push('\r'),
                        other => s.push(other as char),
                    }
                    i += 1;
                } else {
                    let ch = src[i..].chars().next().unwrap();
                    s.push(ch);
                    i += ch.len_utf8();
                }
            }
            if i >= bytes.len() {
                return Err("unterminated string literal".into());
            }
            i += 1;
            tokens.push(Token::String(s));
            continue;
        }

        // Number
        if b.is_ascii_digit() {
            let start = i;
            let mut has_dot = false;
            while i < bytes.len() {
                let c = bytes[i];
                if c.is_ascii_digit() {
                    i += 1;
                } else if c == b'.' && !has_dot {
                    has_dot = true;
                    i += 1;
                } else {
                    break;
                }
            }
            let num: f64 = src[start..i]
                .parse()
                .map_err(|_| format!("invalid number: {}", &src[start..i]))?;
            tokens.push(Token::Number(num));
            continue;
        }

        // Identifier / keyword
        if b.is_ascii_alphabetic() || b == b'_' || b == b'$' {
            let start = i;
            while i < bytes.len()
                && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_' || bytes[i] == b'$')
            {
                i += 1;
            }
            let word = &src[start..i];
            let tok = match word {
                "let" => Token::Let,
                "const" => Token::Const,
                "var" => Token::Var,
                "function" => Token::Function,
                "return" => Token::Return,
                "if" => Token::If,
                "else" => Token::Else,
                "while" => Token::While,
                "for" => Token::For,
                "true" => Token::True,
                "false" => Token::False,
                "null" => Token::Null,
                "undefined" => Token::Undefined,
                "typeof" => Token::Typeof,
                "break" => Token::Break,
                "continue" => Token::Continue,
                _ => Token::Ident(word.to_string()),
            };
            tokens.push(tok);
            continue;
        }

        // Multi-char operators
        let rem = &src[i..];
        let matched = if rem.starts_with("===") {
            Some((3, Token::EqEqEq))
        } else if rem.starts_with("!==") {
            Some((3, Token::BangEqEq))
        } else if rem.starts_with("==") {
            Some((2, Token::EqEq))
        } else if rem.starts_with("!=") {
            Some((2, Token::BangEq))
        } else if rem.starts_with("<=") {
            Some((2, Token::Le))
        } else if rem.starts_with(">=") {
            Some((2, Token::Ge))
        } else if rem.starts_with("&&") {
            Some((2, Token::AndAnd))
        } else if rem.starts_with("||") {
            Some((2, Token::OrOr))
        } else if rem.starts_with("=>") {
            Some((2, Token::Arrow))
        } else if rem.starts_with("++") {
            Some((2, Token::PlusPlus))
        } else if rem.starts_with("--") {
            Some((2, Token::MinusMinus))
        } else if rem.starts_with("+=") {
            Some((2, Token::PlusEq))
        } else if rem.starts_with("-=") {
            Some((2, Token::MinusEq))
        } else {
            None
        };
        if let Some((len, tok)) = matched {
            tokens.push(tok);
            i += len;
            continue;
        }

        let single = match b {
            b'(' => Token::LParen,
            b')' => Token::RParen,
            b'{' => Token::LBrace,
            b'}' => Token::RBrace,
            b'[' => Token::LBracket,
            b']' => Token::RBracket,
            b';' => Token::Semicolon,
            b',' => Token::Comma,
            b'.' => Token::Dot,
            b':' => Token::Colon,
            b'?' => Token::Question,
            b'+' => Token::Plus,
            b'-' => Token::Minus,
            b'*' => Token::Star,
            b'/' => Token::Slash,
            b'%' => Token::Percent,
            b'=' => Token::Eq,
            b'<' => Token::Lt,
            b'>' => Token::Gt,
            b'!' => Token::Bang,
            other => return Err(format!("unexpected character in JS: '{}'", other as char)),
        };
        tokens.push(single);
        i += 1;
    }

    tokens.push(Token::Eof);
    Ok(tokens)
}
