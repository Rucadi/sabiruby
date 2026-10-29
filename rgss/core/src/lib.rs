//! The reusable, headless part of an RGSS player.
//!
//! This crate deliberately owns no window, WGPU device, event loop, or audio
//! device. An embedding application supplies those services and drives the VM.
//! The first implementation provides project discovery, a rooted compiler/file
//! host, RGSS service state, and a small compatibility surface. Rendering and
//! device adapters can be added without changing the VM crate.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{Cursor, Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use flate2::read::{DeflateDecoder, ZlibDecoder};
use fontdue::{Font as DueFont, FontSettings};
use image::AnimationDecoder;
use sabiruby::host::{EvalError, EvalOptions};
use sabiruby::value::ObjId;
use sabiruby::{Host, Step, Value, Vm, VmError};
use sabiruby_compiler::{Options, compile, compile_eval};

#[derive(Debug)]
pub enum RgssError {
    Io(std::io::Error),
    Vm(VmError),
    Compile(String),
    Runtime(String),
    InvalidProject(String),
}

impl std::fmt::Display for RgssError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "I/O error: {e}"),
            Self::Vm(e) => write!(f, "VM error: {e}"),
            Self::Compile(e) => write!(f, "compile error: {e}"),
            Self::Runtime(e) => write!(f, "Ruby runtime error: {e}"),
            Self::InvalidProject(e) => write!(f, "invalid RGSS project: {e}"),
        }
    }
}

impl std::error::Error for RgssError {}
impl From<std::io::Error> for RgssError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}
impl From<VmError> for RgssError {
    fn from(e: VmError) -> Self {
        Self::Vm(e)
    }
}

/// Metadata exposed by a virtual filesystem. Paths are project-relative and
/// use `/` separators; implementations may map them to a directory, archive,
/// package, or sandbox.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct VfsMetadata {
    pub len: u64,
    pub is_file: bool,
    pub is_dir: bool,
}

/// One child returned by [`VirtualFileSystem::read_dir`].
///
/// The vector order is significant: the desktop implementation preserves the
/// order produced by the host filesystem, which is the order MRI exposes for
/// `Dir.entries`/`Dir.foreach`. Ruby's `Dir.glob` applies its own lexical sort
/// after collecting matches.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VfsDirEntry {
    pub name: String,
    pub metadata: VfsMetadata,
}

/// Filesystem boundary used by Ruby `File`, `Dir` and `require`.
///
/// Asset naming and extension probing deliberately live in a private RGSS-core
/// helper instead of this trait. A VFS implementation therefore only has to
/// expose storage and Ruby directory semantics; an archive/WASM backend does
/// not need to know what RGSS considers an image or audio asset.
pub trait VirtualFileSystem: Send + Sync {
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>>;
    fn write(&self, path: &str, bytes: &[u8], append: bool) -> std::io::Result<usize>;
    fn metadata(&self, path: &str) -> std::io::Result<VfsMetadata>;

    /// Return direct children in the backend's native enumeration order.
    ///
    /// Backends that only support file reads can keep the default and return
    /// `Unsupported`; the RGSS archive reader can implement this later without
    /// changing the Ruby-facing API.
    fn read_dir(&self, _path: &str) -> std::io::Result<Vec<VfsDirEntry>> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "directory enumeration is not available in this VFS",
        ))
    }

    fn remove(&self, _path: &str) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "removal is not available in this VFS",
        ))
    }

    fn create_dir(&self, _path: &str) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "directory creation is not available in this VFS",
        ))
    }

    fn rename(&self, _from: &str, _to: &str) -> std::io::Result<()> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "rename is not available in this VFS",
        ))
    }
}

/// Default project-rooted filesystem used by the desktop probe.
#[derive(Clone, Debug)]
pub struct OsVfs {
    root: PathBuf,
    case_insensitive: bool,
}

impl OsVfs {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            case_insensitive: true,
        }
    }

    /// Construct the desktop backend with native case-sensitive lookup.
    /// `new` intentionally defaults to the Windows-like behavior expected by
    /// most RPG Maker XP projects.
    pub fn new_case_sensitive(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            case_insensitive: false,
        }
    }

    fn resolve(&self, path: &str) -> std::io::Result<PathBuf> {
        let normalized = path.replace('\\', "/");
        let candidate = rooted(&self.root, &normalized).ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "path escapes project root",
            )
        })?;
        if !self.case_insensitive || candidate.exists() {
            return Ok(candidate);
        }
        let mut current = self.root.clone();
        for component in Path::new(&normalized).components() {
            let Component::Normal(name) = component else {
                continue;
            };
            let name = name.to_string_lossy();
            let exact = current.join(name.as_ref());
            if exact.exists() {
                current = exact;
                continue;
            }
            let mut match_path = None;
            for entry in fs::read_dir(&current)? {
                let entry = entry?;
                if entry
                    .file_name()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&name)
                {
                    match_path = Some(entry.path());
                    break;
                }
            }
            current = match_path.unwrap_or_else(|| current.join(name.as_ref()));
        }
        Ok(current)
    }
}

impl VirtualFileSystem for OsVfs {
    fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
        fs::read(self.resolve(path)?)
    }
    fn write(&self, path: &str, bytes: &[u8], append: bool) -> std::io::Result<usize> {
        let mut options = fs::OpenOptions::new();
        options.write(true).create(true);
        if append {
            options.append(true);
        } else {
            options.truncate(true);
        }
        let mut file = options.open(self.resolve(path)?)?;
        file.write_all(bytes)?;
        Ok(bytes.len())
    }
    fn metadata(&self, path: &str) -> std::io::Result<VfsMetadata> {
        let metadata = fs::metadata(self.resolve(path)?)?;
        Ok(VfsMetadata {
            len: metadata.len(),
            is_file: metadata.is_file(),
            is_dir: metadata.is_dir(),
        })
    }

    fn read_dir(&self, path: &str) -> std::io::Result<Vec<VfsDirEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.resolve(path)?)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            entries.push(VfsDirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                metadata: VfsMetadata {
                    len: metadata.len(),
                    is_file: metadata.is_file(),
                    is_dir: metadata.is_dir(),
                },
            });
        }
        Ok(entries)
    }

    fn remove(&self, path: &str) -> std::io::Result<()> {
        let path = self.resolve(path)?;
        let metadata = fs::metadata(&path)?;
        if metadata.is_dir() {
            fs::remove_dir(path)
        } else {
            fs::remove_file(path)
        }
    }

    fn create_dir(&self, path: &str) -> std::io::Result<()> {
        fs::create_dir(self.resolve(path)?)
    }

    fn rename(&self, from: &str, to: &str) -> std::io::Result<()> {
        fs::rename(self.resolve(from)?, self.resolve(to)?)
    }
}

/// Project metadata needed before the VM starts executing Ruby.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Project {
    root: PathBuf,
    pub title: String,
    pub scripts_path: PathBuf,
    /// Project-relative archive path used by the runtime VFS.
    pub scripts_asset: String,
    pub screen_width: u32,
    pub screen_height: u32,
}

impl Project {
    pub fn open(root: impl Into<PathBuf>) -> Result<Self, RgssError> {
        let root = root.into();
        let ini_path = root.join("Game.ini");
        let ini = fs::read_to_string(&ini_path)?;
        let title = ini
            .lines()
            .find_map(|line| line.strip_prefix("Title="))
            .unwrap_or("RGSS Game")
            .trim()
            .to_owned();
        let scripts = ini
            .lines()
            .find_map(|line| line.strip_prefix("Scripts="))
            .unwrap_or("Data\\Scripts.rxdata")
            .trim();
        let scripts_path = root.join(scripts.replace('\\', "/"));
        if !scripts_path.is_file() {
            return Err(RgssError::InvalidProject(format!(
                "script archive does not exist: {}",
                scripts_path.display()
            )));
        }
        let mkxp = fs::read_to_string(root.join("mkxp.json")).unwrap_or_default();
        let screen_width = json_number(&mkxp, "defScreenW").unwrap_or(512);
        let screen_height = json_number(&mkxp, "defScreenH").unwrap_or(384);
        Ok(Self {
            root,
            title,
            scripts_path,
            scripts_asset: scripts.replace('\\', "/"),
            screen_width,
            screen_height,
        })
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Enumerate source files for diagnostics and tooling. This is not used by
    /// the runtime entrypoint, which always reads [`Self::scripts_asset`].
    pub fn extracted_scripts(&self) -> Result<Vec<PathBuf>, RgssError> {
        let vfs = OsVfs::new(self.root.clone());
        self.extracted_scripts_with_vfs(&vfs)
            .map(|files| files.into_iter().map(|path| self.root.join(path)).collect())
    }

    /// Discover source scripts through an embedding-selected filesystem view.
    /// This is kept separate from the compiled archive loader so an archive or
    /// WASM backend can expose diagnostics without changing boot semantics.
    pub fn extracted_scripts_with_vfs(
        &self,
        vfs: &dyn VirtualFileSystem,
    ) -> Result<Vec<String>, RgssError> {
        let mut files = Vec::new();
        collect_rb(vfs, "Data/Scripts", &mut files)?;
        files.sort();
        Ok(files)
    }

    /// Discover plugin source files for tooling and embedders. Runtime plugin
    /// registration is performed by the game's Ruby `PluginManager`; this
    /// helper never appends plugin code to the main `Data/Scripts` list.
    pub fn plugin_scripts(&self) -> Result<Vec<PathBuf>, RgssError> {
        let vfs = OsVfs::new(self.root.clone());
        plugin_scripts_with_vfs(&vfs)
            .map(|files| files.into_iter().map(|path| self.root.join(path)).collect())
    }

    pub fn read_asset(&self, relative: &str) -> Result<Vec<u8>, RgssError> {
        let vfs = OsVfs::new(self.root.clone());
        Ok(vfs.read(relative)?)
    }
}

fn vfs_join(parent: &str, child: &str) -> String {
    if parent.is_empty() || parent == "." {
        child.to_owned()
    } else {
        format!("{parent}/{child}")
    }
}

fn collect_rb(
    vfs: &dyn VirtualFileSystem,
    dir: &str,
    out: &mut Vec<String>,
) -> Result<(), RgssError> {
    let entries = match vfs.read_dir(dir) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    for entry in entries {
        let path = vfs_join(dir, &entry.name);
        if entry.metadata.is_dir {
            collect_rb(vfs, &path, out)?;
        } else if Path::new(&entry.name)
            .extension()
            .is_some_and(|extension| extension == "rb")
        {
            out.push(path);
        }
    }
    Ok(())
}

fn plugin_scripts_with_vfs(vfs: &dyn VirtualFileSystem) -> Result<Vec<String>, RgssError> {
    let mut files = Vec::new();
    collect_rb(vfs, "Plugins", &mut files)?;
    files.sort_by_key(|path| {
        let depth = path.split('/').count();
        (depth, path.clone())
    });
    Ok(files)
}

#[derive(Clone, Debug)]
struct PluginArchiveEntry {
    root: String,
    name: String,
    version: String,
    essentials: Vec<String>,
    link: Option<String>,
    dependencies: Vec<PluginDependency>,
    incompatibilities: Vec<String>,
    credits: Vec<String>,
    disabled: Option<String>,
    scripts: Vec<String>,
}

#[derive(Clone, Debug)]
struct PluginDependency {
    kind: Option<String>,
    name: String,
    version: Option<String>,
}

fn parse_plugin_meta(root: &str, bytes: &[u8]) -> PluginArchiveEntry {
    let mut name = None;
    let mut version = None;
    let mut essentials = Vec::new();
    let mut link = None;
    let mut dependencies = Vec::new();
    let mut incompatibilities = Vec::new();
    let mut credits = Vec::new();
    let mut disabled = None;
    for line in String::from_utf8_lossy(bytes).lines() {
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let key = key
            .trim()
            .trim_start_matches('\u{feff}')
            .to_ascii_lowercase();
        let values: Vec<String> = value
            .split(',')
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_owned)
            .collect();
        match key.as_str() {
            "name" => name = values.first().cloned(),
            "version" => version = values.first().cloned(),
            "essentials" => essentials.extend(values),
            "link" | "website" => link = values.first().cloned(),
            "requires" => {
                if let Some(dep_name) = values.first() {
                    dependencies.push(PluginDependency {
                        kind: values.get(2).map(|kind| kind.to_ascii_lowercase()),
                        name: dep_name.clone(),
                        version: values.get(1).cloned(),
                    });
                }
            }
            "exact" | "optional" => {
                if values.len() >= 2 {
                    dependencies.push(PluginDependency {
                        kind: Some(key.clone()),
                        name: values[0].clone(),
                        version: Some(values[1].clone()),
                    });
                }
            }
            "conflicts" => incompatibilities.extend(values),
            "credits" => credits.extend(values),
            "disabled" => disabled = values.first().cloned(),
            _ => {}
        }
    }
    PluginArchiveEntry {
        root: root.to_owned(),
        name: name.unwrap_or_else(|| root.rsplit('/').next().unwrap_or(root).to_owned()),
        version: version.unwrap_or_else(|| "0".to_owned()),
        essentials,
        link,
        dependencies,
        incompatibilities,
        credits,
        disabled,
        scripts: Vec::new(),
    }
}

fn plugin_archive_entries(
    vfs: &dyn VirtualFileSystem,
) -> Result<Vec<PluginArchiveEntry>, RgssError> {
    let roots = match vfs.read_dir("Plugins") {
        Ok(entries) => entries
            .into_iter()
            .filter(|entry| entry.metadata.is_dir)
            .map(|entry| vfs_join("Plugins", &entry.name))
            .collect::<Vec<_>>(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => return Ok(Vec::new()),
        Err(error) => return Err(error.into()),
    };
    // Do not resolve dependencies here.  Essentials' Ruby PluginManager owns
    // that policy (including Requires/Optional/Exact/Conflicts).  The VFS
    // order is only the input order used by its `Dir.get` equivalent.
    let mut entries = Vec::new();
    for root in roots {
        let meta_path = vfs_join(&root, "meta.txt");
        let Ok(meta) = vfs.read(&meta_path) else {
            continue;
        };
        let mut entry = parse_plugin_meta(&root, &meta);
        collect_rb(vfs, &root, &mut entry.scripts)?;
        // These optional CRuby stdlib copies use syntax outside the RGSS
        // compiler's supported profile.  Keep this capability workaround
        // separate from plugin ordering: PluginManager still owns
        // registration and dependency validation.
        entry.scripts.retain(|path| {
            !path.ends_with("/Libraries/set.rb") && !path.ends_with("/Libraries/tsort.rb")
        });
        entry.scripts.sort();
        entries.push(entry);
    }

    Ok(entries)
}

/// Read the compiled Essentials plugin archive when it is present.  The
/// archive is the authoritative order seen by RGSS: plugins may override one
/// another's `setup` methods, so reconstructing order from directory names or
/// dependency metadata can change observable Ruby behaviour.  We still build
/// the source/script payload from the VFS so alternate backends remain usable.
fn plugin_archive_order(vm: &mut Vm, vfs: &dyn VirtualFileSystem) -> Result<Vec<String>, VmError> {
    let bytes = vfs.read("Data/PluginScripts.rxdata").map_err(|error| {
        vm.raise_arg(&format!("cannot read Data/PluginScripts.rxdata: {error}"))
    })?;
    let archive = MarshalDecoder::new(vm, &bytes).decode()?;
    let mut order = Vec::new();
    for plugin in vm.ary_vals(archive).unwrap_or_default() {
        let Some(fields) = vm.ary_vals(plugin) else {
            continue;
        };
        let Some(name) = fields.first().and_then(|value| vm.str_bytes(*value)) else {
            continue;
        };
        order.push(String::from_utf8_lossy(name).into_owned());
    }
    Ok(order)
}

fn order_plugin_entries(
    entries: Vec<PluginArchiveEntry>,
    archive_order: &[String],
) -> Vec<PluginArchiveEntry> {
    let mut by_name = entries
        .into_iter()
        .map(|entry| (entry.name.clone(), entry))
        .collect::<HashMap<_, _>>();
    let mut ordered = Vec::with_capacity(by_name.len());
    for name in archive_order {
        if let Some(entry) = by_name.remove(name) {
            ordered.push(entry);
        }
    }
    // Keep filesystem-only plugins available to embedders and development
    // projects which do not regenerate PluginScripts.rxdata yet.
    let mut remaining = by_name.into_values().collect::<Vec<_>>();
    remaining.sort_by(|left, right| left.name.cmp(&right.name));
    ordered.extend(remaining);
    ordered
}

fn plugin_meta_set(vm: &mut Vm, hash: Value, name: &str, value: Value) -> Result<(), VmError> {
    let key = Value::Sym(vm.intern(name));
    vm.hash_set(hash, key, value)
}

fn plugin_dependency_value(vm: &mut Vm, dependency: &PluginDependency) -> Value {
    let name = vm.str_new(dependency.name.as_bytes());
    match (&dependency.kind, &dependency.version) {
        (None, None) => name,
        (None, Some(version)) => {
            let version = vm.str_new(version.as_bytes());
            vm.ary_new(vec![name, version])
        }
        (Some(kind), Some(version)) => {
            let kind = Value::Sym(vm.intern(kind));
            let version = vm.str_new(version.as_bytes());
            vm.ary_new(vec![kind, name, version])
        }
        (Some(kind), None) => {
            let kind = Value::Sym(vm.intern(kind));
            vm.ary_new(vec![kind, name])
        }
    }
}

fn rgss_plugin_scripts(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let vfs = state_mut(vm).vfs.clone();
    let entries = plugin_archive_entries(vfs.as_ref())
        .map_err(|error| vm.raise_arg(&format!("cannot load plugin scripts: {error}")))?;
    let entries = match plugin_archive_order(vm, vfs.as_ref()) {
        Ok(order) if !order.is_empty() => order_plugin_entries(entries, &order),
        _ => entries,
    };
    let mut plugins = Vec::with_capacity(entries.len());
    for entry in entries {
        let meta = vm.hash_new();
        let name = vm.str_new(entry.name.as_bytes());
        plugin_meta_set(vm, meta, "name", name)?;
        let version = vm.str_new(entry.version.as_bytes());
        plugin_meta_set(vm, meta, "version", version)?;
        if !entry.essentials.is_empty() {
            let values = entry
                .essentials
                .iter()
                .map(|value| vm.str_new(value.as_bytes()))
                .collect();
            let values = vm.ary_new(values);
            plugin_meta_set(vm, meta, "essentials", values)?;
        }
        if let Some(link) = entry.link {
            let link = vm.str_new(link.as_bytes());
            plugin_meta_set(vm, meta, "link", link)?;
        }
        if !entry.dependencies.is_empty() {
            let values = entry
                .dependencies
                .iter()
                .map(|dependency| plugin_dependency_value(vm, dependency))
                .collect();
            let values = vm.ary_new(values);
            plugin_meta_set(vm, meta, "dependencies", values)?;
        }
        if !entry.incompatibilities.is_empty() {
            let values = entry
                .incompatibilities
                .iter()
                .map(|value| vm.str_new(value.as_bytes()))
                .collect();
            let values = vm.ary_new(values);
            plugin_meta_set(vm, meta, "incompatibilities", values)?;
        }
        if !entry.credits.is_empty() {
            let values = entry
                .credits
                .iter()
                .map(|value| vm.str_new(value.as_bytes()))
                .collect();
            let values = vm.ary_new(values);
            plugin_meta_set(vm, meta, "credits", values)?;
        }
        if let Some(disabled) = entry.disabled {
            let disabled = vm.str_new(disabled.as_bytes());
            plugin_meta_set(vm, meta, "disabled", disabled)?;
        }
        let mut scripts = Vec::with_capacity(entry.scripts.len());
        for path in entry.scripts {
            let source = vfs.read(&path).map_err(|error| {
                vm.raise_arg(&format!("cannot read plugin script {path}: {error}"))
            })?;
            let relative = path
                .strip_prefix(&format!("{}/", entry.root))
                .unwrap_or(&path);
            let relative = vm.str_new(relative.as_bytes());
            let source = vm.str_new(&source);
            scripts.push(vm.ary_new(vec![relative, source]));
        }
        let name = vm.str_new(entry.name.as_bytes());
        let scripts = vm.ary_new(scripts);
        plugins.push(vm.ary_new(vec![name, meta, scripts]));
    }
    Ok(vm.ary_new(plugins))
}

fn rooted(root: &Path, name: &str) -> Option<PathBuf> {
    let candidate = Path::new(name);
    if candidate.components().any(|c| {
        matches!(
            c,
            Component::ParentDir | Component::RootDir | Component::Prefix(_)
        )
    }) {
        return None;
    }
    Some(root.join(candidate))
}

fn json_number(src: &str, key: &str) -> Option<u32> {
    let marker = format!("\"{key}\"");
    let after = src.split_once(&marker)?.1;
    let digits: String = after
        .split_once(':')?
        .1
        .chars()
        .skip_while(|c| c.is_ascii_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits.parse().ok()
}

/// mruby's compiler rejects constant-path operator writes (`A::B += value`)
/// even though RPG Maker's Ruby scripts use them for mutable string constants.
/// Rewrite that general syntax into an ordinary path assignment before handing
/// RGSS source to the compiler; this is deliberately independent of any game
/// or plugin name.
fn rewrite_constant_path_operator_writes(source: &[u8]) -> Vec<u8> {
    let text = String::from_utf8_lossy(source);
    let mut rewritten = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let mut output = line.to_owned();
        for operator in ["+=", "-=", "*=", "/=", "%=", "|=", "&="] {
            let Some(operator_at) = output.find(operator) else {
                continue;
            };
            let before = &output[..operator_at];
            let lhs_end = before.trim_end().len();
            let lhs_start = before[..lhs_end]
                .rfind(|character: char| {
                    !(character.is_ascii_alphanumeric() || character == '_' || character == ':')
                })
                .map_or(0, |index| {
                    index + before[index..].chars().next().map_or(1, char::len_utf8)
                });
            let lhs = &before[lhs_start..lhs_end];
            if !lhs.contains("::") || lhs.is_empty() {
                continue;
            }
            let rhs = &output[operator_at + operator.len()..];
            output = format!(
                "{}{} = {} {}{}",
                &before[..lhs_start],
                lhs,
                lhs,
                &operator[..operator.len() - 1],
                rhs
            );
            break;
        }
        rewritten.push_str(&output);
    }
    rewritten.into_bytes()
}

/// File/compiler host rooted at a game directory. It prevents `require` and
/// `load` from escaping the project and keeps the VM independent of `std::fs`.
pub struct ProjectHost {
    vfs: Arc<dyn VirtualFileSystem>,
}

/// Host-side keyboard state shared with the Ruby `Input` module.
///
/// Events are queued by the window adapter and committed when Ruby calls
/// `Input.update`, matching RGSS's one-frame trigger/release semantics.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    pending_pressed: BTreeSet<i32>,
    pending_released: BTreeSet<i32>,
    pressed: BTreeSet<i32>,
    triggered: BTreeSet<i32>,
    released: BTreeSet<i32>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AudioChannel {
    Bgm,
    Bgs,
    Me,
    Se,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AudioCommand {
    pub channel: AudioChannel,
    pub path: String,
    pub volume: u8,
    pub pitch: u16,
}

#[derive(Clone, Debug)]
struct BitmapData {
    width: i64,
    height: i64,
    pixels: Vec<u8>,
    revision: u64,
    frames: Vec<Vec<u8>>,
    frame_delays_ms: Vec<u64>,
    current_frame: usize,
    animation_elapsed_ms: u64,
    manual_frame_rate: Option<f64>,
    playing: bool,
}

impl InputState {
    pub fn press(&mut self, code: i32) {
        self.pending_pressed.insert(code);
    }

    pub fn release(&mut self, code: i32) {
        self.pending_pressed.remove(&code);
        self.pending_released.insert(code);
    }

    pub fn update(&mut self) {
        self.triggered = self
            .pending_pressed
            .difference(&self.pressed)
            .copied()
            .collect();
        self.released = self.pending_released.clone();
        self.pressed = self.pending_pressed.clone();
        self.pending_released.clear();
    }

    pub fn pressed(&self, code: i32) -> bool {
        self.pressed.contains(&code)
    }

    pub fn triggered(&self, code: i32) -> bool {
        self.triggered.contains(&code)
    }

    pub fn released(&self, code: i32) -> bool {
        self.released.contains(&code)
    }
}

impl ProjectHost {
    pub fn new(project: &Project) -> Self {
        Self::with_vfs(project, Arc::new(OsVfs::new(project.root.clone())))
    }
    pub fn with_vfs(_: &Project, vfs: Arc<dyn VirtualFileSystem>) -> Self {
        Self { vfs }
    }
}

impl Host for ProjectHost {
    fn compile(&mut self, src: &[u8], opts: &EvalOptions) -> Result<Vec<u8>, String> {
        let source = rewrite_constant_path_operator_writes(src);
        let options = Options {
            filename: opts.filename.to_owned(),
            debug_info: opts.debug_info,
            ..Options::default()
        };
        compile(&source, &options).map_err(|e| e.to_string())
    }

    fn compile_eval(&mut self, src: &[u8], opts: &EvalOptions) -> Result<Vec<u8>, EvalError> {
        let source = rewrite_constant_path_operator_writes(src);
        let options = Options {
            filename: opts.filename.to_owned(),
            debug_info: opts.debug_info,
            ..Options::default()
        };
        compile_eval(&source, &options, opts.line, opts.scopes).map_err(|e| {
            let d = e.diagnostics.iter().find(|d| d.kind.is_error());
            EvalError {
                line: d.map_or(0, |d| d.line),
                message: d.map_or_else(|| "compile error".to_owned(), |d| d.message.clone()),
            }
        })
    }

    fn read_file(&mut self, path: &str) -> Option<Vec<u8>> {
        self.vfs.read(path).ok()
    }
    fn file_exists(&mut self, path: &str) -> bool {
        self.vfs.metadata(path).is_ok_and(|m| m.is_file)
    }
}

/// State shared by the initial native RGSS bindings. It is intentionally
/// independent from WGPU so a headless host can test Ruby behavior.
pub struct RgssState {
    pub width: u32,
    pub height: u32,
    pub frame_rate: u32,
    pub frame_count: u64,
    pub fullscreen: bool,
    pub scale: f64,
    pub frames: u64,
    pub data_directory: String,
    pub working_directory: String,
    pub vfs: Arc<dyn VirtualFileSystem>,
    pub frame_limit: Option<u64>,
    pub input: InputState,
    pub audio_commands: Vec<AudioCommand>,
    bitmaps: HashMap<ObjId, BitmapData>,
    last_animation_ms: u64,
    started: Instant,
}

impl RgssState {
    pub fn new(project: &Project, vfs: Arc<dyn VirtualFileSystem>) -> Self {
        Self {
            width: project.screen_width,
            height: project.screen_height,
            frame_rate: 60,
            frame_count: 0,
            fullscreen: false,
            scale: 1.0,
            frames: 0,
            data_directory: "Data".to_owned(),
            working_directory: ".".to_owned(),
            vfs,
            frame_limit: None,
            input: InputState::default(),
            audio_commands: Vec::new(),
            bitmaps: HashMap::new(),
            last_animation_ms: 0,
            started: Instant::now(),
        }
    }
    pub fn uptime(&self) -> f64 {
        self.started.elapsed().as_secs_f64()
    }
}

/// Host-neutral description of one visible RGSS sprite.  It contains no VM
/// handles or renderer types, so WGPU, a software compositor, and WASM hosts
/// can all consume the same frame snapshot.
#[derive(Clone, Debug, PartialEq)]
pub struct SpriteSnapshot {
    pub bitmap_id: u32,
    pub bitmap_path: Option<String>,
    pub bitmap_rgba: Option<Arc<[u8]>>,
    pub bitmap_width: u32,
    pub bitmap_height: u32,
    pub bitmap_revision: u64,
    pub src_x: i64,
    pub src_y: i64,
    pub src_width: i64,
    pub src_height: i64,
    pub x: i64,
    pub y: i64,
    pub ox: i64,
    pub oy: i64,
    pub zoom_x: f64,
    pub zoom_y: f64,
    pub opacity: u8,
    pub angle: f64,
    pub mirror: bool,
    pub color: [u8; 4],
    pub tone: [i16; 4],
    pub blend_type: i64,
    pub visible: bool,
    pub z: i64,
    pub tile: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshot {
    pub width: u32,
    pub height: u32,
    pub sprites: Vec<SpriteSnapshot>,
}

fn state_mut(vm: &mut Vm) -> &mut RgssState {
    vm.host_state_mut::<RgssState>()
        .expect("RGSS state is installed")
}

fn value_i64(value: Value, default: i64) -> i64 {
    match value {
        Value::Int(value) => value,
        Value::Float(value) if value.is_finite() => value as i64,
        _ => default,
    }
}

fn value_f64(value: Value, default: f64) -> f64 {
    match value {
        Value::Int(value) => value as f64,
        Value::Float(value) if value.is_finite() => value,
        _ => default,
    }
}

fn color_components(vm: &Vm, value: Value, default: [i64; 4]) -> [i64; 4] {
    let Some(object) = value.obj() else {
        return default;
    };
    ["@red", "@green", "@blue", "@alpha"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| value_i64(vm.ivar_get(object, name), default[index]).clamp(0, 255))
        .collect::<Vec<_>>()
        .try_into()
        .expect("color component count")
}

fn tone_components(vm: &Vm, value: Value) -> [i16; 4] {
    let Some(object) = value.obj() else {
        return [0; 4];
    };
    ["@red", "@green", "@blue", "@gray"]
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            let value = value_i64(vm.ivar_get(object, name), 0);
            if index == 3 {
                value.clamp(0, 255) as i16
            } else {
                value.clamp(-255, 255) as i16
            }
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("tone component count")
}

fn overlay_colors(base: [u8; 4], overlay: [u8; 4]) -> [u8; 4] {
    let factor = f32::from(overlay[3]) / 255.0;
    let mix = |left: u8, right: u8| {
        (f32::from(left) * (1.0 - factor) + f32::from(right) * factor).round() as u8
    };
    [
        mix(base[0], overlay[0]),
        mix(base[1], overlay[1]),
        mix(base[2], overlay[2]),
        (f32::from(base[3]) + f32::from(overlay[3]) * (1.0 - f32::from(base[3]) / 255.0)).round()
            as u8,
    ]
}

fn add_tones(base: [i16; 4], overlay: [i16; 4]) -> [i16; 4] {
    [
        base[0].saturating_add(overlay[0]).clamp(-255, 255),
        base[1].saturating_add(overlay[1]).clamp(-255, 255),
        base[2].saturating_add(overlay[2]).clamp(-255, 255),
        base[3].saturating_add(overlay[3]).clamp(0, 255),
    ]
}

fn int_arg(vm: &mut Vm, args: &[Value], index: usize) -> Result<i64, VmError> {
    vm.expect_int(args.get(index).copied().unwrap_or(Value::Nil), "integer")
}

fn graphics_width(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Int(state_mut(vm).width.into()))
}
fn graphics_height(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Int(state_mut(vm).height.into()))
}
fn graphics_frame_rate(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    if !args.is_empty() {
        let frame_rate = int_arg(vm, args, 0)?.max(1) as u32;
        state_mut(vm).frame_rate = frame_rate;
        return Ok(Value::Nil);
    }
    Ok(Value::Int(state_mut(vm).frame_rate.into()))
}
fn graphics_frame_count(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    if let Some(value) = args.first().copied() {
        let frame_count = vm.expect_int(value, "frame count")?.max(0) as u64;
        state_mut(vm).frame_count = frame_count;
        return Ok(Value::Nil);
    }
    Ok(Value::Int(
        state_mut(vm).frame_count.min(i64::MAX as u64) as i64
    ))
}
fn graphics_update(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let reached_limit = {
        let state = state_mut(vm);
        state
            .frame_limit
            .is_some_and(|limit| state.frame_count >= limit)
    };
    if reached_limit {
        return Err(vm.raise_arg("headless frame limit reached"));
    }
    {
        let s = state_mut(vm);
        s.frame_count = s.frame_count.saturating_add(1);
        s.frames = s.frames.saturating_add(1);
    }
    advance_animated_bitmaps(vm);
    vm.request_step_pause();
    Ok(Value::Nil)
}

fn advance_animated_bitmaps(vm: &mut Vm) {
    let state = state_mut(vm);
    let now_ms = state.started.elapsed().as_millis() as u64;
    // Match the host animation clock used by rsxp: the first tick establishes
    // the baseline and never skips the first decoded frame.
    let delta_ms = if state.last_animation_ms == 0 {
        0
    } else {
        now_ms.saturating_sub(state.last_animation_ms).min(250)
    };
    state.last_animation_ms = now_ms;
    for bitmap in state.bitmaps.values_mut() {
        if !bitmap.playing || bitmap.frames.len() < 2 {
            continue;
        }
        bitmap.animation_elapsed_ms = bitmap.animation_elapsed_ms.saturating_add(delta_ms);
        loop {
            let delay = bitmap.manual_frame_rate.map_or_else(
                || bitmap.frame_delays_ms[bitmap.current_frame].max(1),
                |rate| (1000.0 / rate.max(0.001)).round() as u64,
            );
            if bitmap.animation_elapsed_ms < delay {
                break;
            }
            bitmap.animation_elapsed_ms -= delay;
            bitmap.current_frame = (bitmap.current_frame + 1) % bitmap.frames.len();
            bitmap.pixels = bitmap.frames[bitmap.current_frame].clone();
            bitmap.revision = bitmap.revision.saturating_add(1);
        }
    }
}
fn graphics_resize(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let w = int_arg(vm, args, 0)?;
    let h = int_arg(vm, args, 1)?;
    let s = state_mut(vm);
    s.width = w.max(1) as u32;
    s.height = h.max(1) as u32;
    Ok(Value::Nil)
}

fn input_code(vm: &mut Vm, args: &[Value]) -> Result<i32, VmError> {
    let value = args.first().copied().unwrap_or(Value::Nil);
    match value {
        Value::Int(value) => Ok(value.clamp(i32::MIN as i64, i32::MAX as i64) as i32),
        Value::Sym(symbol) => Ok(match vm.sym_name(symbol).as_str() {
            "C" => 1,
            "B" => 2,
            "A" => 3,
            "X" => 4,
            "Y" => 5,
            "Z" => 6,
            "L" => 7,
            "R" => 8,
            "UP" => 9,
            "DOWN" => 10,
            "LEFT" => 11,
            "RIGHT" => 12,
            "ALT" => 13,
            "CTRL" | "LCTRL" | "RCTRL" => 14,
            "SHIFT" => 15,
            "F8" => 16,
            "F9" => 17,
            "MOUSELEFT" => 18,
            "MOUSERIGHT" => 19,
            "MOUSEMIDDLE" => 20,
            "ESCAPE" | "BACKSPACE" => 21,
            "RETURN" => 23,
            "F3" => 0x103,
            "F5" => 0x105,
            "TAB" => 0x109,
            "DELETE" => 0x10a,
            "HOME" => 0x10b,
            "END" => 0x10c,
            "KP_MINUS" => 0x10d,
            "KP_PLUS" => 0x10e,
            name if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() => {
                0x100 + i32::from(name.as_bytes()[0])
            }
            _ => 0,
        }),
        _ => Err(vm.raise_type("Input key must be an Integer or Symbol")),
    }
}

fn input_update(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    state_mut(vm).input.update();
    Ok(Value::Nil)
}

fn input_trigger(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.triggered(code)))
}

fn input_repeat(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.pressed(code)))
}

fn input_press(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.pressed(code)))
}

fn input_release(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.released(code)))
}

fn input_dir4(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let input = &state_mut(vm).input;
    let direction = if input.pressed(9) {
        8
    } else if input.pressed(10) {
        2
    } else if input.pressed(11) {
        4
    } else if input.pressed(12) {
        6
    } else {
        0
    };
    Ok(Value::Int(direction))
}

fn input_dir8(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let input = &state_mut(vm).input;
    let up = input.pressed(9);
    let down = input.pressed(10);
    let left = input.pressed(11);
    let right = input.pressed(12);
    let direction = match (up, down, left, right) {
        (true, false, true, false) => 7,
        (true, false, false, true) => 9,
        (false, true, true, false) => 1,
        (false, true, false, true) => 3,
        _ if up => 8,
        _ if down => 2,
        _ if left => 4,
        _ if right => 6,
        _ => 0,
    };
    Ok(Value::Int(direction))
}

fn graphics_fullscreen(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let s = state_mut(vm);
    if let Some(v) = args.first() {
        s.fullscreen = v.truthy();
        Ok(Value::Nil)
    } else {
        Ok(Value::bool(s.fullscreen))
    }
}
fn graphics_scale(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let s = state_mut(vm);
    if let Some(Value::Float(v)) = args.first() {
        s.scale = *v;
        Ok(Value::Nil)
    } else if let Some(Value::Int(v)) = args.first() {
        s.scale = *v as f64;
        Ok(Value::Nil)
    } else {
        Ok(Value::Float(s.scale))
    }
}
fn graphics_delta(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Float(1.0 / state_mut(vm).frame_rate as f64))
}

fn graphics_snap_to_bitmap(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    // The core has no renderer-owned backbuffer.  Still return a real Bitmap
    // with the current RGSS dimensions so freeze/transition code can retain
    // and dispose a snapshot safely; a graphical host may replace this with a
    // composited frame when it owns a readback-capable surface.
    let class = vm
        .const_path_get("Bitmap")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Bitmap class is not installed"))?;
    let bitmap = vm.object_new(class);
    let (width, height) = {
        let state = state_mut(vm);
        (state.width, state.height)
    };
    bitmap_initialize(
        vm,
        bitmap,
        &[Value::Int(i64::from(width)), Value::Int(i64::from(height))],
        Value::Nil,
    )?;
    Ok(bitmap)
}

fn audio_path(vm: &mut Vm, args: &[Value]) -> Option<String> {
    let value = args.first().copied()?;
    if let Some(bytes) = vm.str_bytes(value) {
        return Some(String::from_utf8_lossy(bytes).into_owned());
    }
    let object = value.obj()?;
    let name = vm.ivar_get(object, "@name");
    vm.str_bytes(name)
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
}

fn audio_play(vm: &mut Vm, args: &[Value], channel: AudioChannel) -> Result<Value, VmError> {
    let Some(path) = audio_path(vm, args) else {
        return Ok(Value::Nil);
    };
    let volume =
        value_i64(args.get(1).copied().unwrap_or(Value::Int(100)), 100).clamp(0, 100) as u8;
    let pitch =
        value_i64(args.get(2).copied().unwrap_or(Value::Int(100)), 100).clamp(1, 400) as u16;
    state_mut(vm).audio_commands.push(AudioCommand {
        channel,
        path,
        volume,
        pitch,
    });
    Ok(Value::Nil)
}

fn audio_stop(vm: &mut Vm, channel: AudioChannel) -> Result<Value, VmError> {
    // An empty path is the unambiguous stop marker for the adapter.  It keeps
    // the command type compact while allowing the host to own mixer policy.
    state_mut(vm).audio_commands.push(AudioCommand {
        channel,
        path: String::new(),
        volume: 0,
        pitch: 100,
    });
    Ok(Value::Nil)
}

fn bgm_play(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    audio_play(vm, args, AudioChannel::Bgm)
}
fn bgs_play(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    audio_play(vm, args, AudioChannel::Bgs)
}
fn me_play(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    audio_play(vm, args, AudioChannel::Me)
}
fn se_play(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    audio_play(vm, args, AudioChannel::Se)
}
fn bgm_stop(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    audio_stop(vm, AudioChannel::Bgm)
}
fn bgs_stop(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    audio_stop(vm, AudioChannel::Bgs)
}
fn me_stop(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    audio_stop(vm, AudioChannel::Me)
}
fn se_stop(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    audio_stop(vm, AudioChannel::Se)
}
fn system_uptime(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Float(state_mut(vm).uptime()))
}
fn system_data_directory(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let p = state_mut(vm).data_directory.clone();
    Ok(vm.str_new(p.as_bytes()))
}
fn system_user_language(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    // RGSS scripts index this value before selecting message files.  Keep it
    // deterministic for embedders that do not expose an OS locale service.
    Ok(vm.str_new(b"es"))
}
fn system_power_state(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    // A host may not expose battery telemetry (for example a desktop, WASM,
    // or an embedded player).  Return the same neutral shape as rsxp's host
    // contract instead of nil, so RGSS code can safely index the result.
    let state = vm.hash_new();
    let seconds = Value::Sym(vm.intern("seconds"));
    let percent = Value::Sym(vm.intern("percent"));
    let discharging = Value::Sym(vm.intern("discharging"));
    vm.hash_set(state, seconds, Value::Nil)?;
    vm.hash_set(state, percent, Value::Nil)?;
    vm.hash_set(state, discharging, Value::False)?;
    Ok(state)
}
fn noop(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Nil)
}
fn false_value(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::False)
}
fn zero_value(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Int(0))
}
fn numeric_round(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let digits = args
        .first()
        .copied()
        .map(|value| vm.expect_int(value, "digits"))
        .transpose()?
        .unwrap_or(0);
    if digits <= 0 {
        return match receiver {
            Value::Int(value) => Ok(Value::Int(value)),
            Value::Float(value) => Ok(Value::Int(value.round() as i64)),
            _ => Err(vm.raise_type("Numeric required")),
        };
    }
    let factor = 10_f64.powi(digits.min(i32::MAX as i64) as i32);
    match receiver {
        Value::Int(value) => Ok(Value::Float((value as f64 * factor).round() / factor)),
        Value::Float(value) => Ok(Value::Float((value * factor).round() / factor)),
        _ => Err(vm.raise_type("Numeric required")),
    }
}
fn true_value(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::True)
}
fn path_arg(vm: &mut Vm, args: &[Value]) -> Result<String, VmError> {
    let value = args.first().copied().unwrap_or(Value::Nil);
    Ok(String::from_utf8_lossy(&vm.expect_str(value, "path")?).into_owned())
}

fn vfs_path(vm: &mut Vm, path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let absolute = normalized.starts_with('/');
    let mut parts = if absolute {
        Vec::new()
    } else {
        state_mut(vm)
            .working_directory
            .split('/')
            .filter(|part| !part.is_empty() && *part != ".")
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop();
            }
            part => parts.push(part.to_owned()),
        }
    }
    if parts.is_empty() {
        ".".to_owned()
    } else {
        parts.join("/")
    }
}

fn vfs_arg(vm: &mut Vm, args: &[Value]) -> Result<String, VmError> {
    let path = path_arg(vm, args)?;
    Ok(vfs_path(vm, &path))
}

fn vfs_dir_arg(vm: &mut Vm, args: &[Value]) -> Result<String, VmError> {
    let path = dir_path_arg(vm, args)?;
    Ok(vfs_path(vm, &path))
}

/// Reader for Ruby's version 4.8 Marshal stream. RGSS data files are ordinary
/// Marshal values (not a game-specific database format), so keeping this at
/// the VM/VFS boundary makes `load_data` work for maps, Essentials `.dat`
/// files, saves, and embedded archive backends alike.
struct MarshalDecoder<'a> {
    vm: &'a mut Vm,
    bytes: &'a [u8],
    offset: usize,
    symbols: Vec<sabiruby::symbol::Sym>,
    objects: Vec<Value>,
}

impl<'a> MarshalDecoder<'a> {
    fn new(vm: &'a mut Vm, bytes: &'a [u8]) -> Self {
        Self {
            vm,
            bytes,
            offset: 0,
            symbols: Vec::new(),
            objects: Vec::new(),
        }
    }

    fn fail<T>(&mut self, message: impl std::fmt::Display) -> Result<T, VmError> {
        Err(self
            .vm
            .raise_arg(&format!("invalid Marshal data: {message}")))
    }

    fn byte(&mut self) -> Result<u8, VmError> {
        let Some(byte) = self.bytes.get(self.offset).copied() else {
            return self.fail("unexpected end of stream");
        };
        self.offset += 1;
        Ok(byte)
    }

    fn bytes(&mut self, length: usize) -> Result<&'a [u8], VmError> {
        let end = self
            .offset
            .checked_add(length)
            .ok_or_else(|| self.vm.raise_arg("Marshal length overflow"))?;
        let Some(bytes) = self.bytes.get(self.offset..end) else {
            return self.fail("truncated value");
        };
        self.offset = end;
        Ok(bytes)
    }

    /// Marshal's compact integer format. Values 1..4 and 252..255 encode the
    /// byte count of a larger signed value; 5..127 and 128..250 are the small
    /// positive/negative forms used by ordinary lengths and fixnums.
    fn long(&mut self) -> Result<i64, VmError> {
        let tag = self.byte()?;
        match tag {
            0 => Ok(0),
            5..=127 => Ok(i64::from(tag) - 5),
            128..=250 => Ok(i64::from(tag) - 251),
            1..=4 => {
                let width = usize::from(tag);
                let mut value = 0i64;
                for shift in 0..width {
                    value |= i64::from(self.byte()?) << (shift * 8);
                }
                Ok(value)
            }
            251 => Ok(-5 - self.read_unsigned(5)?),
            252..=255 => {
                let width = usize::from(256u16 - u16::from(tag));
                Ok(-5 - self.read_unsigned(width)?)
            }
        }
    }

    fn read_unsigned(&mut self, width: usize) -> Result<i64, VmError> {
        let mut value = 0i64;
        for shift in 0..width {
            value |= i64::from(self.byte()?) << (shift * 8);
        }
        Ok(value)
    }

    fn length(&mut self) -> Result<usize, VmError> {
        let length = self.long()?;
        if length < 0 {
            return self.fail("negative length");
        }
        usize::try_from(length).map_err(|_| self.vm.raise_arg("Marshal length is too large"))
    }

    fn symbol(&mut self) -> Result<sabiruby::symbol::Sym, VmError> {
        let length = self.length()?;
        let bytes = self.bytes(length)?.to_vec();
        let symbol = self.vm.intern(&String::from_utf8_lossy(&bytes));
        self.symbols.push(symbol);
        Ok(symbol)
    }

    fn symbol_link(&mut self) -> Result<sabiruby::symbol::Sym, VmError> {
        let index = self.length()?;
        self.symbols
            .get(index)
            .copied()
            .ok_or_else(|| self.vm.raise_arg("invalid Marshal symbol link"))
    }

    fn symbol_value(&mut self) -> Result<sabiruby::symbol::Sym, VmError> {
        match self.byte()? {
            b':' => self.symbol(),
            b';' => self.symbol_link(),
            tag => self.fail(format!("expected Marshal symbol, got 0x{tag:02x}")),
        }
    }

    fn object_link(&mut self) -> Result<Value, VmError> {
        let index = self.length()?;
        self.objects.get(index).copied().ok_or_else(|| {
            self.vm.raise_arg(&format!(
                "invalid Marshal object link {index} at offset {} ({} objects)",
                self.offset,
                self.objects.len()
            ))
        })
    }

    fn parse_string(&mut self) -> Result<Value, VmError> {
        let length = self.length()?;
        let bytes = self.bytes(length)?.to_vec();
        let value = self.vm.str_new(&bytes);
        self.objects.push(value);
        Ok(value)
    }

    fn parse_array(&mut self) -> Result<Value, VmError> {
        let array = self.vm.ary_new(Vec::new());
        self.objects.push(array);
        for _ in 0..self.length()? {
            let value = self.value()?;
            self.vm.ary_push(array, value)?;
        }
        Ok(array)
    }

    fn parse_hash(&mut self, with_default: bool) -> Result<Value, VmError> {
        let hash = self.vm.hash_new();
        self.objects.push(hash);
        for _ in 0..self.length()? {
            let key = self.value()?;
            let value = self.value()?;
            self.vm.hash_set(hash, key, value)?;
        }
        if with_default {
            // The current Ruby hash API does not expose a native default setter;
            // consume the value so subsequent objects remain aligned. Essentials
            // data hashes do not use a non-nil default.
            let _ = self.value()?;
        }
        Ok(hash)
    }

    fn parse_object(&mut self) -> Result<Value, VmError> {
        let class_name = self.symbol_value()?;
        let name = self.vm.sym_name(class_name);
        let class = self.class_for_name(&name)?;
        let object = self.vm.object_new(class);
        self.objects.push(object);
        for _ in 0..self.length()? {
            let symbol = self.symbol_value()?;
            let name = self.vm.sym_name(symbol);
            let value = self.value()?;
            if let Some(object) = object.obj() {
                self.vm.ivar_set(object, &name, value);
            }
        }
        Ok(object)
    }

    fn class_for_name(&mut self, name: &str) -> Result<ObjId, VmError> {
        if let Some(Value::Obj(class)) = self.vm.const_path_get(name) {
            return Ok(class);
        }
        let superclass = match self.vm.const_path_get("RGSSData") {
            Some(Value::Obj(class)) => class,
            _ => self.vm.core.object,
        };
        let mut outer = self.vm.core.object;
        for (index, part) in name.split("::").filter(|part| !part.is_empty()).enumerate() {
            let symbol = self.vm.intern(part);
            if let Some(Value::Obj(value)) = self.vm.const_get(outer, symbol) {
                outer = value;
                continue;
            }
            outer = if index == 0 {
                self.vm.define_module(part)
            } else {
                self.vm.define_class_under(outer, part, superclass)
            };
        }
        if self.vm.const_path_get(name).is_some() {
            Ok(outer)
        } else {
            self.fail(format!("cannot create Marshal class {name}"))
        }
    }

    fn parse_bignum(&mut self) -> Result<Value, VmError> {
        let negative = self.byte()? == b'-';
        let words = self.length()?;
        let Some(byte_count) = words.checked_mul(2) else {
            return self.fail("Marshal bignum overflow");
        };
        let bytes = self.bytes(byte_count)?.to_vec();
        let value = sabiruby::bigint::BigInt::from_bytes_le(&bytes);
        let value = if negative { value.neg() } else { value };
        let value = self.vm.bint_value(value);
        self.objects.push(value);
        Ok(value)
    }

    fn parse_float(&mut self) -> Result<Value, VmError> {
        let length = self.length()?;
        let bytes = self.bytes(length)?.to_vec();
        let text = String::from_utf8_lossy(&bytes);
        let value = match text.as_ref() {
            "nan" => f64::NAN,
            "inf" => f64::INFINITY,
            "-inf" => f64::NEG_INFINITY,
            _ => text
                .parse()
                .map_err(|_| self.vm.raise_arg("invalid Marshal float"))?,
        };
        let value = Value::Float(value);
        self.objects.push(value);
        Ok(value)
    }

    fn parse_user_object(&mut self, raw: bool) -> Result<Value, VmError> {
        let class_name = self.symbol_value()?;
        let name = self.vm.sym_name(class_name);
        let class = self.class_for_name(&name)?;
        let payload = if raw {
            let length = self.length()?;
            let bytes = self.bytes(length)?.to_vec();
            if name == "Table" {
                let object = self.vm.object_new(class);
                self.objects.push(object);
                self.decode_table(object, &bytes)?;
                return Ok(object);
            }
            self.vm.str_new(&bytes)
        } else {
            self.value()?
        };
        let object = self.vm.object_new(class);
        self.objects.push(object);
        if let Some(object) = object.obj() {
            self.vm.ivar_set(object, "@__marshal_data", payload);
        }
        Ok(object)
    }

    fn decode_table(&mut self, object: Value, bytes: &[u8]) -> Result<(), VmError> {
        if bytes.len() < 20 {
            return self.fail("truncated Table payload");
        }
        let read_u32 = |offset: usize| -> Option<u32> {
            let end = offset.checked_add(4)?;
            Some(u32::from_le_bytes(bytes.get(offset..end)?.try_into().ok()?))
        };
        let dimensions =
            read_u32(0).ok_or_else(|| self.vm.raise_arg("invalid Table dimensions"))?;
        let xsize = read_u32(4).ok_or_else(|| self.vm.raise_arg("invalid Table xsize"))?;
        let ysize = read_u32(8).ok_or_else(|| self.vm.raise_arg("invalid Table ysize"))?;
        let zsize = read_u32(12).unwrap_or(1);
        let data_offset = 16;
        let data_size = read_u32(data_offset)
            .ok_or_else(|| self.vm.raise_arg("invalid Table data size"))?
            as usize;
        let values_offset = data_offset + 4;
        let available = bytes.len().saturating_sub(values_offset) / 4;
        let count = data_size.min(available);
        let values = (0..count)
            .map(|index| {
                let offset = values_offset + index * 4;
                i32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("Table word")) as i64
            })
            .map(Value::Int)
            .collect();
        let Some(object) = object.obj() else {
            return self.fail("Table payload did not create an object");
        };
        self.vm
            .ivar_set(object, "@dimensions", Value::Int(i64::from(dimensions)));
        self.vm
            .ivar_set(object, "@xsize", Value::Int(i64::from(xsize)));
        self.vm
            .ivar_set(object, "@ysize", Value::Int(i64::from(ysize)));
        self.vm
            .ivar_set(object, "@zsize", Value::Int(i64::from(zsize)));
        let data = self.vm.ary_new(values);
        self.vm.ivar_set(object, "@data", data);
        Ok(())
    }

    fn parse_ivar_wrapper(&mut self) -> Result<Value, VmError> {
        let object = self.value()?;
        for _ in 0..self.length()? {
            let symbol = self.symbol_value()?;
            let name = self.vm.sym_name(symbol);
            let value = self.value()?;
            // `:E` is MRI's String encoding flag. SabiRuby strings are UTF-8/
            // byte strings already, so retaining the data is sufficient.
            if name != "E" {
                if let Some(object) = object.obj() {
                    self.vm.ivar_set(object, &name, value);
                }
            }
        }
        Ok(object)
    }

    fn value(&mut self) -> Result<Value, VmError> {
        match self.byte()? {
            b'0' => Ok(Value::Nil),
            b'T' => Ok(Value::True),
            b'F' => Ok(Value::False),
            b'i' => Ok(Value::Int(self.long()?)),
            b'l' => self.parse_bignum(),
            b'f' => self.parse_float(),
            b':' => Ok(Value::Sym(self.symbol()?)),
            b';' => Ok(Value::Sym(self.symbol_link()?)),
            b'"' => self.parse_string(),
            b'I' => self.parse_ivar_wrapper(),
            b'[' => self.parse_array(),
            b'{' => self.parse_hash(false),
            b'}' => self.parse_hash(true),
            b'o' => self.parse_object(),
            b'u' => self.parse_user_object(true),
            b'U' => self.parse_user_object(false),
            b'C' => {
                let _class = self.symbol_value()?;
                self.value()
            }
            b'c' | b'm' => {
                let symbol = self.symbol_value()?;
                self.vm
                    .const_path_get(&self.vm.sym_name(symbol))
                    .ok_or_else(|| self.vm.raise_arg("unknown Marshal constant"))
            }
            b'e' => {
                let _module = self.symbol_value()?;
                self.value()
            }
            b'@' => self.object_link(),
            tag => self.fail(format!("unsupported tag 0x{tag:02x}")),
        }
    }

    fn decode(mut self) -> Result<Value, VmError> {
        if self.byte()? != 4 || self.byte()? != 8 {
            return self.fail("unsupported Marshal version");
        }
        self.value()
    }
}

fn rgss_load_data(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let bytes = {
        let vfs = state_mut(vm).vfs.clone();
        vfs.read(&path)
            .map_err(|error| vm.raise_arg(&format!("cannot load {path}: {error}")))?
    };
    MarshalDecoder::new(vm, &bytes).decode()
}

#[derive(Clone, Debug)]
struct ScriptArchiveEntry {
    id: i64,
    name: String,
    source: Vec<u8>,
}

fn inflate_script(bytes: &[u8]) -> Result<Vec<u8>, std::io::Error> {
    let mut source = Vec::new();
    let mut zlib = ZlibDecoder::new(bytes);
    match zlib.read_to_end(&mut source) {
        Ok(_) => return Ok(source),
        Err(zlib_error) => {
            source.clear();
            let mut raw = DeflateDecoder::new(bytes);
            raw.read_to_end(&mut source).map_err(|raw_error| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    format!("zlib: {zlib_error}; raw deflate: {raw_error}"),
                )
            })?;
        }
    }
    Ok(source)
}

fn read_script_archive(
    vm: &mut Vm,
    vfs: &dyn VirtualFileSystem,
    path: &str,
) -> Result<Vec<ScriptArchiveEntry>, RgssError> {
    let bytes = vfs.read(path).map_err(|error| {
        RgssError::InvalidProject(format!("cannot read script archive {path}: {error}"))
    })?;
    let archive = MarshalDecoder::new(vm, &bytes).decode()?;
    let Some(records) = vm.ary_vals(archive) else {
        return Err(RgssError::InvalidProject(format!(
            "script archive {path} is not an Array"
        )));
    };
    let mut scripts = Vec::with_capacity(records.len());
    for (index, record) in records.into_iter().enumerate() {
        let Some(fields) = vm.ary_vals(record) else {
            return Err(RgssError::InvalidProject(format!(
                "script archive {path} entry {index} is not an Array"
            )));
        };
        let Some(name) = fields
            .get(1)
            .and_then(|value| vm.str_bytes(*value))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        else {
            return Err(RgssError::InvalidProject(format!(
                "script archive {path} entry {index} has no name"
            )));
        };
        let Some(compressed) = fields
            .get(2)
            .and_then(|value| vm.str_bytes(*value))
            .map(ToOwned::to_owned)
        else {
            return Err(RgssError::InvalidProject(format!(
                "script archive {path} entry {index} has no source"
            )));
        };
        let id = fields
            .first()
            .and_then(|value| match value {
                Value::Int(id) => Some(*id),
                _ => None,
            })
            .unwrap_or(index as i64);
        let source = inflate_script(&compressed).map_err(|error| {
            RgssError::InvalidProject(format!("cannot inflate script {name} from {path}: {error}"))
        })?;
        scripts.push(ScriptArchiveEntry { id, name, source });
    }
    Ok(scripts)
}

fn install_script_global(vm: &mut Vm, scripts: &[ScriptArchiveEntry]) {
    let records = scripts
        .iter()
        .map(|script| {
            let id = Value::Int(script.id);
            let name = vm.str_new(script.name.as_bytes());
            // Keep the decoded source in both slots. The compatibility
            // prelude's Inflate facade accepts this embedded representation;
            // slot 3 mirrors rsxp's decoded-source convenience.
            let source = vm.str_new(&script.source);
            vm.ary_new(vec![id, name, source, source])
        })
        .collect();
    let scripts = vm.ary_new(records);
    vm.global_set("$RGSS_SCRIPTS", scripts);
}

fn file_directory(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    Ok(Value::bool(
        vfs.metadata(&path).is_ok_and(|metadata| metadata.is_dir),
    ))
}
fn file_file(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    Ok(Value::bool(
        vfs.metadata(&path).is_ok_and(|metadata| metadata.is_file),
    ))
}
fn file_exist(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    Ok(Value::bool(vfs.metadata(&path).is_ok()))
}
fn file_join(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let mut joined = String::new();
    for value in args {
        let part = String::from_utf8_lossy(&vm.expect_str(*value, "path")?).into_owned();
        if !joined.is_empty() && !joined.ends_with('/') {
            joined.push('/');
        }
        joined.push_str(part.trim_matches(['/', '\\']));
    }
    Ok(vm.str_new(joined.as_bytes()))
}
fn file_basename(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = path_arg(vm, args)?.replace('\\', "/");
    let trimmed = path.trim_end_matches('/');
    let mut basename = trimmed.rsplit('/').next().unwrap_or(trimmed).to_owned();
    if let Some(suffix) = args.get(1).copied().and_then(|value| vm.str_bytes(value)) {
        let suffix = String::from_utf8_lossy(suffix);
        if suffix == ".*" {
            if let Some(dot) = basename.rfind('.') {
                basename.truncate(dot);
            }
        } else if basename.ends_with(suffix.as_ref()) {
            basename.truncate(basename.len() - suffix.len());
        }
    }
    Ok(vm.str_new(basename.as_bytes()))
}
fn file_dirname(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = path_arg(vm, args)?.replace('\\', "/");
    let trimmed = path.trim_end_matches('/');
    let dirname =
        trimmed.rsplit_once('/').map_or(
            ".",
            |(parent, _)| if parent.is_empty() { "/" } else { parent },
        );
    Ok(vm.str_new(dirname.as_bytes()))
}
fn file_self_path(vm: &Vm, receiver: Value) -> Option<String> {
    let obj = receiver.obj()?;
    let name = vm.ivar_get(obj, "@path");
    let bytes = vm.str_bytes(name)?;
    Some(String::from_utf8_lossy(bytes).replace('\\', "/"))
}
fn file_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let Some(obj) = receiver.obj() else {
        return Ok(receiver);
    };
    let path = vfs_arg(vm, args)?;
    let path_value = vm.str_new(path.as_bytes());
    let mode_value = match args.get(1).copied() {
        Some(value) => value,
        None => vm.str_new(b"rb"),
    };
    vm.ivar_set(obj, "@path", path_value);
    vm.ivar_set(obj, "@mode", mode_value);
    vm.ivar_set(obj, "@pos", Value::Int(0));
    Ok(receiver)
}
fn file_open(vm: &mut Vm, _: Value, args: &[Value], block: Value) -> Result<Value, VmError> {
    let file_sym = vm.intern("File");
    let file_class = match vm.const_get(vm.core.object, file_sym) {
        Some(Value::Obj(c)) => c,
        _ => return Ok(Value::Nil),
    };
    let new_sym = vm.intern("new");
    let file = vm.funcall(Value::Obj(file_class), new_sym, args, Value::Nil)?;
    if block.is_nil() {
        return Ok(file);
    }
    vm.call_block(block, &[file])
}
fn file_class_read(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    let mut bytes = vfs.read(&path).unwrap_or_default();
    if let Some(Value::Int(length)) = args.get(1).copied() {
        let offset = args
            .get(2)
            .and_then(|value| match value {
                Value::Int(offset) => Some((*offset).max(0) as usize),
                _ => None,
            })
            .unwrap_or(0);
        bytes = bytes.get(offset..).unwrap_or_default().to_vec();
        bytes.truncate(length.max(0) as usize);
    }
    Ok(vm.str_new(&bytes))
}
fn file_class_readlines(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    let bytes = vfs.read(&path).unwrap_or_default();
    let lines = String::from_utf8_lossy(&bytes)
        .split_inclusive('\n')
        .map(|line| vm.str_new(line.as_bytes()))
        .collect();
    Ok(vm.ary_new(lines))
}
fn file_class_foreach(
    vm: &mut Vm,
    _: Value,
    args: &[Value],
    block: Value,
) -> Result<Value, VmError> {
    if block.is_nil() {
        return Err(vm.raise_arg("no block given"));
    }
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    let bytes = vfs
        .read(&path)
        .map_err(|error| vm.raise_arg(&format!("cannot read {path}: {error}")))?;
    for line in String::from_utf8_lossy(&bytes).split_inclusive('\n') {
        let value = vm.str_new(line.as_bytes());
        vm.call_block(block, &[value])?;
    }
    Ok(Value::Nil)
}
fn file_class_write(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let value = args.get(1).copied().unwrap_or(Value::Nil);
    let bytes = vm.expect_str(value, "string")?;
    let vfs = state_mut(vm).vfs.clone();
    vfs.write(&path, &bytes, false)
        .map_err(|error| vm.raise_arg(&format!("cannot write {path}: {error}")))?;
    Ok(Value::Int(bytes.len() as i64))
}
fn file_delete(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    vfs.remove(&path)
        .map_err(|error| vm.raise_arg(&format!("cannot delete {path}: {error}")))?;
    Ok(Value::Int(1))
}
fn file_copy(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let source = vfs_arg(vm, args)?;
    let Some(destination_value) = args.get(1).copied() else {
        return Err(vm.raise_arg("destination path required"));
    };
    let destination_arg =
        String::from_utf8_lossy(&vm.expect_str(destination_value, "path")?).into_owned();
    let destination = vfs_path(vm, &destination_arg);
    let vfs = state_mut(vm).vfs.clone();
    let bytes = vfs
        .read(&source)
        .map_err(|error| vm.raise_arg(&format!("cannot read {source}: {error}")))?;
    vfs.write(&destination, &bytes, false)
        .map_err(|error| vm.raise_arg(&format!("cannot write {destination}: {error}")))?;
    Ok(Value::Int(1))
}
fn file_move(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let source = vfs_arg(vm, args)?;
    let Some(destination_value) = args.get(1).copied() else {
        return Err(vm.raise_arg("destination path required"));
    };
    let destination_arg =
        String::from_utf8_lossy(&vm.expect_str(destination_value, "path")?).into_owned();
    let destination = vfs_path(vm, &destination_arg);
    let vfs = state_mut(vm).vfs.clone();
    match vfs.rename(&source, &destination) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::Unsupported => {
            let bytes = vfs
                .read(&source)
                .map_err(|error| vm.raise_arg(&format!("cannot read {source}: {error}")))?;
            vfs.write(&destination, &bytes, false)
                .map_err(|error| vm.raise_arg(&format!("cannot write {destination}: {error}")))?;
            vfs.remove(&source)
                .map_err(|error| vm.raise_arg(&format!("cannot delete {source}: {error}")))?;
        }
        Err(error) => return Err(vm.raise_arg(&format!("cannot move {source}: {error}"))),
    }
    Ok(Value::Int(1))
}
fn file_read(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let Some(path) = file_self_path(vm, receiver) else {
        return Ok(vm.str_new(b""));
    };
    let vfs = state_mut(vm).vfs.clone();
    let mut bytes = vfs.read(&path).unwrap_or_default();
    let pos = receiver
        .obj()
        .map(|o| vm.ivar_get(o, "@pos"))
        .and_then(|v| match v {
            Value::Int(n) => Some(n.max(0) as usize),
            _ => None,
        })
        .unwrap_or(0);
    if pos >= bytes.len() {
        bytes.clear();
    } else {
        bytes = bytes[pos..].to_vec();
    }
    if let Some(Value::Int(length)) = args.first().copied() {
        bytes.truncate(length.max(0) as usize);
    }
    if let Some(obj) = receiver.obj() {
        vm.ivar_set(obj, "@pos", Value::Int((pos + bytes.len()) as i64));
    }
    Ok(vm.str_new(&bytes))
}
fn file_eof(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let Some(path) = file_self_path(vm, receiver) else {
        return Ok(Value::True);
    };
    let vfs = state_mut(vm).vfs.clone();
    let len = vfs
        .metadata(&path)
        .map(|metadata| metadata.len as i64)
        .unwrap_or(0);
    let pos = receiver
        .obj()
        .map(|o| vm.ivar_get(o, "@pos"))
        .and_then(|v| match v {
            Value::Int(n) => Some(n),
            _ => None,
        })
        .unwrap_or(0);
    Ok(Value::bool(pos >= len))
}
fn file_pos(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let Some(obj) = receiver.obj() else {
        return Ok(Value::Int(0));
    };
    if let Some(value) = args.first().copied() {
        vm.ivar_set(obj, "@pos", value);
    }
    Ok(vm.ivar_get(obj, "@pos"))
}
fn file_write(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let Some(value) = args.first().copied() else {
        return Ok(Value::Int(0));
    };
    let bytes = vm.expect_str(value, "string")?;
    let Some(path) = file_self_path(vm, receiver) else {
        return Ok(Value::Int(bytes.len() as i64));
    };
    let position = receiver
        .obj()
        .and_then(|obj| match vm.ivar_get(obj, "@pos") {
            Value::Int(position) => Some(position.max(0) as usize),
            _ => None,
        })
        .unwrap_or(0);
    let append = receiver
        .obj()
        .and_then(|obj| vm.str_bytes(vm.ivar_get(obj, "@mode")))
        .is_some_and(|mode| mode.contains(&b'a'))
        || position > 0;
    let vfs = state_mut(vm).vfs.clone();
    let _ = vfs.write(&path, &bytes, append);
    if let Some(obj) = receiver.obj() {
        let next_position = if append {
            position.saturating_add(bytes.len())
        } else {
            bytes.len()
        };
        vm.ivar_set(obj, "@pos", Value::Int(next_position as i64));
    }
    Ok(Value::Int(bytes.len() as i64))
}
fn file_noop(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Nil)
}

fn dir_path_arg(vm: &mut Vm, args: &[Value]) -> Result<String, VmError> {
    if args.is_empty() {
        Ok(".".to_owned())
    } else {
        path_arg(vm, args)
    }
}

fn read_dir_for_vm(vm: &mut Vm, path: &str) -> Result<Vec<VfsDirEntry>, VmError> {
    let vfs = state_mut(vm).vfs.clone();
    vfs.read_dir(path)
        .map_err(|error| vm.raise_arg(&format!("cannot read directory {path}: {error}")))
}

fn dir_entries(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_dir_arg(vm, args)?;
    let entries = read_dir_for_vm(vm, &path)?;
    let mut values = vec![vm.str_new(b"."), vm.str_new(b"..")];
    values.extend(
        entries
            .iter()
            .map(|entry| vm.str_new(entry.name.as_bytes())),
    );
    Ok(vm.ary_new(values))
}

fn dir_children(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_dir_arg(vm, args)?;
    let entries = read_dir_for_vm(vm, &path)?;
    let values = entries
        .iter()
        .map(|entry| vm.str_new(entry.name.as_bytes()))
        .collect();
    Ok(vm.ary_new(values))
}

fn dir_foreach(vm: &mut Vm, _: Value, args: &[Value], block: Value) -> Result<Value, VmError> {
    if block.is_nil() {
        return Err(vm.raise_arg("no block given"));
    }
    let path = vfs_dir_arg(vm, args)?;
    let entries = read_dir_for_vm(vm, &path)?;
    let dot = vm.str_new(b".");
    vm.call_block(block, &[dot])?;
    let dot_dot = vm.str_new(b"..");
    vm.call_block(block, &[dot_dot])?;
    for entry in entries {
        let value = vm.str_new(entry.name.as_bytes());
        vm.call_block(block, &[value])?;
    }
    Ok(Value::Nil)
}
fn dir_mkdir(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_dir_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    vfs.create_dir(&path)
        .map_err(|error| vm.raise_arg(&format!("cannot create directory {path}: {error}")))?;
    Ok(Value::Int(0))
}
fn dir_delete(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_dir_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    vfs.remove(&path)
        .map_err(|error| vm.raise_arg(&format!("cannot delete directory {path}: {error}")))?;
    Ok(Value::Int(0))
}
fn dir_chdir(vm: &mut Vm, _: Value, args: &[Value], block: Value) -> Result<Value, VmError> {
    let path = vfs_dir_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    if !vfs.metadata(&path).is_ok_and(|metadata| metadata.is_dir) {
        return Err(vm.raise_arg(&format!("cannot chdir to {path}")));
    }
    let previous = state_mut(vm).working_directory.clone();
    state_mut(vm).working_directory = path;
    if block.is_nil() {
        return Ok(Value::Int(0));
    }
    let result = vm.call_block(block, &[]);
    state_mut(vm).working_directory = previous;
    result
}

fn glob_segment_matches(pattern: &str, value: &str) -> bool {
    fn matches(pattern: &[u8], value: &[u8]) -> bool {
        match pattern.first().copied() {
            None => value.is_empty(),
            Some(b'*') => {
                matches(&pattern[1..], value)
                    || (!value.is_empty() && matches(pattern, &value[1..]))
            }
            Some(b'?') => !value.is_empty() && matches(&pattern[1..], &value[1..]),
            Some(byte) => {
                value.first().copied() == Some(byte) && matches(&pattern[1..], &value[1..])
            }
        }
    }
    matches(pattern.as_bytes(), value.as_bytes())
}

fn glob_collect(
    vfs: &dyn VirtualFileSystem,
    base: &str,
    segments: &[&str],
    index: usize,
    matches: &mut Vec<String>,
) -> std::io::Result<()> {
    if index == segments.len() {
        if base.is_empty() {
            matches.push(".".to_owned());
        } else if vfs.metadata(base).is_ok() {
            matches.push(base.to_owned());
        }
        return Ok(());
    }

    let segment = segments[index];
    if segment == "**" {
        glob_collect(vfs, base, segments, index + 1, matches)?;
        let entries = vfs.read_dir(if base.is_empty() { "." } else { base })?;
        for entry in entries {
            if entry.metadata.is_dir {
                let child = vfs_join(base, &entry.name);
                glob_collect(vfs, &child, segments, index, matches)?;
            }
        }
        return Ok(());
    }

    let has_wildcard = segment.contains('*') || segment.contains('?');
    if !has_wildcard {
        let child = vfs_join(base, segment);
        if index + 1 == segments.len() {
            if vfs.metadata(&child).is_ok() {
                matches.push(child);
            }
        } else if vfs.metadata(&child).is_ok_and(|metadata| metadata.is_dir) {
            glob_collect(vfs, &child, segments, index + 1, matches)?;
        }
        return Ok(());
    }

    let entries = vfs.read_dir(if base.is_empty() { "." } else { base })?;
    for entry in entries {
        if entry.name.starts_with('.') && !segment.starts_with('.') {
            continue;
        }
        if !glob_segment_matches(segment, &entry.name) {
            continue;
        }
        let child = vfs_join(base, &entry.name);
        if index + 1 == segments.len() {
            matches.push(child);
        } else if entry.metadata.is_dir {
            glob_collect(vfs, &child, segments, index + 1, matches)?;
        }
    }
    Ok(())
}

fn dir_glob(vm: &mut Vm, _: Value, args: &[Value], block: Value) -> Result<Value, VmError> {
    let pattern = path_arg(vm, args)?.replace('\\', "/");
    let cwd = state_mut(vm).working_directory.clone();
    let full_pattern = vfs_path(vm, &pattern);
    let vfs = state_mut(vm).vfs.clone();
    let mut paths = Vec::new();
    let full_normalized = full_pattern
        .strip_prefix("./")
        .unwrap_or(&full_pattern)
        .trim_matches('/');
    let full_segments: Vec<&str> = if full_normalized.is_empty() {
        Vec::new()
    } else {
        full_normalized.split('/').collect()
    };
    glob_collect(vfs.as_ref(), "", &full_segments, 0, &mut paths)
        .map_err(|error| vm.raise_arg(&format!("cannot glob {pattern}: {error}")))?;
    if cwd != "." {
        let prefix = format!("{cwd}/");
        for path in &mut paths {
            if let Some(relative) = path.strip_prefix(&prefix) {
                *path = relative.to_owned();
            }
        }
    }
    paths.sort();
    paths.dedup();
    let values: Vec<Value> = paths
        .iter()
        .map(|path| vm.str_new(path.as_bytes()))
        .collect();
    if block.is_nil() {
        Ok(vm.ary_new(values))
    } else {
        for value in values {
            vm.call_block(block, &[value])?;
        }
        Ok(Value::Nil)
    }
}

fn dir_pwd(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let path = state_mut(vm).working_directory.clone();
    Ok(vm.str_new(path.as_bytes()))
}

fn image_dimensions(bytes: &[u8]) -> Option<(i64, i64)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") && bytes.len() >= 24 {
        let width = u32::from_be_bytes(bytes[16..20].try_into().ok()?) as i64;
        let height = u32::from_be_bytes(bytes[20..24].try_into().ok()?) as i64;
        return Some((width.max(1), height.max(1)));
    }
    if (bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a")) && bytes.len() >= 10 {
        let width = u16::from_le_bytes(bytes[6..8].try_into().ok()?) as i64;
        let height = u16::from_le_bytes(bytes[8..10].try_into().ok()?) as i64;
        return Some((width.max(1), height.max(1)));
    }
    if bytes.starts_with(b"BM") && bytes.len() >= 26 {
        let width = i32::from_le_bytes(bytes[18..22].try_into().ok()?).unsigned_abs() as i64;
        let height = i32::from_le_bytes(bytes[22..26].try_into().ok()?).unsigned_abs() as i64;
        return Some((width.max(1), height.max(1)));
    }
    None
}

struct DecodedImage {
    width: i64,
    height: i64,
    pixels: Vec<u8>,
    frames: Vec<Vec<u8>>,
    frame_delays_ms: Vec<u64>,
}

fn decode_image(bytes: &[u8]) -> Option<DecodedImage> {
    if image::guess_format(bytes).ok() == Some(image::ImageFormat::Gif) {
        let decoder = image::codecs::gif::GifDecoder::new(Cursor::new(bytes)).ok()?;
        let frames = decoder.into_frames().collect_frames().ok()?;
        if frames.len() > 1 {
            let first = frames.first()?;
            let (width, height) = first.buffer().dimensions();
            let mut pixels = Vec::with_capacity(frames.len());
            let mut frame_delays_ms = Vec::with_capacity(frames.len());
            for frame in frames {
                let (numerator, denominator) = frame.delay().numer_denom_ms();
                let delay = (u64::from(numerator) / u64::from(denominator.max(1))).max(1);
                frame_delays_ms.push(delay);
                pixels.push(frame.into_buffer().into_raw());
            }
            return Some(DecodedImage {
                width: width.max(1) as i64,
                height: height.max(1) as i64,
                pixels: pixels.first()?.clone(),
                frames: pixels,
                frame_delays_ms,
            });
        }
    }
    let image = image::load_from_memory(bytes).ok()?.to_rgba8();
    let (width, height) = image.dimensions();
    Some(DecodedImage {
        width: width.max(1) as i64,
        height: height.max(1) as i64,
        pixels: image.into_raw(),
        frames: Vec::new(),
        frame_delays_ms: Vec::new(),
    })
}

/// Resolve an RGSS asset after Ruby has supplied its logical name.  This is a
/// core policy helper, not a VFS capability: the VFS only answers explicit
/// reads and optional directory enumeration.  Ruby's `RTP`/`pbResolveBitmap`
/// path remains authoritative for normal game loads; the extension and stem
/// probes here keep native `Bitmap.new("Graphics/foo")` compatible too.
fn read_asset_bytes(
    vfs: &dyn VirtualFileSystem,
    path: &str,
    extensions: &[&str],
) -> Option<Vec<u8>> {
    let normalized = path.replace('\\', "/");
    let mut candidates = vec![normalized.clone()];
    for extension in extensions {
        let candidate = format!("{normalized}.{extension}");
        if !candidates
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(&candidate))
        {
            candidates.push(candidate);
        }
    }
    for candidate in &candidates {
        if let Ok(bytes) = vfs.read(candidate) {
            return Some(bytes);
        }
    }

    // This fallback is optional.  Archive/WASM VFS implementations that do
    // not expose enumeration still work for explicit paths and names already
    // resolved by Ruby.
    let (parent, requested_name) = normalized
        .rsplit_once('/')
        .map_or((".", normalized.as_str()), |(parent, name)| (parent, name));
    let requested_stem = Path::new(requested_name)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| requested_name.to_owned());
    let entries = vfs.read_dir(parent).ok()?;
    for entry in entries {
        if !entry.metadata.is_file {
            continue;
        }
        let exact_name = entry.name.eq_ignore_ascii_case(requested_name);
        let entry_stem = Path::new(&entry.name)
            .file_stem()
            .map(|stem| stem.to_string_lossy());
        let same_stem = entry_stem.is_some_and(|stem| stem.eq_ignore_ascii_case(&requested_stem));
        let known_extension = Path::new(&entry.name)
            .extension()
            .map(|extension| extension.to_string_lossy())
            .is_some_and(|extension| {
                extensions
                    .iter()
                    .any(|known| extension.eq_ignore_ascii_case(known))
            });
        if exact_name || (same_stem && known_extension) {
            let candidate = vfs_join(parent, &entry.name);
            if let Ok(bytes) = vfs.read(&candidate) {
                return Some(bytes);
            }
        }
    }
    None
}

fn bitmap_dimensions(vm: &mut Vm, path: &str) -> (i64, i64) {
    let vfs = state_mut(vm).vfs.clone();
    read_asset_bytes(vfs.as_ref(), path, &["png", "gif", "jpg", "jpeg", "bmp"])
        .and_then(|bytes| image_dimensions(&bytes))
        .unwrap_or((1, 1))
}

fn bitmap_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = receiver
        .obj()
        .ok_or_else(|| vm.raise_type("Bitmap receiver is not an object"))?;
    let (width, height, path, decoded_pixels) = match args {
        [Value::Int(width), Value::Int(height), ..] => (*width, *height, None, None),
        [path, ..] => {
            let path = vm
                .str_bytes(*path)
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .unwrap_or_default();
            let vfs = state_mut(vm).vfs.clone();
            let decoded =
                read_asset_bytes(vfs.as_ref(), &path, &["png", "gif", "jpg", "jpeg", "bmp"])
                    .and_then(|bytes| decode_image(&bytes));
            let (width, height) = decoded.as_ref().map_or_else(
                || bitmap_dimensions(vm, &path),
                |image| (image.width, image.height),
            );
            (width, height, Some(path), decoded)
        }
        [] => (1, 1, None, None),
    };
    vm.ivar_set(object, "@width", Value::Int(width.max(1)));
    vm.ivar_set(object, "@height", Value::Int(height.max(1)));
    vm.ivar_set(object, "@animated", Value::False);
    vm.ivar_set(object, "@frame_count", Value::Int(1));
    let has_path = path.is_some();
    let path = path
        .map(|path| vm.str_new(path.as_bytes()))
        .unwrap_or(Value::Nil);
    vm.ivar_set(object, "@path", path);
    let pixel_len = width
        .max(1)
        .checked_mul(height.max(1))
        .and_then(|size| size.checked_mul(4))
        .unwrap_or(0) as usize;
    let decoded = decoded_pixels;
    state_mut(vm).bitmaps.insert(
        object,
        BitmapData {
            width: width.max(1),
            height: height.max(1),
            pixels: decoded.as_ref().map_or_else(
                || {
                    if has_path {
                        Vec::new()
                    } else {
                        vec![0; pixel_len]
                    }
                },
                |image| image.pixels.clone(),
            ),
            revision: 1,
            frames: decoded
                .as_ref()
                .map_or_else(Vec::new, |image| image.frames.clone()),
            frame_delays_ms: decoded
                .as_ref()
                .map_or_else(Vec::new, |image| image.frame_delays_ms.clone()),
            current_frame: 0,
            animation_elapsed_ms: 0,
            manual_frame_rate: None,
            playing: false,
        },
    );
    let animated = decoded.as_ref().is_some_and(|image| image.frames.len() > 1);
    vm.ivar_set(object, "@animated", Value::bool(animated));
    vm.ivar_set(
        object,
        "@frame_count",
        Value::Int(if animated {
            decoded.as_ref().map_or(1, |image| image.frames.len()) as i64
        } else {
            1
        }),
    );
    Ok(Value::Nil)
}

fn bitmap_dimension(vm: &mut Vm, receiver: Value, name: &str) -> Result<Value, VmError> {
    let object = receiver
        .obj()
        .ok_or_else(|| vm.raise_type("Bitmap receiver is not an object"))?;
    Ok(vm.ivar_get(object, name))
}

fn bitmap_width(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    bitmap_dimension(vm, receiver, "@width")
}

fn bitmap_height(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    bitmap_dimension(vm, receiver, "@height")
}

fn bitmap_animated(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, "@animated"))
}

fn bitmap_frame_count(
    vm: &mut Vm,
    receiver: Value,
    _: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, "@frame_count"))
}

fn bitmap_frame_rate(
    vm: &mut Vm,
    receiver: Value,
    _: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let rate = state_mut(vm)
        .bitmaps
        .get(&object)
        .and_then(|bitmap| {
            bitmap.manual_frame_rate.or_else(|| {
                bitmap
                    .frame_delays_ms
                    .get(bitmap.current_frame)
                    .map(|delay| 1000.0 / f64::from((*delay).max(1) as u32))
            })
        })
        .unwrap_or(0.0);
    Ok(Value::Float(rate))
}

fn bitmap_frame_rate_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let rate = value_f64(args.first().copied().unwrap_or(Value::Nil), 0.0);
    if rate.is_finite() && rate > 0.0 {
        if let Some(bitmap) = state_mut(vm).bitmaps.get_mut(&object) {
            bitmap.manual_frame_rate = Some(rate);
        }
    }
    Ok(Value::Nil)
}

fn bitmap_play(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    if let Some(bitmap) = state_mut(vm).bitmaps.get_mut(&object) {
        bitmap.playing = true;
    }
    Ok(Value::Nil)
}

fn bitmap_stop(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    if let Some(bitmap) = state_mut(vm).bitmaps.get_mut(&object) {
        bitmap.playing = false;
    }
    Ok(Value::Nil)
}

fn bitmap_update(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    advance_animated_bitmaps(vm);
    Ok(Value::Nil)
}

fn bitmap_playing(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(Value::bool(
        state_mut(vm)
            .bitmaps
            .get(&object)
            .is_some_and(|bitmap| bitmap.playing),
    ))
}

fn bitmap_playing_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let playing = args.first().copied().unwrap_or(Value::False).truthy();
    if let Some(bitmap) = state_mut(vm).bitmaps.get_mut(&object) {
        bitmap.playing = playing;
    }
    Ok(Value::Nil)
}

fn bitmap_current_frame(
    vm: &mut Vm,
    receiver: Value,
    _: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let frame = state_mut(vm)
        .bitmaps
        .get(&object)
        .map_or(0, |bitmap| bitmap.current_frame);
    Ok(Value::Int(frame as i64))
}

fn bitmap_goto_and_stop(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let frame = int_arg(vm, args, 0)?.max(0) as usize;
    if let Some(bitmap) = state_mut(vm).bitmaps.get_mut(&object) {
        if !bitmap.frames.is_empty() {
            bitmap.current_frame = frame.min(bitmap.frames.len() - 1);
            bitmap.pixels = bitmap.frames[bitmap.current_frame].clone();
        }
        bitmap.animation_elapsed_ms = 0;
        bitmap.revision = bitmap.revision.saturating_add(1);
        bitmap.playing = false;
    }
    Ok(Value::Nil)
}

fn bitmap_goto_and_play(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let frame = int_arg(vm, args, 0)?.max(0) as usize;
    if let Some(bitmap) = state_mut(vm).bitmaps.get_mut(&object) {
        if !bitmap.frames.is_empty() {
            bitmap.current_frame = frame.min(bitmap.frames.len() - 1);
            bitmap.pixels = bitmap.frames[bitmap.current_frame].clone();
        }
        bitmap.animation_elapsed_ms = 0;
        bitmap.revision = bitmap.revision.saturating_add(1);
        bitmap.playing = true;
    }
    Ok(Value::Nil)
}

fn bitmap_rect(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let width = bitmap_width(vm, receiver, &[], Value::Nil)?;
    let height = bitmap_height(vm, receiver, &[], Value::Nil)?;
    let width = vm.expect_int(width, "bitmap width")?;
    let height = vm.expect_int(height, "bitmap height")?;
    new_rect(vm, 0, 0, width, height)
}

fn bitmap_data_mut(state: &mut RgssState, object: ObjId) -> Option<&mut BitmapData> {
    let data = state.bitmaps.get_mut(&object)?;
    data.revision = data.revision.saturating_add(1);
    if data.pixels.is_empty() {
        let size = data.width.checked_mul(data.height)?.checked_mul(4)? as usize;
        data.pixels.resize(size, 0);
    }
    Some(data)
}

fn color_rgba(vm: &Vm, value: Value) -> [u8; 4] {
    let Some(object) = value.obj() else {
        return [0, 0, 0, 0];
    };
    [
        value_i64(vm.ivar_get(object, "@red"), 255).clamp(0, 255) as u8,
        value_i64(vm.ivar_get(object, "@green"), 255).clamp(0, 255) as u8,
        value_i64(vm.ivar_get(object, "@blue"), 255).clamp(0, 255) as u8,
        value_i64(vm.ivar_get(object, "@alpha"), 255).clamp(0, 255) as u8,
    ]
}

fn bitmap_fill_rect(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let (x, y, width, height, color) = if let Some(rect) = args.first().and_then(|arg| arg.obj()) {
        (
            value_i64(vm.ivar_get(rect, "@x"), 0),
            value_i64(vm.ivar_get(rect, "@y"), 0),
            value_i64(vm.ivar_get(rect, "@width"), 0),
            value_i64(vm.ivar_get(rect, "@height"), 0),
            args.get(1).copied().unwrap_or(Value::Nil),
        )
    } else {
        (
            int_arg(vm, args, 0)?,
            int_arg(vm, args, 1)?,
            int_arg(vm, args, 2)?,
            int_arg(vm, args, 3)?,
            args.get(4).copied().unwrap_or(Value::Nil),
        )
    };
    let rgba = color_rgba(vm, color);
    let state = state_mut(vm);
    let Some(bitmap) = bitmap_data_mut(state, object) else {
        return Ok(Value::Nil);
    };
    let x0 = x.max(0).min(bitmap.width);
    let y0 = y.max(0).min(bitmap.height);
    let x1 = x.saturating_add(width).max(0).min(bitmap.width);
    let y1 = y.saturating_add(height).max(0).min(bitmap.height);
    for py in y0..y1 {
        for px in x0..x1 {
            let index = ((py * bitmap.width + px) * 4) as usize;
            bitmap.pixels[index..index + 4].copy_from_slice(&rgba);
        }
    }
    Ok(Value::Nil)
}

fn bitmap_clear(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    if let Some(bitmap) = bitmap_data_mut(state_mut(vm), object) {
        bitmap.pixels.fill(0);
    }
    Ok(Value::Nil)
}

fn bitmap_set_pixel(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let x = int_arg(vm, args, 0)?;
    let y = int_arg(vm, args, 1)?;
    let rgba = color_rgba(vm, args.get(2).copied().unwrap_or(Value::Nil));
    if let Some(bitmap) = bitmap_data_mut(state_mut(vm), object) {
        if x >= 0 && y >= 0 && x < bitmap.width && y < bitmap.height {
            let index = ((y * bitmap.width + x) * 4) as usize;
            bitmap.pixels[index..index + 4].copy_from_slice(&rgba);
        }
    }
    Ok(Value::Nil)
}

fn bitmap_get_pixel(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let x = int_arg(vm, args, 0)?;
    let y = int_arg(vm, args, 1)?;
    let rgba = state_mut(vm)
        .bitmaps
        .get(&object)
        .filter(|bitmap| x >= 0 && y >= 0 && x < bitmap.width && y < bitmap.height)
        .and_then(|bitmap| {
            let index = ((y * bitmap.width + x) * 4) as usize;
            bitmap.pixels.get(index..index + 4)
        })
        .map(|pixel| [pixel[0], pixel[1], pixel[2], pixel[3]])
        .unwrap_or([0, 0, 0, 0]);
    new_color(
        vm,
        rgba[0].into(),
        rgba[1].into(),
        rgba[2].into(),
        rgba[3].into(),
    )
}

fn rect_values(vm: &Vm, value: Value) -> Option<(i64, i64, i64, i64)> {
    let object = value.obj()?;
    Some((
        value_i64(vm.ivar_get(object, "@x"), 0),
        value_i64(vm.ivar_get(object, "@y"), 0),
        value_i64(vm.ivar_get(object, "@width"), 0),
        value_i64(vm.ivar_get(object, "@height"), 0),
    ))
}

fn bitmap_blt(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let destination = object_instance(vm, receiver)?;
    let x = int_arg(vm, args, 0)?;
    let y = int_arg(vm, args, 1)?;
    let source = args.get(2).and_then(|value| value.obj());
    let Some((sx, sy, sw, sh)) = args.get(3).and_then(|value| rect_values(vm, *value)) else {
        return Ok(Value::Nil);
    };
    let opacity =
        value_i64(args.get(4).copied().unwrap_or(Value::Int(255)), 255).clamp(0, 255) as u16;
    let Some(source) = source else {
        return Ok(Value::Nil);
    };
    let (source_width, source_height, source_pixels) = {
        let state = state_mut(vm);
        let Some(bitmap) = state.bitmaps.get(&source) else {
            return Ok(Value::Nil);
        };
        (bitmap.width, bitmap.height, bitmap.pixels.clone())
    };
    let state = state_mut(vm);
    let Some(bitmap) = bitmap_data_mut(state, destination) else {
        return Ok(Value::Nil);
    };
    for row in 0..sh.max(0) {
        for column in 0..sw.max(0) {
            let source_x = sx + column;
            let source_y = sy + row;
            let target_x = x + column;
            let target_y = y + row;
            if source_x < 0
                || source_y < 0
                || source_x >= source_width
                || source_y >= source_height
                || target_x < 0
                || target_y < 0
                || target_x >= bitmap.width
                || target_y >= bitmap.height
            {
                continue;
            }
            let source_index = ((source_y * source_width + source_x) * 4) as usize;
            let target_index = ((target_y * bitmap.width + target_x) * 4) as usize;
            if source_index + 4 > source_pixels.len() {
                continue;
            }
            bitmap.pixels[target_index..target_index + 3]
                .copy_from_slice(&source_pixels[source_index..source_index + 3]);
            bitmap.pixels[target_index + 3] =
                (u16::from(source_pixels[source_index + 3]) * opacity / 255) as u8;
        }
    }
    Ok(Value::Nil)
}

fn bitmap_stretch_blt(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let destination = object_instance(vm, receiver)?;
    let Some((dx, dy, dw, dh)) = args.get(0).and_then(|value| rect_values(vm, *value)) else {
        return Ok(Value::Nil);
    };
    let source = args.get(1).and_then(|value| value.obj());
    let Some((sx, sy, sw, sh)) = args.get(2).and_then(|value| rect_values(vm, *value)) else {
        return Ok(Value::Nil);
    };
    let opacity =
        value_i64(args.get(3).copied().unwrap_or(Value::Int(255)), 255).clamp(0, 255) as u16;
    let Some(source) = source else {
        return Ok(Value::Nil);
    };
    let (source_width, source_height, source_pixels) = {
        let state = state_mut(vm);
        let Some(bitmap) = state.bitmaps.get(&source) else {
            return Ok(Value::Nil);
        };
        (bitmap.width, bitmap.height, bitmap.pixels.clone())
    };
    let state = state_mut(vm);
    let Some(bitmap) = bitmap_data_mut(state, destination) else {
        return Ok(Value::Nil);
    };
    if dw <= 0 || dh <= 0 || sw <= 0 || sh <= 0 {
        return Ok(Value::Nil);
    }
    for row in 0..dh {
        for column in 0..dw {
            let source_x = sx + column * sw / dw;
            let source_y = sy + row * sh / dh;
            let target_x = dx + column;
            let target_y = dy + row;
            if source_x < 0
                || source_y < 0
                || source_x >= source_width
                || source_y >= source_height
                || target_x < 0
                || target_y < 0
                || target_x >= bitmap.width
                || target_y >= bitmap.height
            {
                continue;
            }
            let source_index = ((source_y * source_width + source_x) * 4) as usize;
            let target_index = ((target_y * bitmap.width + target_x) * 4) as usize;
            if source_index + 4 > source_pixels.len() {
                continue;
            }
            bitmap.pixels[target_index..target_index + 3]
                .copy_from_slice(&source_pixels[source_index..source_index + 3]);
            bitmap.pixels[target_index + 3] =
                (u16::from(source_pixels[source_index + 3]) * opacity / 255) as u8;
        }
    }
    Ok(Value::Nil)
}

fn bitmap_draw_text(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let (x, y, width, height, text, color) =
        if let Some(rect) = args.first().and_then(|arg| arg.obj()) {
            let text = args
                .get(1)
                .and_then(|value| vm.str_bytes(*value))
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .unwrap_or_default();
            (
                value_i64(vm.ivar_get(rect, "@x"), 0),
                value_i64(vm.ivar_get(rect, "@y"), 0),
                value_i64(vm.ivar_get(rect, "@width"), 0),
                value_i64(vm.ivar_get(rect, "@height"), 0),
                text,
                bitmap_font(vm, receiver, &[], Value::Nil)
                    .ok()
                    .map(|font| vm.ivar_get(font.obj().unwrap_or(object), "@color"))
                    .unwrap_or(Value::Nil),
            )
        } else {
            let text = args
                .get(4)
                .and_then(|value| vm.str_bytes(*value))
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
                .unwrap_or_default();
            (
                int_arg(vm, args, 0)?,
                int_arg(vm, args, 1)?,
                int_arg(vm, args, 2)?,
                int_arg(vm, args, 3)?,
                text,
                bitmap_font(vm, receiver, &[], Value::Nil)
                    .ok()
                    .map(|font| vm.ivar_get(font.obj().unwrap_or(object), "@color"))
                    .unwrap_or(Value::Nil),
            )
        };
    let rgba = color_rgba(vm, color);
    let font = bitmap_font(vm, receiver, &[], Value::Nil)?;
    let size = font
        .obj()
        .map(|font| value_i64(vm.ivar_get(font, "@size"), 24))
        .unwrap_or(24)
        .max(1);
    let alignment = args
        .get(5)
        .and_then(|value| match value {
            Value::Int(value) => Some(*value),
            _ => None,
        })
        .unwrap_or(0);
    let raster_font = fontdue_font(vm, font);
    let state = state_mut(vm);
    let Some(bitmap) = bitmap_data_mut(state, object) else {
        return Ok(Value::Nil);
    };
    if let Some(font) = raster_font.as_ref() {
        let scale = size as f32;
        let line_metrics = font.horizontal_line_metrics(scale);
        let ascent = line_metrics
            .as_ref()
            .map_or(scale * 0.8, |metrics| metrics.ascent);
        let line_height = line_metrics.as_ref().map_or(scale * 1.2, |metrics| {
            (metrics.ascent - metrics.descent + metrics.line_gap).max(scale)
        });
        let clip_right = if width > 0 {
            x.saturating_add(width)
        } else {
            bitmap.width
        };
        let clip_bottom = if height > 0 {
            y.saturating_add(height)
        } else {
            bitmap.height
        };
        for (line_index, line) in text.lines().enumerate() {
            let line_width = line_width(font, scale, line);
            let mut cursor_x = match alignment {
                1 => x as f32 + ((width as f32 - line_width) / 2.0).max(0.0),
                2 => x as f32 + (width as f32 - line_width).max(0.0),
                _ => x as f32,
            };
            let baseline = y as f32 + ascent + line_index as f32 * line_height;
            for character in line.chars() {
                let (metrics, coverage) = font.rasterize(character, scale);
                let glyph_x = cursor_x + metrics.xmin as f32;
                let glyph_y = baseline - metrics.height as f32 - metrics.ymin as f32;
                for row in 0..metrics.height {
                    for column in 0..metrics.width {
                        let coverage = coverage[row * metrics.width + column];
                        if coverage == 0 {
                            continue;
                        }
                        let px = glyph_x as i64 + column as i64;
                        let py = glyph_y as i64 + row as i64;
                        if px >= x && px < clip_right && py >= y && py < clip_bottom {
                            blend_bitmap_pixel(bitmap, px, py, rgba, coverage as f32 / 255.0);
                        }
                    }
                }
                cursor_x += metrics.advance_width;
            }
        }
    } else {
        let glyph_width = (size * 3 / 5).max(1);
        for (index, _) in text.lines().flat_map(str::chars).enumerate() {
            let gx = x.saturating_add(index as i64 * glyph_width);
            let x0 = gx.max(0).min(bitmap.width);
            let x1 = gx.saturating_add(glyph_width - 1).max(0).min(bitmap.width);
            let y0 = y.max(0).min(bitmap.height);
            let y1 = y.saturating_add(height.min(size)).max(0).min(bitmap.height);
            for py in y0..y1 {
                for px in x0..x1 {
                    let pixel = ((py * bitmap.width + px) * 4) as usize;
                    bitmap.pixels[pixel..pixel + 4].copy_from_slice(&rgba);
                }
            }
        }
    }
    let _ = width;
    Ok(Value::Nil)
}

fn blend_bitmap_pixel(bitmap: &mut BitmapData, x: i64, y: i64, color: [u8; 4], coverage: f32) {
    if x < 0 || y < 0 || x >= bitmap.width || y >= bitmap.height {
        return;
    }
    let index = ((y * bitmap.width + x) * 4) as usize;
    let source_alpha = (f32::from(color[3]) * coverage.clamp(0.0, 1.0)).round() as u32;
    let destination_alpha = u32::from(bitmap.pixels[index + 3]);
    let output_alpha = source_alpha + destination_alpha * (255 - source_alpha) / 255;
    if output_alpha == 0 {
        bitmap.pixels[index..index + 4].fill(0);
        return;
    }
    for channel in 0..3 {
        let source = u32::from(color[channel]) * source_alpha;
        let destination =
            u32::from(bitmap.pixels[index + channel]) * destination_alpha * (255 - source_alpha)
                / 255;
        bitmap.pixels[index + channel] = ((source + destination) / output_alpha).min(255) as u8;
    }
    bitmap.pixels[index + 3] = output_alpha.min(255) as u8;
}

/// Return the deterministic layout rectangle RGSS scripts expect from
/// `Bitmap#text_size`.  The desktop renderer can replace the approximation
/// with its font backend later; keeping the result a real `Rect` is important
/// because Essentials uses its width/height immediately while laying out UI.
fn bitmap_text_size(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let text = args
        .first()
        .and_then(|value| vm.str_bytes(*value))
        .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
        .unwrap_or_default();
    let font = bitmap_font(vm, receiver, &[], Value::Nil)?;
    let size = font
        .obj()
        .map(|object| vm.ivar_get(object, "@size"))
        .and_then(|value| match value {
            Value::Int(value) => Some(value.max(1)),
            _ => None,
        })
        .unwrap_or(24);
    let (width, height) = if let Some(font) = fontdue_font(vm, font) {
        let scale = size as f32;
        let line_metrics = font.horizontal_line_metrics(scale);
        let ascent = line_metrics
            .as_ref()
            .map_or(scale * 0.8, |metrics| metrics.ascent);
        let line_height = line_metrics.as_ref().map_or(scale * 1.2, |metrics| {
            (metrics.ascent - metrics.descent + metrics.line_gap).max(scale)
        });
        let width = text
            .lines()
            .map(|line| line_width(&font, scale, line))
            .fold(0.0f32, f32::max)
            .ceil() as i64;
        let line_count = text.lines().count().max(1) as f32;
        let height = (ascent + (line_count - 1.0) * line_height).ceil() as i64;
        (width.max(1), height.max(1))
    } else {
        let line_count = text.lines().count().max(1) as i64;
        let longest_line = text
            .lines()
            .map(|line| line.chars().count())
            .max()
            .unwrap_or(0) as i64;
        // Keep a deterministic fallback for projects without a bundled font.
        (
            (longest_line * size * 3 / 5).max(1),
            (line_count * (size + 4)).max(1),
        )
    };
    new_rect(vm, 0, 0, width, height)
}
fn bitmap_max_size(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::Int(16_384))
}

fn object_instance(vm: &mut Vm, receiver: Value) -> Result<ObjId, VmError> {
    receiver
        .obj()
        .ok_or_else(|| vm.raise_type("receiver is not an object"))
}

fn rect_set_values(vm: &mut Vm, object: ObjId, args: &[Value]) -> Result<(), VmError> {
    let values = [
        int_arg(vm, args, 0)?,
        int_arg(vm, args, 1)?,
        int_arg(vm, args, 2)?,
        int_arg(vm, args, 3)?,
    ];
    for (name, value) in ["@x", "@y", "@width", "@height"].into_iter().zip(values) {
        vm.ivar_set(object, name, Value::Int(value));
    }
    Ok(())
}

fn rect_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let defaults = [Value::Int(0); 4];
    let values = if args.len() >= 4 { args } else { &defaults };
    rect_set_values(vm, object, values)?;
    Ok(Value::Nil)
}

fn rect_component(vm: &mut Vm, receiver: Value, name: &str) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, name))
}

fn rect_x(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    rect_component(vm, receiver, "@x")
}

fn rect_y(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    rect_component(vm, receiver, "@y")
}

fn rect_width(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    rect_component(vm, receiver, "@width")
}

fn rect_height(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    rect_component(vm, receiver, "@height")
}

fn rect_set(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    rect_set_values(vm, object, args)?;
    Ok(Value::Nil)
}

fn rect_empty(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let width = rect_component(vm, receiver, "@width")?;
    let height = rect_component(vm, receiver, "@height")?;
    let empty = matches!(width, Value::Int(value) if value <= 0)
        || matches!(height, Value::Int(value) if value <= 0);
    Ok(if empty { Value::True } else { Value::False })
}

fn new_rect(vm: &mut Vm, x: i64, y: i64, width: i64, height: i64) -> Result<Value, VmError> {
    let class = vm
        .const_path_get("Rect")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Rect class is not installed"))?;
    let object = vm.object_new(class);
    let object_id = object_instance(vm, object)?;
    for (name, value) in [("@x", x), ("@y", y), ("@width", width), ("@height", height)] {
        vm.ivar_set(object_id, name, Value::Int(value));
    }
    Ok(object)
}

fn viewport_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let rect = match args {
        [Value::Obj(rect), ..] => Value::Obj(*rect),
        [_, _, _, ..] => {
            let x = int_arg(vm, args, 0)?;
            let y = int_arg(vm, args, 1)?;
            let width = int_arg(vm, args, 2)?;
            let height = int_arg(vm, args, 3)?;
            new_rect(vm, x, y, width, height)?
        }
        _ => {
            let (width, height) = {
                let state = state_mut(vm);
                (i64::from(state.width), i64::from(state.height))
            };
            new_rect(vm, 0, 0, width, height)?
        }
    };
    vm.ivar_set(object, "@rect", rect);
    let color = new_color(vm, 255, 255, 255, 0)?;
    let tone = new_tone(vm, 0, 0, 0, 0)?;
    vm.ivar_set(object, "@color", color);
    vm.ivar_set(object, "@tone", tone);
    vm.ivar_set(object, "@z", Value::Int(0));
    vm.ivar_set(object, "@visible", Value::True);
    vm.ivar_set(object, "@disposed", Value::False);
    Ok(Value::Nil)
}

fn viewport_rect(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, "@rect"))
}

fn window_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let values = [
        args.first().copied().unwrap_or(Value::Int(0)),
        args.get(1).copied().unwrap_or(Value::Int(0)),
        args.get(2).copied().unwrap_or(Value::Int(0)),
        args.get(3).copied().unwrap_or(Value::Int(0)),
    ];
    for (name, value) in ["@x", "@y", "@width", "@height"].into_iter().zip(values) {
        vm.ivar_set(object, name, value);
    }
    vm.ivar_set(object, "@visible", Value::True);
    vm.ivar_set(object, "@active", Value::False);
    vm.ivar_set(object, "@opacity", Value::Int(255));
    vm.ivar_set(object, "@blend_type", Value::Int(0));
    vm.ivar_set(object, "@back_opacity", Value::Int(255));
    vm.ivar_set(object, "@contents_opacity", Value::Int(255));
    let color = new_color(vm, 255, 255, 255, 255)?;
    let tone = new_tone(vm, 0, 0, 0, 0)?;
    let cursor_rect = new_rect(vm, 0, 0, 0, 0)?;
    vm.ivar_set(object, "@color", color);
    vm.ivar_set(object, "@tone", tone);
    vm.ivar_set(object, "@cursor_rect", cursor_rect);
    vm.ivar_set(object, "@disposed", Value::False);
    Ok(Value::Nil)
}

fn register_renderable(vm: &mut Vm, receiver: Value) -> Result<(), VmError> {
    let Some(sprite_class) = vm.const_path_get("Sprite").and_then(Value::obj) else {
        return Ok(());
    };
    let instances = vm.ivar_get(sprite_class, "@__sabiruby_instances");
    let instances = if instances.is_nil() {
        let instances = vm.ary_new(Vec::new());
        vm.ivar_set(sprite_class, "@__sabiruby_instances", instances);
        instances
    } else {
        instances
    };
    vm.ary_push(instances, receiver)
}

fn plane_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    register_renderable(vm, receiver)?;
    let object = object_instance(vm, receiver)?;
    vm.ivar_set(
        object,
        "@viewport",
        args.first().copied().unwrap_or(Value::Nil),
    );
    vm.ivar_set(object, "@__sabiruby_plane", Value::True);
    vm.ivar_set(object, "@bitmap", Value::Nil);
    let color = new_color(vm, 255, 255, 255, 0)?;
    let tone = new_tone(vm, 0, 0, 0, 0)?;
    vm.ivar_set(object, "@color", color);
    vm.ivar_set(object, "@tone", tone);
    vm.ivar_set(object, "@x", Value::Int(0));
    vm.ivar_set(object, "@y", Value::Int(0));
    vm.ivar_set(object, "@z", Value::Int(0));
    vm.ivar_set(object, "@ox", Value::Int(0));
    vm.ivar_set(object, "@oy", Value::Int(0));
    vm.ivar_set(object, "@zoom_x", Value::Float(1.0));
    vm.ivar_set(object, "@zoom_y", Value::Float(1.0));
    vm.ivar_set(object, "@opacity", Value::Int(255));
    vm.ivar_set(object, "@visible", Value::True);
    vm.ivar_set(object, "@disposed", Value::False);
    Ok(Value::Nil)
}

fn color_set_values(vm: &mut Vm, object: ObjId, args: &[Value]) -> Result<(), VmError> {
    let defaults = [Value::Int(0), Value::Int(0), Value::Int(0), Value::Int(255)];
    let values = [
        args.first().copied().unwrap_or(defaults[0]),
        args.get(1).copied().unwrap_or(defaults[1]),
        args.get(2).copied().unwrap_or(defaults[2]),
        args.get(3).copied().unwrap_or(defaults[3]),
    ];
    for (name, value) in ["@red", "@green", "@blue", "@alpha"]
        .into_iter()
        .zip(values)
    {
        let component = vm.expect_int(value, "color component")?;
        vm.ivar_set(object, name, Value::Int(component));
    }
    Ok(())
}

fn color_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    color_set_values(vm, object, args)?;
    Ok(Value::Nil)
}

fn color_component(vm: &mut Vm, receiver: Value, name: &str) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, name))
}

fn color_red(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    color_component(vm, receiver, "@red")
}

fn color_green(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    color_component(vm, receiver, "@green")
}

fn color_blue(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    color_component(vm, receiver, "@blue")
}

fn color_alpha(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    color_component(vm, receiver, "@alpha")
}

fn color_set(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    color_set_values(vm, object, args)?;
    Ok(Value::Nil)
}

fn new_color(vm: &mut Vm, red: i64, green: i64, blue: i64, alpha: i64) -> Result<Value, VmError> {
    let class = vm
        .const_path_get("Color")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Color class is not installed"))?;
    let object = vm.object_new(class);
    let object_id = object_instance(vm, object)?;
    for (name, value) in [
        ("@red", red),
        ("@green", green),
        ("@blue", blue),
        ("@alpha", alpha),
    ] {
        vm.ivar_set(object_id, name, Value::Int(value));
    }
    Ok(object)
}

fn color_black(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    new_color(vm, 0, 0, 0, 255)
}

fn color_white(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    new_color(vm, 255, 255, 255, 255)
}

fn tone_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let values = [
        args.first().copied().unwrap_or(Value::Int(0)),
        args.get(1).copied().unwrap_or(Value::Int(0)),
        args.get(2).copied().unwrap_or(Value::Int(0)),
        args.get(3).copied().unwrap_or(Value::Int(0)),
    ];
    for (name, value) in ["@red", "@green", "@blue", "@gray"].into_iter().zip(values) {
        let component = vm.expect_int(value, "tone component")?;
        vm.ivar_set(object, name, Value::Int(component));
    }
    Ok(Value::Nil)
}

fn tone_component(vm: &mut Vm, receiver: Value, name: &str) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, name))
}

fn tone_red(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    tone_component(vm, receiver, "@red")
}

fn tone_green(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    tone_component(vm, receiver, "@green")
}

fn tone_blue(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    tone_component(vm, receiver, "@blue")
}

fn tone_gray(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    tone_component(vm, receiver, "@gray")
}

fn new_tone(vm: &mut Vm, red: i64, green: i64, blue: i64, gray: i64) -> Result<Value, VmError> {
    let class = vm
        .const_path_get("Tone")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Tone class is not installed"))?;
    let object = vm.object_new(class);
    let object_id = object_instance(vm, object)?;
    for (name, value) in [
        ("@red", red),
        ("@green", green),
        ("@blue", blue),
        ("@gray", gray),
    ] {
        vm.ivar_set(object_id, name, Value::Int(value));
    }
    Ok(object)
}

fn new_font(vm: &mut Vm) -> Result<Value, VmError> {
    let class = vm
        .const_path_get("Font")
        .and_then(Value::obj)
        .ok_or_else(|| vm.raise_type("Font class is not installed"))?;
    let object = vm.object_new(class);
    let object_id = object_instance(vm, object)?;
    let name = match vm.ivar_get(class, "@default_name") {
        Value::Nil => vm.str_new(b"Arial"),
        value => value,
    };
    vm.ivar_set(object_id, "@name", name);
    vm.ivar_set(
        object_id,
        "@size",
        match vm.ivar_get(class, "@default_size") {
            Value::Nil => Value::Int(22),
            value => value,
        },
    );
    vm.ivar_set(
        object_id,
        "@bold",
        match vm.ivar_get(class, "@default_bold") {
            Value::Nil => Value::False,
            value => value,
        },
    );
    vm.ivar_set(
        object_id,
        "@italic",
        match vm.ivar_get(class, "@default_italic") {
            Value::Nil => Value::False,
            value => value,
        },
    );
    vm.ivar_set(
        object_id,
        "@outline",
        match vm.ivar_get(class, "@default_outline") {
            Value::Nil => Value::False,
            value => value,
        },
    );
    vm.ivar_set(
        object_id,
        "@shadow",
        match vm.ivar_get(class, "@default_shadow") {
            Value::Nil => Value::False,
            value => value,
        },
    );
    let color = new_color(vm, 255, 255, 255, 255)?;
    let out_color = new_color(vm, 0, 0, 0, 128)?;
    vm.ivar_set(object_id, "@color", color);
    vm.ivar_set(object_id, "@out_color", out_color);
    Ok(object)
}

fn font_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let defaults = new_font(vm)?;
    let default_object = object_instance(vm, defaults)?;
    for name in [
        "@name",
        "@size",
        "@bold",
        "@italic",
        "@outline",
        "@shadow",
        "@color",
        "@out_color",
    ] {
        vm.ivar_set(object, name, vm.ivar_get(default_object, name));
    }
    if let Some(name) = args.first().copied() {
        vm.ivar_set(object, "@name", name);
    }
    if let Some(size) = args.get(1).copied() {
        let size = vm.expect_int(size, "font size")?;
        vm.ivar_set(object, "@size", Value::Int(size));
    }
    Ok(Value::Nil)
}

fn font_component(vm: &mut Vm, receiver: Value, name: &str) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, name))
}

fn font_name(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@name")
}

fn font_size(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@size")
}

fn font_bold(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@bold")
}

fn font_italic(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@italic")
}

fn font_outline(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@outline")
}

fn font_shadow(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@shadow")
}

fn font_color(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@color")
}

fn font_out_color(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@out_color")
}

fn fontdue_font(vm: &mut Vm, font: Value) -> Option<DueFont> {
    let bytes = font_asset_bytes(vm, font)?;
    DueFont::from_bytes(bytes, FontSettings::default()).ok()
}

fn line_width(font: &DueFont, scale: f32, text: &str) -> f32 {
    text.chars()
        .map(|character| font.metrics(character, scale).advance_width)
        .sum()
}

fn font_asset_bytes(vm: &mut Vm, font: Value) -> Option<Vec<u8>> {
    let name = font
        .obj()
        .map(|object| vm.ivar_get(object, "@name"))
        .unwrap_or(Value::Nil);
    let names = if let Some(values) = vm.ary_vals(name) {
        values
            .into_iter()
            .filter_map(|value| vm.str_bytes(value))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .collect::<Vec<_>>()
    } else {
        vm.str_bytes(name)
            .map(|bytes| vec![String::from_utf8_lossy(bytes).into_owned()])
            .unwrap_or_default()
    };
    let vfs = state_mut(vm).vfs.clone();
    for name in names {
        let name = name.trim();
        if name.is_empty() {
            continue;
        }
        let requested = if name.contains('/') || name.contains('\\') {
            name.replace('\\', "/")
        } else {
            format!("Fonts/{name}")
        };
        if let Some(bytes) = read_asset_bytes(vfs.as_ref(), &requested, &["ttf", "otf"]) {
            return Some(bytes);
        }
    }
    // Essentials projects commonly set their system font in Ruby.  Keep the
    // same project-font fallback used by the desktop player when a script asks
    // for a host font such as Arial that is not bundled in the game.
    read_asset_bytes(vfs.as_ref(), "Fonts/power green", &["ttf", "otf"])
}

fn bitmap_font(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let font = vm.ivar_get(object, "@font");
    if !matches!(font, Value::Nil) {
        return Ok(font);
    }
    let font = new_font(vm)?;
    vm.ivar_set(object, "@font", font);
    Ok(font)
}

fn font_default_color(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    new_color(vm, 255, 255, 255, 255)
}

fn font_exist(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::True)
}

fn sprite_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    // Keep the registry in a Ruby class ivar so the GC sees the same strong
    // references as Ruby code.  Storing raw ObjIds only in host state would
    // leave stale handles after a collection.
    register_renderable(vm, receiver)?;
    let viewport = args.first().copied().unwrap_or(Value::Nil);
    vm.ivar_set(object, "@viewport", viewport);
    vm.ivar_set(object, "@bitmap", Value::Nil);
    let src_rect = new_rect(vm, 0, 0, 0, 0)?;
    vm.ivar_set(object, "@src_rect", src_rect);
    let color = new_color(vm, 255, 255, 255, 0)?;
    let tone = new_tone(vm, 0, 0, 0, 0)?;
    vm.ivar_set(object, "@color", color);
    vm.ivar_set(object, "@tone", tone);
    vm.ivar_set(object, "@x", Value::Int(0));
    vm.ivar_set(object, "@y", Value::Int(0));
    vm.ivar_set(object, "@z", Value::Int(0));
    vm.ivar_set(object, "@ox", Value::Int(0));
    vm.ivar_set(object, "@oy", Value::Int(0));
    vm.ivar_set(object, "@zoom_x", Value::Float(1.0));
    vm.ivar_set(object, "@zoom_y", Value::Float(1.0));
    vm.ivar_set(object, "@opacity", Value::Int(255));
    vm.ivar_set(object, "@visible", Value::True);
    vm.ivar_set(object, "@angle", Value::Float(0.0));
    vm.ivar_set(object, "@mirror", Value::False);
    vm.ivar_set(object, "@disposed", Value::False);
    Ok(Value::Nil)
}

fn sprite_bitmap(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, "@bitmap"))
}

fn sprite_bitmap_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let bitmap = args.first().copied().unwrap_or(Value::Nil);
    if vm.ivar_get(object, "@bitmap") == bitmap {
        return Ok(Value::Nil);
    }
    vm.ivar_set(object, "@bitmap", bitmap);
    let (width, height) = if let Value::Obj(bitmap_object) = bitmap {
        let width_value = vm.ivar_get(bitmap_object, "@width");
        let height_value = vm.ivar_get(bitmap_object, "@height");
        let width = vm.expect_int(width_value, "bitmap width")?;
        let height = vm.expect_int(height_value, "bitmap height")?;
        (width, height)
    } else {
        (0, 0)
    };
    // `src_rect` is a mutable Ruby object.  Mutate that object instead of
    // replacing it so callers which retained a reference (IconSprite does
    // this while swapping an AnimatedBitmap) observe the new full-bitmap
    // rectangle just like RGSS.
    let src_rect = vm.ivar_get(object, "@src_rect");
    if let Some(src_rect) = src_rect.obj() {
        set_rect_values(vm, src_rect, 0, 0, width, height);
    } else {
        let src_rect = new_rect(vm, 0, 0, width, height)?;
        vm.ivar_set(object, "@src_rect", src_rect);
    }
    Ok(Value::Nil)
}

fn sprite_src_rect(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, "@src_rect"))
}

fn sprite_src_rect_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let source = args.first().copied().unwrap_or(Value::Nil);
    let Some(source) = source.obj() else {
        return Ok(Value::Nil);
    };
    let x = value_i64(vm.ivar_get(source, "@x"), 0);
    let y = value_i64(vm.ivar_get(source, "@y"), 0);
    let width = value_i64(vm.ivar_get(source, "@width"), 0);
    let height = value_i64(vm.ivar_get(source, "@height"), 0);
    let target = vm.ivar_get(object, "@src_rect");
    if let Some(target) = target.obj() {
        set_rect_values(vm, target, x, y, width, height);
    } else {
        let target = new_rect(vm, x, y, width, height)?;
        vm.ivar_set(object, "@src_rect", target);
    }
    Ok(Value::Nil)
}

fn set_rect_values(vm: &mut Vm, object: ObjId, x: i64, y: i64, width: i64, height: i64) {
    vm.ivar_set(object, "@x", Value::Int(x));
    vm.ivar_set(object, "@y", Value::Int(y));
    vm.ivar_set(object, "@width", Value::Int(width));
    vm.ivar_set(object, "@height", Value::Int(height));
}

fn sprite_width(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let bitmap = vm.ivar_get(object, "@bitmap");
    Ok(Value::Int(
        bitmap
            .obj()
            .map(|bitmap| value_i64(vm.ivar_get(bitmap, "@width"), 0))
            .unwrap_or(0),
    ))
}

fn sprite_height(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let bitmap = vm.ivar_get(object, "@bitmap");
    Ok(Value::Int(
        bitmap
            .obj()
            .map(|bitmap| value_i64(vm.ivar_get(bitmap, "@height"), 0))
            .unwrap_or(0),
    ))
}

fn object_dispose(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    vm.ivar_set(object, "@disposed", Value::True);
    vm.ivar_set(object, "@visible", Value::False);
    Ok(Value::Nil)
}

fn object_disposed(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(Value::bool(vm.ivar_get(object, "@disposed").truthy()))
}

fn install_module(
    vm: &mut Vm,
    name: &str,
    methods: &[(&str, sabiruby::object::NativeFn)],
) -> ObjId {
    let module = vm.define_module(name);
    for &(method, function) in methods {
        vm.define_module_function(module, method, function)
            .expect("RGSS module function");
    }
    module
}

fn install_class(vm: &mut Vm, name: &str) -> ObjId {
    install_subclass(vm, name, vm.core.object)
}

fn install_subclass(vm: &mut Vm, name: &str, superclass: ObjId) -> ObjId {
    let class = vm.define_class(name, superclass);
    vm.define_method(class, "initialize", noop);
    vm.define_method(class, "dispose", noop);
    vm.define_method(class, "disposed?", false_value);
    class
}

fn install_class_function(
    vm: &mut Vm,
    class: ObjId,
    name: &str,
    function: sabiruby::object::NativeFn,
) {
    let singleton = vm
        .singleton_class(Value::Obj(class))
        .expect("RGSS class singleton");
    vm.define_method(singleton, name, function);
}

/// Install the small service surface needed by the first headless boot pass.
/// Methods intentionally return deterministic values until their real backend is
/// connected; missing behavior remains visible through the compatibility report.
pub fn install_rgss(vm: &mut Vm, project: &Project) {
    install_rgss_with_vfs(vm, project, Arc::new(OsVfs::new(project.root.clone())));
}

/// Install RGSS bindings using an embedding-selected filesystem.
pub fn install_rgss_with_vfs(vm: &mut Vm, project: &Project, vfs: Arc<dyn VirtualFileSystem>) {
    vm.set_allow_self_superclass_reopen(true);
    vm.set_host_state(RgssState::new(project, vfs));
    let graphics = install_module(
        vm,
        "Graphics",
        &[
            ("width", graphics_width),
            ("height", graphics_height),
            ("frame_rate", graphics_frame_rate),
            ("frame_rate=", graphics_frame_rate),
            ("frame_count", graphics_frame_count),
            ("frame_count=", graphics_frame_count),
            ("update", graphics_update),
            ("resize_screen", graphics_resize),
            ("fullscreen", graphics_fullscreen),
            ("fullscreen=", graphics_fullscreen),
            ("scale", graphics_scale),
            ("scale=", graphics_scale),
            ("delta", graphics_delta),
            ("transition", noop),
            ("freeze", noop),
            ("frame_reset", noop),
            ("center", noop),
            ("snap_to_bitmap", graphics_snap_to_bitmap),
            ("average_frame_rate", graphics_frame_rate),
        ],
    );
    let _ = graphics;
    let system = install_module(
        vm,
        "System",
        &[
            ("uptime", system_uptime),
            ("real_uptime", system_uptime),
            ("unscaled_uptime", system_uptime),
            ("data_directory", system_data_directory),
            ("set_window_title", noop),
            ("game_title", noop),
            ("user_language", system_user_language),
            ("user_name", noop),
            ("power_state", system_power_state),
        ],
    );
    let _ = system;
    let input = install_module(
        vm,
        "Input",
        &[
            ("update", input_update),
            ("trigger?", input_trigger),
            ("repeat?", input_repeat),
            ("press?", input_press),
            ("release?", input_release),
            ("triggerex?", input_trigger),
            ("pressex?", input_press),
            ("repeatex?", input_repeat),
            ("dir4", input_dir4),
            ("dir8", input_dir8),
            ("text_input", false_value),
            ("mouse_x", zero_value),
            ("mouse_y", zero_value),
            ("mouse_in_window?", true_value),
            ("clipboard", noop),
        ],
    );
    let _ = input;
    let audio = install_module(
        vm,
        "Audio",
        &[
            ("bgm_play", bgm_play),
            ("bgm_stop", bgm_stop),
            ("bgm_fade", noop),
            ("bgm_pos", zero_value),
            ("bgs_play", bgs_play),
            ("bgs_stop", bgs_stop),
            ("bgs_fade", noop),
            ("me_play", me_play),
            ("me_stop", me_stop),
            ("me_fade", noop),
            ("se_play", se_play),
            ("se_stop", se_stop),
        ],
    );
    let _ = audio;
    vm.define_method(vm.core.numeric, "round", numeric_round);
    for class in [
        "Bitmap", "Sprite", "Viewport", "Plane", "Window", "Color", "Tone", "Rect", "Table", "Font",
    ] {
        let class_id = install_class(vm, class);
        if matches!(class, "Sprite" | "Viewport" | "Plane" | "Window") {
            vm.define_method(class_id, "update", noop);
            vm.define_method(class_id, "refresh", noop);
        }
        if class == "Viewport" {
            vm.define_method(class_id, "initialize", viewport_initialize);
            vm.define_method(class_id, "rect", viewport_rect);
        }
        if class == "Plane" {
            vm.define_method(class_id, "initialize", plane_initialize);
            vm.define_method(class_id, "dispose", object_dispose);
            vm.define_method(class_id, "disposed?", object_disposed);
        }
        if class == "Window" {
            vm.define_method(class_id, "initialize", window_initialize);
            vm.define_method(class_id, "dispose", object_dispose);
            vm.define_method(class_id, "disposed?", object_disposed);
        }
        if class == "Sprite" {
            vm.define_method(class_id, "initialize", sprite_initialize);
            vm.define_method(class_id, "bitmap", sprite_bitmap);
            vm.define_method(class_id, "bitmap=", sprite_bitmap_set);
            vm.define_method(class_id, "src_rect", sprite_src_rect);
            vm.define_method(class_id, "src_rect=", sprite_src_rect_set);
            vm.define_method(class_id, "width", sprite_width);
            vm.define_method(class_id, "height", sprite_height);
            vm.define_method(class_id, "dispose", object_dispose);
            vm.define_method(class_id, "disposed?", object_disposed);
        }
        if class == "Rect" {
            vm.define_method(class_id, "initialize", rect_initialize);
            vm.define_method(class_id, "x", rect_x);
            vm.define_method(class_id, "y", rect_y);
            vm.define_method(class_id, "width", rect_width);
            vm.define_method(class_id, "height", rect_height);
            vm.define_method(class_id, "set", rect_set);
            vm.define_method(class_id, "empty?", rect_empty);
        }
        if class == "Color" {
            vm.define_method(class_id, "initialize", color_initialize);
            vm.define_method(class_id, "red", color_red);
            vm.define_method(class_id, "green", color_green);
            vm.define_method(class_id, "blue", color_blue);
            vm.define_method(class_id, "alpha", color_alpha);
            vm.define_method(class_id, "set", color_set);
            install_class_function(vm, class_id, "black", color_black);
            install_class_function(vm, class_id, "white", color_white);
        }
        if class == "Tone" {
            vm.define_method(class_id, "initialize", tone_initialize);
            vm.define_method(class_id, "red", tone_red);
            vm.define_method(class_id, "green", tone_green);
            vm.define_method(class_id, "blue", tone_blue);
            vm.define_method(class_id, "gray", tone_gray);
        }
        if class == "Font" {
            vm.define_method(class_id, "initialize", font_initialize);
            vm.define_method(class_id, "name", font_name);
            vm.define_method(class_id, "size", font_size);
            vm.define_method(class_id, "bold", font_bold);
            vm.define_method(class_id, "italic", font_italic);
            vm.define_method(class_id, "outline", font_outline);
            vm.define_method(class_id, "shadow", font_shadow);
            vm.define_method(class_id, "color", font_color);
            vm.define_method(class_id, "out_color", font_out_color);
            install_class_function(vm, class_id, "default_color", font_default_color);
            install_class_function(vm, class_id, "exist?", font_exist);
        }
        if class == "Bitmap" {
            vm.define_method(class_id, "initialize", bitmap_initialize);
            vm.define_method(class_id, "dispose", object_dispose);
            vm.define_method(class_id, "disposed?", object_disposed);
            vm.define_method(class_id, "width", bitmap_width);
            vm.define_method(class_id, "height", bitmap_height);
            vm.define_method(class_id, "animated?", bitmap_animated);
            vm.define_method(class_id, "frame_count", bitmap_frame_count);
            vm.define_method(class_id, "frame_rate", bitmap_frame_rate);
            vm.define_method(class_id, "frame_rate=", bitmap_frame_rate_set);
            vm.define_method(class_id, "play", bitmap_play);
            vm.define_method(class_id, "stop", bitmap_stop);
            vm.define_method(class_id, "update", bitmap_update);
            vm.define_method(class_id, "current_frame", bitmap_current_frame);
            vm.define_method(class_id, "playing?", bitmap_playing);
            vm.define_method(class_id, "playing=", bitmap_playing_set);
            vm.define_method(class_id, "goto_and_play", bitmap_goto_and_play);
            vm.define_method(class_id, "goto_and_stop", bitmap_goto_and_stop);
            vm.define_method(class_id, "font", bitmap_font);
            vm.define_method(class_id, "rect", bitmap_rect);
            vm.define_method(class_id, "draw_text", bitmap_draw_text);
            vm.define_method(class_id, "fill_rect", bitmap_fill_rect);
            vm.define_method(class_id, "clear", bitmap_clear);
            vm.define_method(class_id, "clear_rect", bitmap_fill_rect);
            vm.define_method(class_id, "set_pixel", bitmap_set_pixel);
            vm.define_method(class_id, "get_pixel", bitmap_get_pixel);
            vm.define_method(class_id, "blt", bitmap_blt);
            vm.define_method(class_id, "stretch_blt", bitmap_stretch_blt);
            vm.define_method(class_id, "text_size", bitmap_text_size);
            install_class_function(vm, class_id, "max_size", bitmap_max_size);
        }
    }
    let io = install_class(vm, "IO");
    let file = install_subclass(vm, "File", io);
    vm.define_method(file, "initialize", file_initialize);
    for (name, function) in [
        ("read", file_read as sabiruby::object::NativeFn),
        ("eof?", file_eof),
        ("pos", file_pos),
        ("pos=", file_pos),
        ("seek", file_pos),
        ("write", file_write),
        ("binmode", file_noop),
        ("close", file_noop),
    ] {
        vm.define_method(io, name, function);
    }
    for (name, function) in [
        ("directory?", file_directory as sabiruby::object::NativeFn),
        ("file?", file_file),
        ("exist?", file_exist),
        ("join", file_join),
        ("basename", file_basename),
        ("dirname", file_dirname),
        ("open", file_open),
        ("read", file_class_read),
        ("readlines", file_class_readlines),
        ("foreach", file_class_foreach),
        ("write", file_class_write),
        ("delete", file_delete),
        ("unlink", file_delete),
        ("copy", file_copy),
        ("move", file_move),
    ] {
        install_class_function(vm, file, name, function);
    }
    let file_test = install_module(vm, "FileTest", &[]);
    for (name, function) in [
        ("directory?", file_directory as sabiruby::object::NativeFn),
        ("file?", file_file),
        ("exist?", file_exist),
    ] {
        vm.define_module_function(file_test, name, function)
            .expect("FileTest function");
    }
    let dir = install_class(vm, "Dir");
    for (name, function) in [
        ("entries", dir_entries as sabiruby::object::NativeFn),
        ("children", dir_children),
        ("foreach", dir_foreach),
        ("glob", dir_glob),
        ("mkdir", dir_mkdir),
        ("delete", dir_delete),
        ("rmdir", dir_delete),
        ("chdir", dir_chdir),
    ] {
        install_class_function(vm, dir, name, function);
    }
    install_class_function(vm, dir, "pwd", dir_pwd);
    vm.define_module_function(vm.core.kernel, "__rgss_plugin_scripts", rgss_plugin_scripts)
        .expect("RGSS plugin archive loader");
    let loader = vm.intern("__rgss_plugin_scripts");
    vm.set_visibility(vm.core.kernel, loader, sabiruby::object::Vis::Private)
        .expect("private RGSS plugin archive loader");
    vm.define_module_function(vm.core.kernel, "__rgss_load_data", rgss_load_data)
        .expect("RGSS Marshal data loader");
    let data_loader = vm.intern("__rgss_load_data");
    vm.set_visibility(vm.core.kernel, data_loader, sabiruby::object::Vis::Private)
        .expect("private RGSS Marshal data loader");
}

/// Runtime driver. It is deliberately usable without a window or GPU.
pub struct RgssRuntime {
    pub project: Project,
    pub vm: Vm,
}

impl RgssRuntime {
    pub fn open(project: Project) -> Result<Self, RgssError> {
        let vfs = Arc::new(OsVfs::new(project.root.clone()));
        Self::open_with_vfs(project, vfs)
    }

    /// Open a project with an embedding-selected filesystem implementation.
    /// The same object is used by compiler `require`/`load` calls and by the
    /// native Ruby `File` bindings, so an archive or memory VFS has no host
    /// filesystem escape hatch.
    pub fn open_with_vfs(
        project: Project,
        vfs: Arc<dyn VirtualFileSystem>,
    ) -> Result<Self, RgssError> {
        let mut vm = Vm::with_mrblib()?;
        vm.set_load_path(&["Data/Scripts", "Plugins", "."]);
        vm.set_host(Box::new(ProjectHost::with_vfs(&project, vfs.clone())));
        install_rgss_with_vfs(&mut vm, &project, vfs);
        let mut runtime = Self { project, vm };
        runtime.run_source("rgss/compatibility_prelude.rb", COMPATIBILITY_PRELUDE)?;
        Ok(runtime)
    }

    pub fn run_source(&mut self, filename: &str, source: &[u8]) -> Result<Value, RgssError> {
        let bytecode = self.compile_source(filename, source)?;
        self.vm
            .load_and_run(&bytecode)
            .map_err(|error| self.runtime_error(error))
    }

    fn compile_source(&mut self, filename: &str, source: &[u8]) -> Result<Vec<u8>, RgssError> {
        let options = Options {
            filename: filename.to_owned(),
            debug_info: true,
            ..Options::default()
        };
        compile(&rewrite_constant_path_operator_writes(source), &options)
            .map_err(|e| RgssError::Compile(e.to_string()))
    }

    fn runtime_error(&mut self, error: VmError) -> RgssError {
        let mut description = self.vm.describe_error(&error);
        if let VmError::Raise(Value::Obj(exception)) = error {
            let backtrace_sym = self.vm.intern("backtrace");
            let backtrace = self
                .vm
                .funcall(Value::Obj(exception), backtrace_sym, &[], Value::Nil)
                .ok()
                .and_then(|value| self.vm.ary_vals(value))
                .map(|frames| {
                    frames
                        .iter()
                        .filter_map(|frame| self.vm.str_bytes(*frame))
                        .map(|frame| String::from_utf8_lossy(frame).into_owned())
                        .collect::<Vec<_>>()
                })
                .filter(|frames| !frames.is_empty());
            if let Some(backtrace) = backtrace {
                description.push('\n');
                description.push_str(&backtrace.join("\n"));
            }
        }
        RgssError::Runtime(description)
    }

    /// Compile and start a Ruby program without running it to completion.
    ///
    /// This is the boundary used by a windowed host: setup scripts can be run
    /// with [`Self::run_source`], while the script containing the RGSS game
    /// loop remains suspended and is advanced by [`Self::step`].
    pub fn start_source(&mut self, filename: &str, source: &[u8]) -> Result<(), RgssError> {
        let bytecode = self.compile_source(filename, source)?;
        let irep = self.vm.load(&bytecode).map_err(RgssError::Vm)?;
        self.vm.start(irep);
        Ok(())
    }

    /// Advance a program started by [`Self::start_source`] by at most
    /// `instruction_budget` VM instructions.
    pub fn step(&mut self, instruction_budget: u64) -> Result<Step, RgssError> {
        self.vm
            .step(instruction_budget.max(1))
            .map_err(|error| self.runtime_error(error))
    }

    /// Number of completed RGSS frames observed by the core.
    pub fn frame_count(&self) -> u64 {
        self.vm
            .host_state::<RgssState>()
            .map_or(0, |state| state.frame_count)
    }

    /// Snapshot the Ruby sprite registry for an embedding renderer.  Ruby
    /// remains authoritative for every property; this is intentionally a
    /// read-only projection at the frame boundary.
    pub fn render_snapshot(&self) -> RenderSnapshot {
        let (width, height) = self
            .vm
            .host_state::<RgssState>()
            .map_or((512, 384), |state| (state.width, state.height));
        let sprites = self
            .vm
            .const_path_get("Sprite")
            .and_then(Value::obj)
            .map(|sprite_class| self.vm.ivar_get(sprite_class, "@__sabiruby_instances"))
            .and_then(|instances| self.vm.ary_vals(instances))
            .unwrap_or_default()
            .into_iter()
            .filter_map(Value::obj);
        let mut snapshot = Vec::new();
        for object in sprites {
            if self.vm.ivar_get(object, "@disposed").truthy() {
                continue;
            }
            let bitmap = self.vm.ivar_get(object, "@bitmap");
            let Some(bitmap) = bitmap.obj() else {
                continue;
            };
            if self.vm.ivar_get(bitmap, "@disposed").truthy() {
                continue;
            }
            let bitmap_width = value_i64(self.vm.ivar_get(bitmap, "@width"), 1).max(1) as u32;
            let bitmap_height = value_i64(self.vm.ivar_get(bitmap, "@height"), 1).max(1) as u32;
            let bitmap_data = self
                .vm
                .host_state::<RgssState>()
                .and_then(|state| state.bitmaps.get(&bitmap));
            let bitmap_path = self
                .vm
                .str_bytes(self.vm.ivar_get(bitmap, "@path"))
                .filter(|_| bitmap_data.is_none_or(|data| data.pixels.is_empty()))
                .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
            let src_rect = self.vm.ivar_get(object, "@src_rect").obj();
            let (src_x, src_y, src_width, src_height) = src_rect
                .map(|rect| {
                    (
                        value_i64(self.vm.ivar_get(rect, "@x"), 0),
                        value_i64(self.vm.ivar_get(rect, "@y"), 0),
                        value_i64(self.vm.ivar_get(rect, "@width"), bitmap_width as i64),
                        value_i64(self.vm.ivar_get(rect, "@height"), bitmap_height as i64),
                    )
                })
                .unwrap_or((0, 0, bitmap_width as i64, bitmap_height as i64));
            let viewport = self.vm.ivar_get(object, "@viewport").obj();
            if viewport.is_some_and(|viewport| !self.vm.ivar_get(viewport, "@visible").truthy()) {
                continue;
            }
            let mut color = color_components(
                &self.vm,
                self.vm.ivar_get(object, "@color"),
                [255, 255, 255, 0],
            )
            .map(|component| component as u8);
            let mut tone = tone_components(&self.vm, self.vm.ivar_get(object, "@tone"));
            if let Some(viewport) = viewport {
                let viewport_color = color_components(
                    &self.vm,
                    self.vm.ivar_get(viewport, "@color"),
                    [255, 255, 255, 0],
                )
                .map(|component| component as u8);
                let viewport_tone = tone_components(&self.vm, self.vm.ivar_get(viewport, "@tone"));
                color = overlay_colors(color, viewport_color);
                tone = add_tones(tone, viewport_tone);
            }
            snapshot.push(SpriteSnapshot {
                bitmap_id: bitmap.0,
                bitmap_path,
                bitmap_rgba: bitmap_data
                    .filter(|data| !data.pixels.is_empty())
                    .map(|data| Arc::from(data.pixels.clone().into_boxed_slice())),
                bitmap_width,
                bitmap_height,
                bitmap_revision: bitmap_data.map_or(0, |data| data.revision),
                src_x,
                src_y,
                src_width,
                src_height,
                x: value_i64(self.vm.ivar_get(object, "@x"), 0),
                y: value_i64(self.vm.ivar_get(object, "@y"), 0),
                ox: value_i64(self.vm.ivar_get(object, "@ox"), 0),
                oy: value_i64(self.vm.ivar_get(object, "@oy"), 0),
                zoom_x: value_f64(self.vm.ivar_get(object, "@zoom_x"), 1.0),
                zoom_y: value_f64(self.vm.ivar_get(object, "@zoom_y"), 1.0),
                opacity: value_i64(self.vm.ivar_get(object, "@opacity"), 255).clamp(0, 255) as u8,
                angle: value_f64(self.vm.ivar_get(object, "@angle"), 0.0),
                mirror: self.vm.ivar_get(object, "@mirror").truthy(),
                color,
                tone,
                blend_type: value_i64(self.vm.ivar_get(object, "@blend_type"), 0),
                visible: self.vm.ivar_get(object, "@visible").truthy(),
                z: value_i64(self.vm.ivar_get(object, "@z"), 0),
                tile: self.vm.ivar_get(object, "@__sabiruby_plane").truthy(),
            });
        }
        snapshot.sort_by_key(|sprite| sprite.z);
        RenderSnapshot {
            width,
            height,
            sprites: snapshot,
        }
    }

    /// Read a bitmap through the same VFS used by Ruby.  The extension probe
    /// mirrors RGSS's permissive `Bitmap.new("Graphics/foo")` lookup.
    pub fn read_asset(&self, path: &str) -> Option<Vec<u8>> {
        let vfs = self.vm.host_state::<RgssState>()?.vfs.clone();
        read_asset_bytes(
            vfs.as_ref(),
            path,
            &[
                "png", "gif", "jpg", "jpeg", "bmp", "ogg", "wav", "mp3", "mid", "midi",
            ],
        )
    }

    /// Queue a host keyboard/button press for the next Ruby `Input.update`.
    pub fn input_press(&mut self, code: i32) {
        if let Some(state) = self.vm.host_state_mut::<RgssState>() {
            state.input.press(code);
        }
    }

    /// Queue a host keyboard/button release for the next Ruby `Input.update`.
    pub fn input_release(&mut self, code: i32) {
        if let Some(state) = self.vm.host_state_mut::<RgssState>() {
            state.input.release(code);
        }
    }

    /// Start the last section in the project's compiled `Scripts.rxdata`
    /// archive for stepped execution after running the preceding sections
    /// normally. The archive order is the RGSS entrypoint; source files under
    /// `Data/Scripts` are not an alternate runtime entrypoint.
    pub fn start_scripts(&mut self, limit: Option<usize>) -> Result<usize, RgssError> {
        let vfs = self
            .vm
            .host_state::<RgssState>()
            .map(|state| state.vfs.clone())
            .ok_or_else(|| RgssError::Runtime("RGSS state is not installed".to_owned()))?;
        let scripts = read_script_archive(&mut self.vm, vfs.as_ref(), &self.project.scripts_asset)?;
        install_script_global(&mut self.vm, &scripts);
        let mut selected: Vec<ScriptArchiveEntry> = scripts
            .into_iter()
            .take(limit.unwrap_or(usize::MAX))
            .collect();
        let Some(entry) = selected.pop() else {
            return Ok(0);
        };
        let count = selected.len() + 1;
        for entry in selected {
            self.run_source(&entry.name, &entry.source)
                .map_err(|error| RgssError::Compile(format!("{}: {error}", entry.name)))?;
        }
        self.start_source(&entry.name, &entry.source)?;
        Ok(count)
    }

    /// Execute compiled `Scripts.rxdata` sections in archive order. The first
    /// error is returned with its script name; this is the headless boot probe.
    pub fn run_scripts(&mut self, limit: Option<usize>) -> Result<usize, RgssError> {
        let vfs = self
            .vm
            .host_state::<RgssState>()
            .map(|state| state.vfs.clone())
            .ok_or_else(|| RgssError::Runtime("RGSS state is not installed".to_owned()))?;
        let scripts = read_script_archive(&mut self.vm, vfs.as_ref(), &self.project.scripts_asset)?;
        install_script_global(&mut self.vm, &scripts);
        let mut count = 0;
        for entry in scripts.into_iter().take(limit.unwrap_or(usize::MAX)) {
            self.run_source(&entry.name, &entry.source)
                .map_err(|error| RgssError::Compile(format!("{}: {error}", entry.name)))?;
            count += 1;
        }
        Ok(count)
    }

    /// Compatibility name for embedders that used the initial source-tree
    /// probe. It still reads only the compiled archive.
    pub fn start_extracted_scripts(&mut self, limit: Option<usize>) -> Result<usize, RgssError> {
        self.start_scripts(limit)
    }

    /// Compatibility name for the old headless probe; the archive remains the
    /// only script entrypoint.
    pub fn run_extracted_scripts(&mut self, limit: Option<usize>) -> Result<usize, RgssError> {
        self.run_scripts(limit)
    }

    pub fn take_output(&mut self) -> Vec<u8> {
        self.vm.take_output()
    }

    /// Drain audio commands emitted by Ruby since the previous frame.  The
    /// core does not open devices; an embedding host maps these commands to
    /// CPAL, WebAudio, or another mixer.
    pub fn take_audio_commands(&mut self) -> Vec<AudioCommand> {
        self.vm
            .host_state_mut::<RgssState>()
            .map(|state| std::mem::take(&mut state.audio_commands))
            .unwrap_or_default()
    }

    /// Limit Graphics.update calls for a headless probe. Embedders normally
    /// leave this unset and drive frames from their own event loop.
    pub fn set_frame_limit(&mut self, limit: Option<u64>) {
        if let Some(state) = self.vm.host_state_mut::<RgssState>() {
            state.frame_limit = limit;
        }
    }
}

const COMPATIBILITY_PRELUDE: &[u8] = br#"
# Minimal Ruby-level compatibility expected before the game's own scripts load.
# The real renderer and font backend will replace these values; keeping them in
# Ruby makes the profile easy for a host to extend without changing the VM.
module Encoding
  class RGSSEncoding
    def initialize(name); @name = name; end
    def name; @name; end
    def to_s; @name; end
    def ==(other); other.equal?(self) || other == @name; end
    alias eql? ==
    def hash; @name.hash; end
  end unless const_defined?(:RGSSEncoding)
  UTF_8 = RGSSEncoding.new("UTF-8") unless const_defined?(:UTF_8)
  @default_internal = UTF_8
  @default_external = UTF_8
  def self.find(name)
    return UTF_8 if name == UTF_8 || name == "UTF-8" || name.to_s == "UTF-8"
    raise ArgumentError, "unknown encoding: #{name}"
  end
  def self.default_internal; @default_internal; end
  def self.default_internal=(value); @default_internal = value; end
  def self.default_external; @default_external; end
  def self.default_external=(value); @default_external = value; end
end
module System
  VERSION = "2.4.2/d13f35c" unless const_defined?(:VERSION)
end
class SystemExit < Exception; end unless defined?(SystemExit)
module Errno
  ENOENT = Class.new(StandardError) unless const_defined?(:ENOENT)
  EINVAL = Class.new(StandardError) unless const_defined?(:EINVAL)
  EACCES = Class.new(StandardError) unless const_defined?(:EACCES)
  EISDIR = Class.new(StandardError) unless const_defined?(:EISDIR)
end
module Input
  C = 1 unless const_defined?(:C)
  B = 2 unless const_defined?(:B)
  A = 3 unless const_defined?(:A)
  X = 4 unless const_defined?(:X)
  Y = 5 unless const_defined?(:Y)
  Z = 6 unless const_defined?(:Z)
  L = 7 unless const_defined?(:L)
  R = 8 unless const_defined?(:R)
  UP = 9 unless const_defined?(:UP)
  DOWN = 10 unless const_defined?(:DOWN)
  LEFT = 11 unless const_defined?(:LEFT)
  RIGHT = 12 unless const_defined?(:RIGHT)
  ALT = 13 unless const_defined?(:ALT)
  CTRL = 14 unless const_defined?(:CTRL)
  SHIFT = 15 unless const_defined?(:SHIFT)
  F8 = 16 unless const_defined?(:F8)
  F9 = 17 unless const_defined?(:F9)
  MOUSELEFT = 18 unless const_defined?(:MOUSELEFT)
  MOUSERIGHT = 19 unless const_defined?(:MOUSERIGHT)
  MOUSEMIDDLE = 20 unless const_defined?(:MOUSEMIDDLE)
  SHIFT = 15 unless const_defined?(:SHIFT)
  BACK = 21 unless const_defined?(:BACK)
  SPECIAL = 22 unless const_defined?(:SPECIAL)
  ACTION = 23 unless const_defined?(:ACTION)
  USE = 24 unless const_defined?(:USE)
  JUMPUP = 25 unless const_defined?(:JUMPUP)
  JUMPDOWN = 26 unless const_defined?(:JUMPDOWN)
  AUX1 = 27 unless const_defined?(:AUX1)
  AUX2 = 28 unless const_defined?(:AUX2)
end
class Font
  class << self
    attr_accessor :default_name, :default_size, :default_bold, :default_italic,
                  :default_outline, :default_shadow, :default_out_color
  end
  self.default_name = "Arial" unless defined?(@default_name)
  self.default_size = 22 unless defined?(@default_size)
  self.default_bold = false unless defined?(@default_bold)
  self.default_italic = false unless defined?(@default_italic)
  self.default_outline = false unless defined?(@default_outline)
  self.default_shadow = false unless defined?(@default_shadow)
  self.default_out_color = Color.new(0, 0, 0, 128) unless defined?(@default_out_color)
end
$: = [] unless $:
TOPLEVEL_BINDING = binding unless defined?(TOPLEVEL_BINDING)
class RGSSData
  def initialize(values = {}); @values = values; end
  def method_missing(name, *_args)
    ivar = "@#{name}"
    return instance_variable_get(ivar) if instance_variable_defined?(ivar)
    return @values[name] if @values.has_key?(name)
    nil
  end
end
module RPG
  # Essentials' RPG::Cache implementation is loaded later, but its first
  # `retain` call expects the backing hash to exist already (the desktop RGSS
  # host creates it during boot).
  module Cache
    @cache = {} unless defined?(@cache)
  end
  unless const_defined?(:AudioFile)
    class AudioFile < RGSSData
      attr_accessor :name, :volume, :pitch

      def initialize(name = "", volume = 100, pitch = 100)
        @name = name
        @volume = volume
        @pitch = pitch
      end
    end
  end
end
class Table
  def dimensions; @dimensions || 3; end
  def xsize; @xsize || 0; end
  def ysize; @ysize || 0; end
  def zsize; @zsize || 0; end
  def [](x, y = 0, z = 0)
    return 0 if x < 0 || y < 0 || z < 0 || x >= xsize || y >= ysize || z >= zsize
    @data[x + xsize * (y + ysize * z)] || 0
  end
  def []=(x, y, z, value = nil)
    if value.nil?
      value = z
      z = 0
    end
    return value if x < 0 || y < 0 || z < 0 || x >= xsize || y >= ysize || z >= zsize
    @data[x + xsize * (y + ysize * z)] = value
    value
  end
  def resize(x, y = 1, z = 1)
    @xsize = x
    @ysize = y
    @zsize = z
    @data = Array.new(x * y * z, 0)
    self
  end
  def clone
    copy = Table.new
    copy.instance_variable_set(:@dimensions, dimensions)
    copy.instance_variable_set(:@xsize, xsize)
    copy.instance_variable_set(:@ysize, ysize)
    copy.instance_variable_set(:@zsize, zsize)
    copy.instance_variable_set(:@data, @data.dup)
    copy
  end
end
# `load_data` is a Ruby-facing facade over the VFS-backed Marshal reader. The
# plugin archive remains a separate path because its records are consumed by
# Essentials' Ruby PluginManager before the normal data files are loaded.
def load_data(path)
  return __rgss_plugin_scripts if path.include?("PluginScripts")
  __rgss_load_data(path)
end
def save_data(_object, _path); nil; end
module Zlib
  module Inflate
    def self.inflate(value); value; end
  end
  module Deflate
    def self.deflate(value); value; end
  end
end
module HTTPLite
  module JSON
    class ParserError < StandardError; end
    def self.parse(_value); {}; end
  end
end
module Kernel
  alias __sabiruby_require require
  def require(name)
    return true if name == "socket" || name == "io/wait"
    __sabiruby_require(name)
  end
  def self.exit!(_status = true)
    raise SystemExit
  end
end
class RGSSThreadStub
  def status; nil; end
end
class Thread
  def self.new(*_args)
    yield if block_given?
    RGSSThreadStub.new
  end
  def self.current; self; end
  def self.exit; nil; end
end unless defined?(Thread)
module RGSSDynamicProperties
  def method_missing(name, *args)
    property = name.to_s
    if property.end_with?("=")
      field = "@#{property[0...-1]}"
      instance_variable_set(field, args[0])
      return args[0]
    end
    query = property.end_with?("?")
    property = property[0...-1] if query
    value = instance_variable_get("@#{property}")
    query ? !value.nil? && value != false : value
  end
end
class String
  def force_encoding(_encoding); self; end
  def encoding; Encoding::UTF_8; end
  alias __sabiruby_sub_bang sub!
  def sub!(pattern, replacement)
    return self if pattern.nil?
    __sabiruby_sub_bang(pattern, replacement)
  end
end
[Bitmap, Sprite, Viewport, Plane, Window, Color, Tone, Rect, Table, Font].each do |klass|
  klass.include(RGSSDynamicProperties)
end
"#;

#[cfg(test)]
mod tests {
    use super::*;

    struct MemoryVfs;

    impl VirtualFileSystem for MemoryVfs {
        fn read(&self, path: &str) -> std::io::Result<Vec<u8>> {
            match path {
                "Data/Scripts/virtual.rb" => Ok(b"from vfs".to_vec()),
                _ => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            }
        }

        fn write(&self, _: &str, bytes: &[u8], _: bool) -> std::io::Result<usize> {
            Ok(bytes.len())
        }

        fn metadata(&self, path: &str) -> std::io::Result<VfsMetadata> {
            match path {
                "." | "Data" | "Data/Scripts" => Ok(VfsMetadata {
                    len: 0,
                    is_file: false,
                    is_dir: true,
                }),
                "Data/Scripts/virtual.rb" => Ok(VfsMetadata {
                    len: 8,
                    is_file: true,
                    is_dir: false,
                }),
                _ => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            }
        }

        fn read_dir(&self, path: &str) -> std::io::Result<Vec<VfsDirEntry>> {
            match path {
                "Data/Scripts" => Ok(vec![VfsDirEntry {
                    name: "virtual.rb".to_owned(),
                    metadata: VfsMetadata {
                        len: 8,
                        is_file: true,
                        is_dir: false,
                    },
                }]),
                _ => Err(std::io::Error::from(std::io::ErrorKind::NotFound)),
            }
        }
    }

    #[test]
    fn parses_anil_project_metadata() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        assert_eq!(project.title, "Pokemon Anil");
        assert_eq!((project.screen_width, project.screen_height), (512, 384));
        assert!(project.scripts_path.ends_with("Data/Scripts.rxdata"));
        assert_eq!(project.scripts_asset, "Data/Scripts.rxdata");
        assert!(project.extracted_scripts().unwrap().len() > 300);
    }

    #[test]
    fn script_archive_is_the_runtime_entrypoint() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project.clone()).unwrap();
        let vfs = OsVfs::new(project.root());
        let scripts = read_script_archive(&mut runtime.vm, &vfs, &project.scripts_asset).unwrap();
        assert!(scripts.len() > 400);
        assert!(scripts.iter().any(|entry| entry.name.contains("Main")));
        assert!(
            scripts
                .iter()
                .any(|entry| entry.source.windows(5).any(|w| w == b"class"))
        );
    }

    #[test]
    fn rooted_host_compiles_and_reads_inside_project() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "smoke.rb",
                b"puts Graphics.width; puts Graphics.height; puts System.data_directory",
            )
            .unwrap();
        let output = String::from_utf8(runtime.take_output()).unwrap();
        assert!(output.contains("512\n384\n"));
    }

    #[test]
    fn system_power_state_has_a_safe_rgss_hash_shape() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "power-state.rb",
                br#"state = System.power_state; puts [state[:seconds], state[:percent], state[:discharging]].inspect"#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "[nil, nil, false]\n"
        );
    }

    #[test]
    fn string_encoding_and_graphics_snapshot_match_rgss_shape() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "encoding-and-snapshot.rb",
                br#"snapshot = Graphics.snap_to_bitmap; puts ["hola".encoding.name, snapshot.width, snapshot.height].join(","); snapshot.dispose"#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "UTF-8,512,384\n"
        );
    }

    #[test]
    fn bitmap_constructor_exposes_rgss_dimensions() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "bitmap.rb",
                br#"
                  a = Bitmap.new(7, 11)
                  b = Bitmap.new("Graphics/Animations/!.png")
                  puts [a.width, a.height, b.width > 0, b.height > 0].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "7,11,true,true\n"
        );
    }

    #[test]
    fn oak_intro_assets_resolve_through_rgss_asset_layer() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "oak-assets.rb",
                br#"
                  background = Bitmap.new("Graphics/Pictures/fondoIntro")
                  oak = Bitmap.new("Graphics/Pictures/oakIntro1")
                  sprite = Sprite.new
                  sprite.bitmap = oak
                  puts [background.width, background.height, oak.width, oak.height].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "512,384,512,384\n"
        );
        let snapshot = runtime.render_snapshot();
        let oak = snapshot
            .sprites
            .iter()
            .find(|sprite| sprite.bitmap_width == 512 && sprite.bitmap_height == 384)
            .expect("oak sprite should reach the render snapshot");
        assert!(
            oak.bitmap_rgba
                .as_ref()
                .is_some_and(|pixels| pixels.chunks_exact(4).any(|pixel| pixel[3] > 0))
        );
    }

    #[test]
    fn viewport_rect_and_color_values_match_rgss_shape() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "geometry.rb",
                br#"
                  viewport = Viewport.new(0, 0, 12, 34)
                  black = Color.black
                  tone = Tone.new(1, 2, 3, 4)
                  bitmap = Bitmap.new(1, 1)
                  bitmap.font.size = 18
                  measured = bitmap.text_size("Anil")
                  puts [viewport.rect.width, viewport.rect.height, black.red, black.alpha,
                        tone.red, tone.gray, bitmap.font.size, measured.width > 0,
                        measured.height > 0, Font.exist?("Arial")].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "12,34,0,255,1,4,18,true,true,true\n"
        );
    }

    #[test]
    fn sprite_and_audio_defaults_match_rgss_shape() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "sprite.rb",
                br#"
                  sprite = Sprite.new(Viewport.new(0, 0, 32, 32))
                  sprite.bitmap = Bitmap.new(7, 11)
                  audio = RPG::AudioFile.new("intro", 80, 90)
                  puts [sprite.src_rect.width, sprite.src_rect.height,
                        sprite.tone.red, sprite.color.alpha,
                        audio.name, audio.volume, audio.pitch].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "7,11,0,0,intro,80,90\n"
        );
    }

    #[test]
    fn sprite_bitmap_swap_updates_retained_src_rect_references() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "sprite-src-rect.rb",
                br#"
                  sprite = Sprite.new
                  old_rect = sprite.src_rect
                  sprite.bitmap = nil
                  bitmap = Bitmap.new(7, 11)
                  sprite.bitmap = bitmap
                  sprite.src_rect = old_rect
                  sprite.src_rect.set(1, 2, 3, 4)
                  sprite.bitmap = bitmap
                  preserved = [sprite.src_rect.x, sprite.src_rect.y,
                               sprite.src_rect.width, sprite.src_rect.height]
                  replacement = Bitmap.new(5, 6)
                  sprite.bitmap = replacement
                  puts [preserved.join("/"), sprite.src_rect.width,
                        sprite.src_rect.height].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "1/2/3/4,5,6\n"
        );
    }

    #[test]
    fn sprite_snapshot_carries_rgss_animation_effects() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "sprite-effects.rb",
                br#"
                  sprite = Sprite.new
                  sprite.bitmap = Bitmap.new(7, 11)
                  sprite.angle = 27.5
                  sprite.mirror = true
                  sprite.color = Color.new(10, 20, 30, 40)
                  sprite.tone = Tone.new(-1, 2, 3, 4)
                  sprite.blend_type = 1
                "#,
            )
            .unwrap();
        let snapshot = runtime.render_snapshot();
        let sprite = &snapshot.sprites[0];
        assert_eq!(sprite.angle, 27.5);
        assert!(sprite.mirror);
        assert_eq!(sprite.color, [10, 20, 30, 40]);
        assert_eq!(sprite.tone, [-1, 2, 3, 4]);
        assert_eq!(sprite.blend_type, 1);
    }

    #[test]
    fn bitmap_raster_operations_are_visible_to_rgss() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "bitmap-raster.rb",
                br#"
                  bitmap = Bitmap.new(8, 8)
                  bitmap.fill_rect(1, 2, 3, 4, Color.new(10, 20, 30, 255))
                  pixel = bitmap.get_pixel(2, 3)
                  puts [pixel.red, pixel.green, pixel.blue, pixel.alpha].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "10,20,30,255\n"
        );
    }

    #[test]
    fn bitmap_file_pixels_are_available_to_compositing_operations() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "bitmap-file-blt.rb",
                br#"
                  source = Bitmap.new("Graphics/UI/Load/panels")
                  target = Bitmap.new(source.width, source.height)
                  target.blt(0, 0, source, source.rect)
                  sprite = Sprite.new
                  sprite.bitmap = target
                "#,
            )
            .unwrap();
        let snapshot = runtime.render_snapshot();
        let pixels = snapshot.sprites[0]
            .bitmap_rgba
            .as_ref()
            .expect("a file bitmap must provide pixels to blt");
        assert!(pixels.iter().any(|pixel| *pixel != 0));
    }

    #[test]
    fn bundled_font_rasterizes_text_and_frame_count_is_writable() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "font-and-frame-count.rb",
                br#"
                  Graphics.frame_count = 42
                  bitmap = Bitmap.new(240, 48)
                  bitmap.font.name = "Power Green"
                  bitmap.font.size = 24
                  bitmap.draw_text(0, 0, 240, 48, "Anil 21", 0)
                  sprite = Sprite.new
                  sprite.bitmap = bitmap
                  puts Graphics.frame_count
                "#,
            )
            .unwrap();
        assert_eq!(String::from_utf8(runtime.take_output()).unwrap(), "42\n");
        let snapshot = runtime.render_snapshot();
        let pixels = snapshot.sprites[0]
            .bitmap_rgba
            .as_ref()
            .expect("text bitmap must expose pixels");
        let alpha_values = pixels
            .chunks_exact(4)
            .map(|pixel| pixel[3])
            .filter(|alpha| *alpha > 0)
            .collect::<BTreeSet<_>>();
        assert!(!alpha_values.is_empty());
    }

    #[test]
    fn font_defaults_match_rsxp_rgss_defaults() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "font-defaults.rb",
                br#"
                  font = Font.new
                  puts [font.name, font.size, font.bold, font.italic,
                        font.outline, font.shadow, font.out_color.alpha].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "Arial,22,false,false,false,false,128\n"
        );
    }

    #[test]
    fn gif_assets_are_decoded_as_frames_when_animated() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let bytes = OsVfs::new(project.root())
            .read("Graphics/UI/evolution_bg.gif")
            .unwrap();
        let decoded = decode_image(&bytes).expect("the bundled GIF should decode");
        assert!(!decoded.pixels.is_empty());
        if decoded.frames.len() > 1 {
            assert_eq!(decoded.frames.len(), decoded.frame_delays_ms.len());
            assert!(decoded.frame_delays_ms.iter().all(|delay| *delay > 0));
        }
    }

    #[test]
    fn plugin_archive_order_matches_compiled_game_order() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "plugin-order.rb",
                br#"
                  plugins = load_data("Data/PluginScripts.rxdata")
                  names = plugins.map { |plugin| plugin[0] }
                  puts names.index("Marin Side Stairs") < names.index("Mover panorama")
                "#,
            )
            .unwrap();
        assert_eq!(String::from_utf8(runtime.take_output()).unwrap(), "true\n");
    }

    #[test]
    fn audio_calls_leave_portable_commands_for_the_host() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "audio.rb",
                br#"Audio.bgm_play("Audio/BGM/CiudadAnil", 80, 110); Audio.se_stop"#,
            )
            .unwrap();
        let commands = runtime.take_audio_commands();
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].path, "Audio/BGM/CiudadAnil");
        assert_eq!(commands[0].volume, 80);
        assert!(commands[1].path.is_empty());
        assert!(runtime.read_asset("Audio/BGM/TITULO").is_some());
    }

    #[test]
    fn custom_vfs_handles_ruby_file_reads() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open_with_vfs(project, Arc::new(MemoryVfs)).unwrap();
        runtime
            .run_source(
                "vfs.rb",
                br#"puts File.open("Data/Scripts/virtual.rb", "rb").read"#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "from vfs\n"
        );
    }

    #[test]
    fn custom_vfs_preserves_directory_view_and_glob_sorting() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open_with_vfs(project, Arc::new(MemoryVfs)).unwrap();
        runtime
            .run_source(
                "vfs-dir.rb",
                br#"puts Dir.entries("Data/Scripts").join(","); puts Dir.glob("Data/Scripts/*.rb").join(",")"#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            ".,..,virtual.rb\nData/Scripts/virtual.rb\n"
        );
    }

    #[test]
    fn graphics_update_is_a_stepped_frame_boundary() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .start_source("frame.rb", b"loop { Graphics.update }")
            .unwrap();
        assert_eq!(runtime.frame_count(), 0);
        assert!(matches!(runtime.step(10_000).unwrap(), Step::Paused));
        assert_eq!(runtime.frame_count(), 1);
        assert!(matches!(runtime.step(10_000).unwrap(), Step::Paused));
        assert_eq!(runtime.frame_count(), 2);
    }

    #[test]
    fn input_symbols_and_direction_use_host_events() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime.input_press(11);
        runtime
            .run_source(
                "input.rb",
                b"Input.update; puts Input.pressex?(:LEFT); puts Input.dir4; puts Input.dir8",
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "true\n4\n4\n"
        );
    }
}
