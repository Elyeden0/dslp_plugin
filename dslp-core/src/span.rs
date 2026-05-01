use serde::{Deserialize, Serialize};

/// 1-indexed line, 0-indexed byte column within the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub col: u32,
}

impl Position {
    pub fn new(line: u32, col: u32) -> Self {
        Self { line, col }
    }
}

/// Half-open span: includes `start`, excludes `end`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Span {
    pub start: Position,
    pub end: Position,
}

impl Span {
    pub fn new(start: Position, end: Position) -> Self {
        Self { start, end }
    }

    pub fn single_line(line: u32, start_col: u32, end_col: u32) -> Self {
        Self {
            start: Position::new(line, start_col),
            end: Position::new(line, end_col),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_constructs_via_helpers() {
        let s = Span::single_line(10, 0, 4);
        assert_eq!(s.start.line, 10);
        assert_eq!(s.end.col, 4);
    }
}
