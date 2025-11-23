#![allow(missing_docs)]

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::{Duration, Instant};

use anyhow::{Context, Result, anyhow};
use clap::Parser;
use libloading::Library;
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use tempfile::TempDir;

const RUNTIME_SOURCE: &str = include_str!("runtime.rs");
const SCRIPT_CARGO_TOML: &str = r#"
[package]
name = "hot_script"
version = "0.1.0"
edition = "2024"

[lib]
crate-type = ["cdylib"]

[dependencies]
anyhow = "1"
subsecond = "0.7"
"#;

#[allow(dead_code)]
mod runtime_defs {
	include!(concat!(
		env!("CARGO_MANIFEST_DIR"),
		"/src/bin/hotinterp/runtime.rs"
	));
}

use runtime_defs::ScriptContext;

#[derive(Parser, Debug)]
#[command(
	name = "hotinterp",
	about = "Experimental Rust mini-interpreter that hot-reloads scripts via Subsecond"
)]
struct Cli {
	/// Path to the user script. The file must expose `pub fn hot_main(ctx: &mut ScriptContext) -> anyhow::Result<()>`.
	script: PathBuf,

	/// Run once and exit instead of watching for changes.
	#[arg(long, default_value_t = false)]
	once: bool,

	/// Build the generated helper crate in release mode.
	#[arg(long, default_value_t = false)]
	release: bool,
}

fn main() -> Result<()> {
	let cli = Cli::parse();
	let mut engine = ScriptEngine::new(cli.script, cli.release)?;

	if cli.once {
		engine.run_and_report()
	} else {
		engine.watch_loop()
	}
}

struct ScriptEngine {
	script_path: PathBuf,
	release: bool,
	workspace: ScriptWorkspace,
	ctx: ScriptContext,
}

impl ScriptEngine {
	fn new(script_path: PathBuf, release: bool) -> Result<Self> {
		let absolute = if script_path.is_absolute() {
			script_path
		} else {
			std::env::current_dir()?.join(script_path)
		};
		let script_path = absolute
			.canonicalize()
			.with_context(|| "unable to resolve script path")?;

		if !script_path.is_file() {
			return Err(anyhow!(
				"script path {} is not a file",
				script_path.display()
			));
		}

		let workspace = ScriptWorkspace::new()?;

		Ok(Self {
			script_path,
			release,
			workspace,
			ctx: ScriptContext::new(),
		})
	}

	fn watch_loop(&mut self) -> Result<()> {
		if let Err(err) = self.run_and_report() {
			eprintln!("script run failed: {err:?}");
		}
		println!(
			"watching {} for changes ({} build)...",
			self.script_path.display(),
			if self.release { "release" } else { "dev" }
		);

		let (tx, rx) = mpsc::channel();
		let mut watcher = RecommendedWatcher::new(
			move |event| {
				let _ = tx.send(event);
			},
			Config::default().with_poll_interval(Duration::from_millis(250)),
		)?;
		let parent = self
			.script_path
			.parent()
			.map(Path::to_path_buf)
			.unwrap_or_else(|| PathBuf::from("."));
		watcher.watch(&parent, RecursiveMode::NonRecursive)?;

		let mut debounce = Instant::now();
		loop {
			match rx.recv() {
				Ok(Ok(event)) if Self::targets_script(&event, &self.script_path) => {
					if debounce.elapsed() < Duration::from_millis(150) {
						continue;
					}
					debounce = Instant::now();
					println!("↻ change detected ({:?})", event.kind);
					if let Err(err) = self.run_and_report() {
						eprintln!("script run failed: {err:?}");
					}
				}
				Ok(Err(err)) => eprintln!("watch error: {err:?}"),
				Err(_) => break,
				_ => {}
			}
		}

		Ok(())
	}

	fn targets_script(event: &Event, script_path: &Path) -> bool {
		event
			.paths
			.iter()
			.any(|changed| match changed.canonicalize() {
				Ok(path) => path == script_path,
				Err(_) => false,
			})
	}

	fn run_and_report(&mut self) -> Result<()> {
		self.run_once()
	}

	fn run_once(&mut self) -> Result<()> {
		let user_code = fs::read_to_string(&self.script_path)
			.with_context(|| format!("failed to read script {}", self.script_path.display()))?;
		let generated = wrap_script_source(&user_code);
		self.workspace.write_source(&generated)?;
		let artifact = self.workspace.build(self.release)?;
		let lib = HotLibrary::load(&artifact)?;

		self.ctx.bump_cycle();
		let status = subsecond::call(|| unsafe { lib.call(&mut self.ctx) });

		for line in self.ctx.drain_output() {
			println!("{line}");
		}

		if status != 0 {
			return Err(anyhow!("script returned non-zero status ({status})"));
		}

		Ok(())
	}
}

struct HotLibrary {
	_lib: Library,
	entry: unsafe extern "C" fn(*mut ScriptContext) -> i32,
}

impl HotLibrary {
	fn load(path: &Path) -> Result<Self> {
		let lib = unsafe { Library::new(path) }
			.with_context(|| format!("failed to load {}", path.display()))?;
		let entry = unsafe {
			let symbol =
				lib.get::<unsafe extern "C" fn(*mut ScriptContext) -> i32>(b"hot_entry\0")?;
			*symbol
		};
		Ok(Self { _lib: lib, entry })
	}

	unsafe fn call(&self, ctx: &mut ScriptContext) -> i32 {
		unsafe { (self.entry)(ctx as *mut ScriptContext) }
	}
}

struct ScriptWorkspace {
	root: TempDir,
	src_path: PathBuf,
}

impl ScriptWorkspace {
	fn new() -> Result<Self> {
		let root = tempfile::Builder::new()
			.prefix("hotinterp-script")
			.tempdir()?;
		fs::create_dir_all(root.path().join("src"))?;
		fs::write(root.path().join("Cargo.toml"), SCRIPT_CARGO_TOML)?;

		Ok(Self {
			src_path: root.path().join("src/lib.rs"),
			root,
		})
	}

	fn write_source(&self, source: &str) -> Result<()> {
		fs::write(&self.src_path, source)
			.with_context(|| format!("failed to write {}", self.src_path.display()))
	}

	fn build(&self, release: bool) -> Result<PathBuf> {
		let mut cmd = Command::new("cargo");
		cmd.arg("build");
		if release {
			cmd.arg("--release");
		}
		cmd.current_dir(self.root.path());
		cmd.stdout(Stdio::piped());
		cmd.stderr(Stdio::piped());

		let output = cmd
			.output()
			.context("failed to run cargo build for script")?;
		if !output.status.success() {
			let stdout = String::from_utf8_lossy(&output.stdout);
			let stderr = String::from_utf8_lossy(&output.stderr);
			return Err(anyhow!(
				"script build failed (status {:?})\nstdout:\n{}\nstderr:\n{}",
				output.status.code(),
				stdout,
				stderr
			));
		}

		let artifact = self.artifact_path(release);
		if !artifact.exists() {
			return Err(anyhow!(
				"expected hotpatch artifact at {}, but it was missing",
				artifact.display()
			));
		}

		Ok(artifact)
	}

	fn artifact_path(&self, release: bool) -> PathBuf {
		let mut path = self.root.path().join("target");
		path.push(if release { "release" } else { "debug" });
		let file_name = format!(
			"{}hot_script{}",
			std::env::consts::DLL_PREFIX,
			std::env::consts::DLL_SUFFIX
		);
		path.join(file_name)
	}
}

fn wrap_script_source(user_code: &str) -> String {
	format!(
		r#"
#![allow(unused)]
use anyhow::Result;

{runtime_source}

{user_code}

#[no_mangle]
pub extern "C" fn hot_entry(ctx: *mut ScriptContext) -> i32 {{
	let ctx = unsafe {{
		assert!(!ctx.is_null(), "script context pointer cannot be null");
		&mut *ctx
	}};
	let mut hot_main = || -> Result<()> {{
		crate::hot_main(ctx)
	}};
	match subsecond::call(&mut hot_main) {{
		Ok(()) => 0,
		Err(err) => {{
			ctx.emit_line(format!("⚠️ {{err:?}}"));
			-1
		}}
	}}
}}
"#,
		runtime_source = RUNTIME_SOURCE,
		user_code = user_code
	)
}
