//! A forgiving Humdrum `**kern` parser.
//!
//! [`parse`] turns the text of a `**kern` file into a [`KernScore`]: one
//! [`Voice`] per spine (spine splits and merges are followed), each a list
//! of timed [`KernNote`]s. See the crate README for what is and is not
//! covered.

#![forbid(unsafe_code)]

mod parse;
mod score;
mod token;

pub use parse::{ParseError, parse};
pub use score::{KernNote, KernScore, Tie, Voice};
pub use token::{Parsed, parse_token};
