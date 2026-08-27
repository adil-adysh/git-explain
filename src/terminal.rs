//! Small line-oriented primitives shared by the accessible interactive editors.
//!
//! These helpers deliberately do not manage terminal state: callers retain
//! their numbered menus and command-specific cancellation wording.

use anyhow::Result;
use std::io::{BufRead, Write};

pub fn read_line<R: BufRead>(input: &mut R) -> Result<Option<String>> {
    let mut value = String::new();
    if input.read_line(&mut value)? == 0 {
        Ok(None)
    } else {
        Ok(Some(value.trim().to_owned()))
    }
}

pub fn confirmation<R: BufRead, W: Write>(
    input: &mut R,
    output: &mut W,
    prompt: &str,
) -> Result<bool> {
    writeln!(output, "\n{prompt}")?;
    Ok(matches!(
        read_line(input)?.as_deref(),
        Some("y") | Some("Y") | Some("yes") | Some("YES")
    ))
}
