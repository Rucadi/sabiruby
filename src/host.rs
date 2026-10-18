//! What the VM asks its host for: compiling a string (`eval`) and reading a file
//! (`require`, later).
//!
//! The VM is `no_std` and knows nothing about the compiler; a program that wants `eval`
//! installs a [`Host`] with [`Vm::set_host`](crate::Vm::set_host). The `sabiruby-compiler`
//! crate provides one (its feature `host`), so the dependency points compiler → VM and the
//! VM crate stays pure Rust. Without a host, `eval` raises NotImplementedError.

use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

/// What an `eval` string needs beyond its text.
pub struct EvalOptions<'a> {
    /// Name for the diagnostics and the debug info (`(eval)` unless the caller says otherwise).
    pub filename: &'a str,
    /// Line the string starts at (`eval(src, nil, file, line)`).
    pub line: u32,
    /// The local variable names the string can see. `scopes[0]` is the caller, each next one
    /// is further out; within a scope the names are in the order of the irep's local variable
    /// table, with an empty name for a hole (an unnamed parameter), so that a position is a
    /// register index.
    pub scopes: &'a [Vec<Vec<u8>>],
    /// Keep the line numbers (`mrbc -g`).
    pub debug_info: bool,
}

impl Default for EvalOptions<'_> {
    fn default() -> Self {
        EvalOptions { filename: "(eval)", line: 1, scopes: &[], debug_info: true }
    }
}

/// Why an `eval` string did not compile, as the SyntaxError says it: `file <name> line
/// <line>: <message>`. What [`Host::compile_eval`] answers with.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvalError {
    /// The line the reference's eval names: that of the first error the compiler reported — a
    /// line within the string for a syntax error (the `line` of [`EvalOptions`] is not added),
    /// 0 for an error of the code generator (mruby 4.1.0-rc2, mruby-eval and the compiler's
    /// `mruby_compat.c`).
    pub line: u32,
    /// The compiler's message (`syntax error, …`, `generator error, …`).
    pub message: String,
}

/// The services a host offers the VM.
///
/// `Send + Sync` so that a [`Vm`](crate::Vm) stays both: an engine that keeps VMs in its own
/// world (rubevy puts one in a Bevy resource) needs that of everything the VM holds, and nothing
/// a host does here is thread-local by nature.
pub trait Host: Send + Sync {
    /// Ruby source to a RITE binary. `Err` is the compiler's message, which becomes the
    /// text of the SyntaxError.
    fn compile(&mut self, src: &[u8], opts: &EvalOptions) -> Result<Vec<u8>, String>;
    /// [`compile`](Host::compile) with the line of the error, which is what `eval` calls. The
    /// default knows no line and names the one the string starts at (`opts.line`); a host that
    /// has its compiler's diagnostics overrides it (`sabiruby-compiler`'s `Compiler` does), and
    /// the SyntaxError then names the line the reference names.
    fn compile_eval(&mut self, src: &[u8], opts: &EvalOptions) -> Result<Vec<u8>, EvalError> {
        self.compile(src, opts).map_err(|message| EvalError { line: opts.line, message })
    }
    /// Contents of a file, for `require`/`load`; `None` when there is none.
    fn read_file(&mut self, path: &str) -> Option<Vec<u8>> {
        let _ = path;
        None
    }
    /// Whether a file is there (`File.file?`).
    fn file_exists(&mut self, path: &str) -> bool {
        self.read_file(path).is_some()
    }
}

/// The host a [`Vm`](crate::Vm) holds, if any.
pub type HostBox = Option<Box<dyn Host + Send + Sync>>;
