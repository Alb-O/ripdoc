use cargo::util::context::GlobalContext;

use crate::error::{Result, convert_cargo_error};

/// Create a cargo configuration with minimal output suited for library usage.
pub fn create_quiet_cargo_config(offline: bool) -> Result<GlobalContext> {
	let mut config = GlobalContext::default().map_err(|err| convert_cargo_error(&err))?;
	config
		.configure(
			0,     // verbose
			true,  // quiet
			None,  // color
			false, // frozen
			false, // locked
			offline,
			&None, // target_dir
			&[],   // unstable_flags
			&[],   // cli_config
		)
		.map_err(|err| convert_cargo_error(&err))?;
	Ok(config)
}

/// Check if rustup is available on the system
pub fn is_rustup_available() -> bool {
	use std::process::{Command, Stdio};
	Command::new("rustup")
		.arg("--version")
		.stderr(Stdio::null())
		.stdout(Stdio::null())
		.status()
		.map(|status| status.success())
		.unwrap_or(false)
}
