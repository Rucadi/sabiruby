//! RGSS1 (Ruby 1.8) projects and the stdlib/IO behaviour older Essentials
//! games rely on. Each test builds a throwaway project: a `Game.ini` and an
//! empty script archive are all `run_source` needs.

use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

/// A project in a fresh temporary folder. `mkxp_ruby` ships a modern Ruby
/// runtime DLL next to the game, as mkxp-z builds do.
fn project(mkxp_ruby: bool) -> (PathBuf, RgssRuntime) {
    static NEXT: AtomicUsize = AtomicUsize::new(0);
    let root = std::env::temp_dir().join(format!(
        "sabiruby-legacy-{}-{}",
        std::process::id(),
        NEXT.fetch_add(1, Ordering::Relaxed)
    ));
    let _ = fs::remove_dir_all(&root);
    fs::create_dir_all(root.join("Data")).unwrap();
    fs::write(
        root.join("Game.ini"),
        "[Game]\r\nLibrary=RGSS104E.dll\r\nScripts=Data\\Scripts.rxdata\r\nTitle=Legacy\r\n",
    )
    .unwrap();
    fs::write(root.join("Data/Scripts.rxdata"), b"").unwrap();
    if mkxp_ruby {
        fs::write(root.join("x64-msvcrt-ruby310.dll"), b"").unwrap();
    }
    let project = Project::open(&root).unwrap();
    assert_eq!(project.ruby18, !mkxp_ruby);
    (root, RgssRuntime::open(project).unwrap())
}

fn run(runtime: &mut RgssRuntime, source: &str) -> String {
    if let Err(error) = runtime.run_source("legacy.rb", source.as_bytes()) {
        panic!(
            "{error}\n{}",
            String::from_utf8_lossy(&runtime.take_output())
        );
    }
    String::from_utf8_lossy(&runtime.take_output()).into_owned()
}

#[test]
fn ruby18_projects_answer_as_ruby_18() {
    let (root, mut runtime) = project(false);
    let out = run(
        &mut runtime,
        r#"
p "A"[0], "abc"[-1], "abc"[3], "abc"[0, 2], "abc"[/b/], "abc"[1..2]
"hello world"[/(w\w+)/]; p $1
p nil.id == nil.__id__, Object.new.id.is_a?(Integer)
def make; proc { return 1 }; end
p make.call
callback = proc { |sender, e| next :early if e == 1; return :returned; :never }
p callback.call(nil, 1), callback.call(nil, 2), callback.call(nil)
loose = proc() { :any }
p loose.call(1, 2)
Thread.critical = true; p Thread.critical; Thread.critical = false
"#,
    );
    assert_eq!(
        out,
        "65\n99\nnil\n\"ab\"\n\"b\"\n\"bc\"\n\"world\"\ntrue\ntrue\n1\n:early\n:returned\n:returned\n:any\ntrue\n"
    );
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn mkxp_projects_keep_modern_ruby() {
    let (root, mut runtime) = project(true);
    let out = run(
        &mut runtime,
        r#"
p "A"[0]
def make; proc { return 1 }; end
p (begin; make.call; rescue LocalJumpError; :local_jump; end)
"#,
    );
    assert_eq!(out, "\"A\"\n:local_jump\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn ruby18_syntax_is_repaired_only_where_it_was_rejected() {
    let (root, mut runtime) = project(false);
    let out = run(
        &mut runtime,
        r#"
def tone(kind)
  case kind
  when 0:
    "zero"
  when 1, 2: "small"
  else "other"
  end
end
p [tone(0), tone(2), tone(5), { a: 1 }[:a], (true ? :yes : :no)]
tries = 0
loop do
  tries += 1
  retry if tries < 3
  break
end
p tries
"#,
    );
    assert_eq!(out, "[\"zero\", \"small\", \"other\", 1, :yes]\n3\n");
    // a block `retry` outside `loop` has no faithful rewrite and stays an error
    let error = runtime
        .run_source("each-retry.rb", b"[1].each do |v|\n  retry\nend\n")
        .unwrap_err();
    assert!(error.to_string().contains("retry"), "{error}");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn legacy_stdlib_and_io_helpers() {
    let (root, mut runtime) = project(false);
    fs::write(root.join("Data/lines.txt"), b"one\ntwo\nthree").unwrap();
    let out = run(
        &mut runtime,
        r#"
p File.extname("Graphics/Pictures/a.png"), File.extname(".hidden"), File.extname("dir.d/file")
p (begin; File.open("Data/missing.dat", "rb"); rescue Errno::ENOENT; :enoent; end)
File.open("Data/lines.txt", "rb") do |f|
  p f.getc, f.gets, f.read(2)
  f.rewind
  bytes = []; f.each_byte { |b| bytes << b; break if bytes.size == 3 }
  p bytes, f.pos
  f.rewind
  p f.readlines
end
packed = Zlib::Deflate.deflate("x" * 100)
p packed.size < 100, Zlib::Inflate.inflate(packed) == "x" * 100, Zlib::Inflate.inflate("plain")
save_data({ :a => [1, 2.5] }, "Data/saved.rxdata")
p load_data("Data/saved.rxdata")
p ENV["TEMP"], ENV["SystemRoot"]
api = Win32API.new("user32", "SetWindowPos", %(l, l, i, i, i, i, i), "i")
p api.call(0, 0, 0, 0, 0, 0, 0)
p Win32API.new("kernel32", "GetCurrentThreadId", "%w()", "l").call
o = Object.new
ObjectSpace.define_finalizer(o, proc {})
p ObjectSpace._id2ref(o.__id__).equal?(o)
p (begin; ObjectSpace._id2ref(Object.new.__id__); rescue RangeError; :dead; end)
"#,
    );
    assert_eq!(
        out,
        concat!(
            "\".png\"\n\"\"\n\"\"\n:enoent\n",
            "111\n\"ne\\n\"\n\"tw\"\n[111, 110, 101]\n3\n[\"one\\n\", \"two\\n\", \"three\"]\n",
            "true\ntrue\n\"plain\"\n{a: [1, 2.5]}\n\".\"\nnil\n1\n0\ntrue\n:dead\n"
        )
    );
    fs::remove_dir_all(root).unwrap();
}

/// MRI 1.8's `w_float` (`%.16g`, then a NUL and the mantissa bits the digits
/// lose, `save_mantissa`) as bytes, to check our reader against its writer.
fn ruby18_float(value: f64, digits: &str) -> Vec<u8> {
    let (fraction, _) = frexp(value.abs());
    let mut rest = ldexp(fraction, 53 - 16);
    rest -= rest.trunc();
    let mut out = digits.as_bytes().to_vec();
    if rest > 0.0 {
        out.push(0);
        while rest > 0.0 {
            let scaled = ldexp(rest, 32);
            let whole = scaled.trunc();
            rest = scaled - whole;
            out.extend_from_slice(&(whole as u32).to_be_bytes());
        }
        while out.last() == Some(&0) {
            out.pop();
        }
    }
    out
}

#[test]
fn ruby18_marshal_floats_keep_their_mantissa_bits() {
    for (value, digits) in [
        (0.1, "0.1"),
        (1.0 / 3.0, "0.3333333333333333"),
        (-2.0 / 3.0, "-0.6666666666666666"),
        (2.5, "2.5"),
    ] {
        let bytes = ruby18_float(value, digits);
        let end = bytes.iter().position(|b| *b == 0).unwrap_or(bytes.len());
        let parsed: f64 = std::str::from_utf8(&bytes[..end]).unwrap().parse().unwrap();
        assert_eq!(
            marshal_load_mantissa(parsed, &bytes[end..]).to_bits(),
            value.to_bits(),
            "{digits}"
        );
    }
}

#[test]
fn case_insensitive_lookups_see_files_written_through_the_vfs() {
    let (root, _runtime) = project(false);
    fs::create_dir_all(root.join("Graphics/Characters")).unwrap();
    fs::write(root.join("Graphics/Characters/Hero.png"), b"1").unwrap();
    let vfs = OsVfs::new(&root);
    assert_eq!(vfs.read("graphics/characters/hero.PNG").unwrap(), b"1");
    assert!(vfs.metadata("Graphics/Characters/Rival.png").is_err());
    vfs.write("Graphics/Characters/Rival.png", b"2", false)
        .unwrap();
    assert_eq!(vfs.read("graphics/characters/RIVAL.png").unwrap(), b"2");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn a_regexp_literal_is_one_object_per_site() {
    let (root, mut runtime) = project(true);
    let out = run(
        &mut runtime,
        r#"
def lit; /a(b)c/i; end
def other; /a(b)c/i; end
def interpolated(n); /x#{n}/; end
p lit.equal?(lit), lit.equal?(other), lit == other
p interpolated(1).equal?(interpolated(1)), interpolated(2).source
p Regexp.new("a").equal?(Regexp.new("a"))
p "xABc" =~ lit, $1
"#,
    );
    assert_eq!(out, "true\nfalse\ntrue\nfalse\n\"x2\"\nfalse\n1\n\"B\"\n");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn blits_composite_over_the_destination() {
    let (root, mut runtime) = project(true);
    let out = run(
        &mut runtime,
        r#"
def rgba(c); [c.red, c.green, c.blue, c.alpha].map(&:to_i); end
ground = Bitmap.new(3, 1)
ground.fill_rect(0, 0, 3, 1, Color.new(0, 0, 255, 255))
upper = Bitmap.new(3, 1)
upper.set_pixel(0, 0, Color.new(255, 0, 0, 255))
upper.set_pixel(2, 0, Color.new(255, 0, 0, 128))
ground.blt(0, 0, upper, upper.rect)
p rgba(ground.get_pixel(0, 0)), rgba(ground.get_pixel(1, 0)), rgba(ground.get_pixel(2, 0))
empty = Bitmap.new(1, 1)
empty.blt(0, 0, upper, Rect.new(2, 0, 1, 1), 128)
p rgba(empty.get_pixel(0, 0))
wide = Bitmap.new(2, 2)
wide.fill_rect(0, 0, 2, 2, Color.new(0, 0, 255, 255))
wide.stretch_blt(wide.rect, upper, Rect.new(1, 0, 1, 1))
p rgba(wide.get_pixel(1, 1))
"#,
    );
    // opaque source replaces, transparent keeps the ground, half alpha mixes;
    // onto nothing the alpha is the source's times the opacity
    assert_eq!(
        out,
        "[255, 0, 0, 255]\n[0, 0, 255, 255]\n[128, 0, 127, 255]\n[255, 0, 0, 64]\n[0, 0, 255, 255]\n"
    );
    fs::remove_dir_all(root).unwrap();
}
