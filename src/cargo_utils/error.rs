use std::fmt;

/// Errors produced while resolving targets or interacting with Cargo/rustdoc.
#[derive(Debug)]
pub enum RipdocError {
	/// Generic error with a message.
	Generate(String),
	/// Failed to parse a manifest file.
	ManifestParse(String),
	/// The requested target path does not point to a Cargo package.
	ManifestNotFound,
	/// A module or crate was not found in the current context.
	ModuleNotFound(String),
	/// The requested target specification was malformed.
	InvalidTarget(String),
}

impl fmt::Display for RipdocError {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		match self {
			Self::Generate(message) => write!(f, "{message}"),
			Self::ManifestParse(message) => write!(f, "failed to parse manifest: {message}"),
			Self::ManifestNotFound => write!(f, "failed to locate Cargo.toml"),
			Self::ModuleNotFound(name) => write!(f, "module or crate not found: {name}"),
			Self::InvalidTarget(message) => write!(f, "{message}"),
		}
	}
}

impl std::error::Error for RipdocError {}

impl From<std::io::Error> for RipdocError {
	fn from(err: std::io::Error) -> Self {
		Self::Generate(err.to_string())
	}
}

/// Result type returned by ripdoc-cargo helpers.
pub type Result<T> = std::result::Result<T, RipdocError>;
