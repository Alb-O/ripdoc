use std::collections::BTreeMap;

/// Shared execution context between the host process and the dynamically compiled script.
#[repr(C)]
#[derive(Default)]
pub struct ScriptContext {
	cycle: u64,
	numbers: BTreeMap<String, f64>,
	strings: BTreeMap<String, String>,
	output: Vec<String>,
}

impl ScriptContext {
	pub const fn new() -> Self {
		Self {
			cycle: 0,
			numbers: BTreeMap::new(),
			strings: BTreeMap::new(),
			output: Vec::new(),
		}
	}

	pub fn bump_cycle(&mut self) {
		self.cycle = self.cycle.wrapping_add(1);
	}

	pub fn cycle(&self) -> u64 {
		self.cycle
	}

	pub fn emit_line(&mut self, line: impl Into<String>) {
		self.output.push(line.into());
	}

	pub fn drain_output(&mut self) -> Vec<String> {
		self.output.drain(..).collect()
	}

	pub fn set_number(&mut self, key: impl Into<String>, value: f64) {
		self.numbers.insert(key.into(), value);
	}

	pub fn number(&self, key: &str) -> Option<f64> {
		self.numbers.get(key).copied()
	}

	pub fn set_text(&mut self, key: impl Into<String>, value: impl Into<String>) {
		self.strings.insert(key.into(), value.into());
	}

	pub fn text(&self, key: &str) -> Option<&str> {
		self.strings.get(key).map(|s| s.as_str())
	}
}
