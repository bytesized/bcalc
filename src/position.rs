use std::{
    cmp::{max, min},
    fmt,
    hash::{Hash, Hasher},
};

// These structs are intended to make it easy to point out user errors by literally pointing at
// them.

#[derive(Clone, Debug)]
pub struct Position {
    pub start: usize,
    pub width: usize,
}

impl Position {
    pub fn from_span(pos1: Position, pos2: Position) -> Position {
        let start = min(pos1.start, pos2.start);
        let end = max(pos1.start + pos1.width, pos2.start + pos2.width);
        Position {
            start,
            width: end - start,
        }
    }

    pub fn from_between(pos1: Position, pos2: Position) -> Position {
        let mut points = vec![
            pos1.start,
            pos1.start + pos1.width,
            pos2.start,
            pos2.start + pos2.width,
        ];
        points.sort_unstable();
        let start = points[1];
        let width = points[2] - start;
        Position { start, width }
    }
}

#[derive(Clone, Debug)]
pub struct Positioned<T>
where
    T: Clone + fmt::Debug,
{
    pub value: T,
    pub position: Position,
}

impl<T> fmt::Display for Positioned<T>
where
    T: Clone + fmt::Debug + fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.value, f)
    }
}

impl<T> PartialEq for Positioned<T>
where
    T: Clone + fmt::Debug + PartialEq,
{
    fn eq(&self, other: &Self) -> bool {
        self.value.eq(&other.value)
    }

    fn ne(&self, other: &Self) -> bool {
        self.value.ne(&other.value)
    }
}

impl<T> Eq for Positioned<T> where T: Clone + fmt::Debug + Eq {}

impl<T> Hash for Positioned<T>
where
    T: Clone + fmt::Debug + Hash,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.value.hash(state)
    }
}

impl<T> Positioned<T>
where
    T: Clone + fmt::Debug + fmt::Display,
{
    pub fn new(value: T, position: Position) -> Positioned<T> {
        Positioned { value, position }
    }

    pub fn new_raw(value: T, start: usize, width: usize) -> Positioned<T> {
        Positioned {
            value,
            position: Position { start, width },
        }
    }

    pub fn new_span(value: T, pos1: Position, pos2: Position) -> Positioned<T> {
        Positioned {
            value,
            position: Position::from_span(pos1, pos2),
        }
    }

    pub fn new_between(value: T, pos1: Position, pos2: Position) -> Positioned<T> {
        Positioned {
            value,
            position: Position::from_between(pos1, pos2),
        }
    }

    pub fn map<U, F>(self, f: F) -> Positioned<U>
    where
        F: FnOnce(T) -> U,
        U: Clone + fmt::Debug,
    {
        Positioned {
            value: f(self.value),
            position: self.position,
        }
    }
}

impl Positioned<String> {
    pub fn trim(&mut self) {
        let start_trim_pos = match self.value.chars().position(|c| !c.is_ascii_whitespace()) {
            None => {
                // The whole string is whitespace
                self.value.clear();
                self.position.width = 0;
                return;
            }
            Some(pos) => pos,
        };
        let stop_trim_pos = self.value.len()
            - self
                .value
                .chars()
                .rev()
                .position(|c| !c.is_ascii_whitespace())
                .unwrap();

        self.position.start += start_trim_pos;
        self.value = self.value[start_trim_pos..stop_trim_pos].to_string();
        self.position.width = self.value.len();
    }
}

#[derive(Clone, Debug)]
pub struct MaybePositioned<T>
where
    T: Clone + fmt::Debug,
{
    pub value: T,
    pub maybe_position: Option<Position>,
}

impl<T> fmt::Display for MaybePositioned<T>
where
    T: Clone + fmt::Debug + fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        fmt::Display::fmt(&self.value, f)
    }
}

impl<T> MaybePositioned<T>
where
    T: Clone + fmt::Debug,
{
    pub fn new_positioned(value: T, position: Position) -> MaybePositioned<T> {
        MaybePositioned {
            value,
            maybe_position: Some(position),
        }
    }

    pub fn new_unpositioned(value: T) -> MaybePositioned<T> {
        MaybePositioned {
            value,
            maybe_position: None,
        }
    }

    pub fn new_span(value: T, pos1: Position, pos2: Position) -> MaybePositioned<T> {
        MaybePositioned {
            value,
            maybe_position: Some(Position::from_span(pos1, pos2)),
        }
    }
}

impl<T> From<Positioned<T>> for MaybePositioned<T>
where
    T: Clone + fmt::Debug,
{
    fn from(item: Positioned<T>) -> Self {
        MaybePositioned::new_positioned(item.value, item.position)
    }
}
