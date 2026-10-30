//! Temporary local tool: runs every archive script but the last (`Main`), then
//! the Ruby file given, and prints what it printed.
//! usage: rgss_probe PROJECT SNIPPET.rb
use sabiruby_rgss_core::{Project, RgssRuntime};

fn main() {
    let mut args = std::env::args().skip(1);
    let root = args.next().expect("usage: rgss_probe PROJECT SNIPPET.rb");
    let snippet = std::fs::read(args.next().expect("SNIPPET.rb")).unwrap();
    let mut runtime = RgssRuntime::open(Project::open(root).unwrap()).unwrap();
    runtime.run_scripts(Some(0)).unwrap();
    runtime
        .run_source("count.rb", b"print $RGSS_SCRIPTS.size")
        .unwrap();
    let total: usize = String::from_utf8(runtime.take_output())
        .unwrap()
        .parse()
        .unwrap();
    let ran = runtime.run_scripts(Some(total - 1));
    print!("{}", String::from_utf8_lossy(&runtime.take_output()));
    if let Err(error) = ran {
        println!("scripts: {error}");
    }
    let result = runtime.run_source("probe.rb", &snippet);
    print!("{}", String::from_utf8_lossy(&runtime.take_output()));
    if let Err(error) = result {
        println!("probe: {error}");
    }
}
