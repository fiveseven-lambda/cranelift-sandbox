use super::parse_term;
use super::CharsPeekable;
use crate::pre_ast::{BracketKind, StringLiteralComponent};

#[derive(Debug)]
pub enum Token {
    Identifier(String),
    Integer(i64),
    StringLiteral(Vec<StringLiteralComponent>),
    Plus,
    Hyphen,
    Asterisk,
    Slash,
    Percent,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    Equal,
    DoubleEqual,
    Exclamation,
    ExclamationEqual,
    Comma,
    Semicolon,
    Colon,
    OpeningBracket(BracketKind),
    ClosingBracket(BracketKind),
    OpeningBrace,
    ClosingBrace,
}

pub fn next(chars: &mut CharsPeekable) -> Option<Token> {
    chars.consume_while(|ch| ch.is_ascii_whitespace());
    let start = chars.offset();
    let first_ch = chars.next()?;
    match first_ch {
        'a'..='z' | 'A'..='Z' | '_' => {
            chars.consume_while(|ch| matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_'));
            let end = chars.offset();
            let name = unsafe { chars.get_substring_unchecked(start, end) };
            Some(Token::Identifier(name.to_owned()))
        }
        '0'..='9' => {
            let mut value: i64 = unsafe { first_ch.to_digit(10).unwrap_unchecked() }.into();
            while let Some(ch) = chars.next_if(|ch| ch.is_ascii_digit()) {
                value = value
                    .checked_mul(10)
                    .unwrap()
                    .checked_add(unsafe { ch.to_digit(10).unwrap_unchecked() } as i64)
                    .unwrap();
            }
            Some(Token::Integer(value))
        }
        '"' => {
            let mut components = Vec::<StringLiteralComponent>::new();
            let mut string = String::new();
            loop {
                match chars.next().unwrap() {
                    '"' => {
                        if !string.is_empty() {
                            components.push(StringLiteralComponent::String(string));
                        }
                        break Some(Token::StringLiteral(components));
                    }
                    '{' => {
                        if !string.is_empty() {
                            components
                                .push(StringLiteralComponent::String(std::mem::take(&mut string)));
                        }
                        let mut peeked = next(chars);
                        let expr = parse_term(chars, &mut peeked);
                        assert!(matches!(peeked, Some(Token::ClosingBrace)));
                        components.push(StringLiteralComponent::Expr(expr.unwrap()));
                    }
                    '\\' => {
                        string.push(chars.next().unwrap());
                    }
                    ch => {
                        string.push(ch);
                    }
                }
            }
        }
        '+' => Some(Token::Plus),
        '-' => Some(Token::Hyphen),
        '*' => Some(Token::Asterisk),
        '%' => Some(Token::Percent),
        '/' => {
            if chars.consume_if_eq('*') {
                let mut comment_depth = 1;
                while comment_depth > 0 {
                    match chars.next().unwrap() {
                        '*' if chars.consume_if_eq('/') => comment_depth -= 1,
                        '/' if chars.consume_if_eq('*') => comment_depth += 1,
                        _ => {}
                    }
                }
                next(chars)
            } else if chars.consume_if_eq('/') {
                chars.consume_while(|ch| ch != '\n');
                next(chars)
            } else {
                Some(Token::Slash)
            }
        }
        '>' => {
            if chars.consume_if_eq('=') {
                Some(Token::GreaterEqual)
            } else {
                Some(Token::Greater)
            }
        }
        '<' => {
            if chars.consume_if_eq('=') {
                Some(Token::LessEqual)
            } else {
                Some(Token::Less)
            }
        }
        '=' => {
            if chars.consume_if_eq('=') {
                Some(Token::DoubleEqual)
            } else {
                Some(Token::Equal)
            }
        }
        '!' => {
            if chars.consume_if_eq('=') {
                Some(Token::ExclamationEqual)
            } else {
                Some(Token::Exclamation)
            }
        }
        ',' => Some(Token::Comma),
        ';' => Some(Token::Semicolon),
        ':' => Some(Token::Colon),
        '(' => Some(Token::OpeningBracket(BracketKind::Round)),
        ')' => Some(Token::ClosingBracket(BracketKind::Round)),
        '[' => Some(Token::OpeningBracket(BracketKind::Square)),
        ']' => Some(Token::ClosingBracket(BracketKind::Square)),
        '{' => Some(Token::OpeningBrace),
        '}' => Some(Token::ClosingBrace),
        _ => todo!(),
    }
}
