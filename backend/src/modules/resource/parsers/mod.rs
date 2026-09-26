//! Bounded parsers: bounded bytes in, normalized content out.
//!
//! Every parser here is a pure function of the bytes it is given. None of them
//! reads or writes a Task, touches the database, or decides policy — the
//! upload policy runs first in `limits`, and the preview projection in
//! `preview` decides what the parsed content means.
//!
//! `archive` is not a format: it is the shared bounded OOXML container reader
//! that `docx` and `xlsx` both build on.

pub(crate) mod archive;
pub(crate) mod docx;
pub(crate) mod image;
pub(crate) mod pdf;
pub(crate) mod text;
pub(crate) mod xlsx;
