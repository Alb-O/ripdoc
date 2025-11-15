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
