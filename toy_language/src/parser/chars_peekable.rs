use std::str::CharIndices;

pub struct CharsPeekable<'s> {
    input: &'s str,
    chars: CharIndices<'s>,
    next_char_index: Option<(usize, char)>,
}
impl<'s> CharsPeekable<'s> {
    pub fn new(input: &str) -> CharsPeekable {
        let mut chars = input.char_indices();
        let next_char_index = chars.next();
        CharsPeekable {
            input,
            chars,
            next_char_index,
        }
    }
    pub unsafe fn get_substring_unchecked(&self, from: usize, to: usize) -> &str {
        self.input.get_unchecked(from..to)
    }
    pub fn next(&mut self) -> Option<char> {
        self.next_if(|_| true)
    }
    pub fn next_if(&mut self, pred: impl FnOnce(char) -> bool) -> Option<char> {
        match self.next_char_index {
            Some((_, ch)) if pred(ch) => {
                self.next_char_index = self.chars.next();
                Some(ch)
            }
            _ => None,
        }
    }
    pub fn consume_if(&mut self, pred: impl FnOnce(char) -> bool) -> bool {
        self.next_if(pred).is_some()
    }
    pub fn consume_if_eq(&mut self, expected: char) -> bool {
        self.consume_if(|ch| ch == expected)
    }
    pub fn consume_while(&mut self, mut pred: impl FnMut(char) -> bool) {
        while self.consume_if(&mut pred) {}
    }
    pub fn offset(&mut self) -> usize {
        match self.next_char_index {
            Some((index, _)) => index,
            None => self.input.len(),
        }
    }
}
