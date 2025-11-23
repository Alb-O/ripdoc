use std::fmt;

use rust_format::Error as FormatError;

/// Errors emitted during renderer execution.
#[derive(Debug)]
pub enum RipdocError {
	/// The requested filter path was not found in the crate.
	FilterNotMatched(String),
	/// Formatting failure while pretty-printing the rendered output.
	Formatter(FormatError),
}

impl fmt::Display for RipdocError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::FilterNotMatched(filter) => {
				write!(f, "filter path '{filter}' did not match any items")
			}
			Self::Formatter(err) => write!(f, "{err}"),
		}
	}
}

impl std::error::Error for RipdocError {}

impl From<FormatError> for RipdocError {
	fn from(err: FormatError) -> Self {
		Self::Formatter(err)
	}
}

/// Result type returned by renderer helpers.
pub type Result<T> = std::result::Result<T, RipdocError>;
