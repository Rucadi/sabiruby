//! The reusable, headless part of an RGSS player.
//!
//! This crate deliberately owns no window, WGPU device, event loop, or audio
//! device. An embedding application supplies those services and drives the VM.
//! The first implementation provides project discovery, a rooted compiler/file
//! host, RGSS service state, and a small compatibility surface. Rendering and
//! device adapters can be added without changing the VM crate.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::io::{Read, Write};
use std::path::{Component, Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::Instant;

use flate2::read::{DeflateDecoder, ZlibDecoder};
use fontdue::{Font as DueFont, FontSettings};
use sabiruby::host::{EvalError, EvalOptions};
use sabiruby::object::{Aspec, ObjKind};
use sabiruby::value::ObjId;
use sabiruby::{Host, Step, Value, Vm, VmError};
use sabiruby_compiler::{Options, compile, compile_eval};

mod bindings;
pub mod media;

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
    /// Modification time as nanoseconds since the Unix epoch, when the backend
    /// records one. `File.mtime` reads this; backends without timestamps keep
    /// `None` and answer with the epoch.
    pub modified_ns: Option<i64>,
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

fn os_metadata(metadata: &fs::Metadata) -> VfsMetadata {
    VfsMetadata {
        len: metadata.len(),
        is_file: metadata.is_file(),
        is_dir: metadata.is_dir(),
        modified_ns: metadata.modified().ok().and_then(|modified| {
            modified
                .duration_since(std::time::UNIX_EPOCH)
                .ok()
                .map(|elapsed| i64::try_from(elapsed.as_nanos()).unwrap_or(i64::MAX))
        }),
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
        Ok(os_metadata(&fs::metadata(self.resolve(path)?)?))
    }

    fn read_dir(&self, path: &str) -> std::io::Result<Vec<VfsDirEntry>> {
        let mut entries = Vec::new();
        for entry in fs::read_dir(self.resolve(path)?)? {
            let entry = entry?;
            let metadata = entry.metadata()?;
            entries.push(VfsDirEntry {
                name: entry.file_name().to_string_lossy().into_owned(),
                metadata: os_metadata(&metadata),
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

/// Host event codes for extended input are `RAW_KEY_BASE + SDL scancode`.
/// RGSS action events use the standard numeric Input constants.
pub const RAW_KEY_BASE: i32 = 0x10000;

/// Host-side keyboard state shared with the Ruby `Input` module.
///
/// Events are queued by the window adapter and committed when Ruby calls
/// `Input.update`, matching RGSS's one-frame trigger/release semantics.
#[derive(Clone, Debug, Default)]
pub struct InputState {
    pending_pressed: BTreeSet<i32>,
    pending_triggered: BTreeSet<i32>,
    pending_released: BTreeSet<i32>,
    held_frames: HashMap<i32, u32>,
    text_input: bool,
    text: String,
    clipboard: String,
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
    // One immutable upload image per revision, shared by all tile sprites and
    // retained snapshots. Writers invalidate it before changing pixels.
    snapshot_pixels: OnceLock<Arc<[u8]>>,
    revision: u64,
    frames: Vec<Vec<u8>>,
    frame_delays_ms: Vec<u64>,
    current_frame: usize,
    animation_elapsed_ms: u64,
    manual_frame_rate: Option<f64>,
    playing: bool,
}

struct ActiveMovie {
    id: u64,
    decoder: media::movie::Movie,
    started_frame: u64,
    skippable: bool,
    current: Option<media::Frame>,
    revision: u64,
    finished: bool,
}

impl InputState {
    /// Accept committed keyboard/IME text only while Ruby requests text entry.
    pub fn push_text(&mut self, text: &str) {
        if self.text_input {
            self.text
                .extend(text.chars().filter(|character| !character.is_control()));
        }
    }

    pub fn press(&mut self, code: i32) {
        if self.pending_pressed.insert(code) {
            self.pending_triggered.insert(code);
        }
    }

    pub fn release(&mut self, code: i32) {
        if self.pending_pressed.remove(&code) {
            self.pending_released.insert(code);
        }
    }

    pub fn update(&mut self) {
        self.triggered = std::mem::take(&mut self.pending_triggered);
        self.released = self.pending_released.clone();
        self.pressed = self.pending_pressed.clone();
        self.held_frames
            .retain(|code, _| self.pressed.contains(code));
        for code in &self.pressed {
            let count = self.held_frames.entry(*code).or_default();
            *count = if self.triggered.contains(code) {
                1
            } else {
                count.saturating_add(1)
            };
        }
        self.pending_released.clear();
    }

    pub fn repeated(&self, code: i32) -> bool {
        self.triggered(code)
            || self
                .held_frames
                .get(&code)
                .is_some_and(|count| *count == 1 || (*count >= 24 && *count % 6 == 0))
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
    /// The RGSS mixer exposes a process-wide BGM volume even when the actual
    /// audio device is owned by the embedding host.  Keep the value in the
    /// core so `Audio.bgm_volume` and `Audio.bgm_set_volume` are deterministic
    /// in headless and non-desktop embeddings too.
    pub bgm_volume: u8,
    bitmaps: HashMap<ObjId, BitmapData>,
    fonts: HashMap<Vec<String>, Option<Arc<DueFont>>>,
    movie: Option<ActiveMovie>,
    next_movie_id: u64,
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
            bgm_volume: 100,
            bitmaps: HashMap::new(),
            fonts: HashMap::new(),
            movie: None,
            next_movie_id: 1,
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
    /// Viewport bounds in logical screen coordinates, before output scaling.
    pub clip_rect: Option<[i64; 4]>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RenderSnapshot {
    pub width: u32,
    pub height: u32,
    pub sprites: Vec<SpriteSnapshot>,
    /// The active `Graphics.play_movie` frame, if any.  Hosts should draw it
    /// above normal sprites while it is present.
    pub movie: Option<MovieSnapshot>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MovieSnapshot {
    pub id: u64,
    pub width: u32,
    pub height: u32,
    pub rgba: Arc<[u8]>,
    pub revision: u64,
    pub finished: bool,
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

/// Tolerant implementation of the helper used by several RGSS projects for
/// screen transitions.  Some projects clear their endpoints one tick before
/// the final `Game_Screen#update`, so the compatibility implementation treats
/// those nil values as the completed transition instead of raising from Ruby
/// arithmetic.
fn compat_lerp(
    _vm: &mut Vm,
    _receiver: Value,
    args: &[Value],
    _block: Value,
) -> Result<Value, VmError> {
    let start_value = if args[0].is_nil() {
        Value::Int(0)
    } else {
        args[0]
    };
    let end_value = if args[1].is_nil() {
        start_value
    } else {
        args[1]
    };
    let start = value_f64(start_value, 0.0);
    let end = value_f64(end_value, start);
    let duration = value_f64(args[2], 0.0);
    let mut delta = value_f64(args[3], 0.0);
    if duration <= 0.0 {
        return Ok(end_value);
    }
    if let Some(now) = args.get(4) {
        delta = value_f64(*now, 0.0) - delta;
    }
    if delta <= 0.0 {
        return Ok(start_value);
    }
    if delta >= duration {
        return Ok(end_value);
    }
    Ok(Value::Float(start + ((end - start) * delta / duration)))
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

fn graphics_play_movie(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vm
        .expect_str(args.first().copied().unwrap_or(Value::Nil), "movie path")
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())?;
    let skippable = args.get(2).is_some_and(|value| value.truthy());
    let vfs = state_mut(vm).vfs.clone();
    let bytes =
        read_asset_bytes(vfs.as_ref(), &path, &["ogv", "theora", "ogg"]).ok_or_else(|| {
            let message = format!("movie asset not found: {path}");
            vm.raise_arg(&message)
        })?;
    let decoder = media::movie::Movie::open(bytes).map_err(|error| {
        let message = format!("unable to open movie {path}: {error}");
        vm.raise_arg(&message)
    })?;
    let state = state_mut(vm);
    let id = state.next_movie_id;
    state.next_movie_id = state.next_movie_id.saturating_add(1);
    let started_frame = state.frame_count;
    state.movie = Some(ActiveMovie {
        id,
        decoder,
        started_frame,
        skippable,
        current: None,
        revision: 0,
        finished: false,
    });
    Ok(Value::Nil)
}

fn graphics_movie_active(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::bool(state_mut(vm).movie.is_some()))
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
    advance_movie(vm);
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
            bitmap.snapshot_pixels.take();
            bitmap.revision = bitmap.revision.saturating_add(1);
        }
    }
}

fn advance_movie(vm: &mut Vm) {
    let state = state_mut(vm);
    let Some(mut movie) = state.movie.take() else {
        return;
    };
    if movie.finished {
        // Keep the final decoded frame visible for one render boundary, then
        // release the decoder on the next Graphics.update.
        return;
    }
    if movie.skippable && (state.input.triggered(13) || state.input.triggered(12)) {
        return;
    }
    let elapsed = state.frame_count.saturating_sub(movie.started_frame) as f64
        / f64::from(state.frame_rate.max(1));
    let frame = match movie.decoder.frame_at(elapsed) {
        Ok(Some(frame)) => Some(frame.clone()),
        Ok(None) => None,
        Err(_) => {
            // A malformed packet should stop playback without taking down the
            // Ruby VM; the host can report the asset error separately.
            return;
        }
    };
    let decoder_revision = movie.decoder.revision();
    match frame {
        Some(frame) => {
            if movie.revision != decoder_revision {
                movie.current = Some(frame);
                movie.revision = decoder_revision;
            }
            movie.finished = movie.decoder.finished();
            state.movie = Some(movie);
        }
        None => {
            movie.finished = movie.decoder.finished();
            state.movie = Some(movie);
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
        Value::Nil => Ok(0),
        Value::Sym(symbol) => {
            let name = vm.sym_name(symbol);
            let code = vm.const_path_get(&format!("Input::{name}"));
            Ok(match code {
                Some(Value::Int(code)) => code as i32,
                _ => 0,
            })
        }
        _ => Err(vm.raise_type("Input key must be an Integer or Symbol")),
    }
}

fn input_raw_code(vm: &mut Vm, args: &[Value]) -> Result<i32, VmError> {
    let scan = match args.first().copied().unwrap_or(Value::Nil) {
        Value::Int(scan) => scan.clamp(0, 511) as i32,
        Value::Sym(symbol) => {
            let name = vm.sym_name(symbol);
            match name.as_str() {
                "RETURN" => 40,
                "ESCAPE" => 41,
                "BACKSPACE" => 42,
                "TAB" => 43,
                "SPACE" => 44,
                "GRAVE" => 53,
                "HOME" => 74,
                "PAGEUP" => 75,
                "DELETE" => 76,
                "END" => 77,
                "PAGEDOWN" => 78,
                "RIGHT" => 79,
                "LEFT" => 80,
                "DOWN" => 81,
                "UP" => 82,
                "KP_MINUS" => 86,
                "KP_PLUS" => 87,
                "KP_ENTER" => 88,
                "LCTRL" => 224,
                "LSHIFT" => 225,
                "LALT" => 226,
                "RCTRL" => 228,
                "RSHIFT" => 229,
                "RALT" => 230,
                name if name.len() == 1 && name.as_bytes()[0].is_ascii_uppercase() => {
                    i32::from(name.as_bytes()[0] - b'A') + 4
                }
                name if name.starts_with('F') => name[1..]
                    .parse::<i32>()
                    .ok()
                    .filter(|number| (1..=12).contains(number))
                    .map_or(0, |number| 57 + number),
                _ => 0,
            }
        }
        _ => return Err(vm.raise_type("Input key must be an Integer or Symbol")),
    };
    Ok(RAW_KEY_BASE + scan)
}

fn input_trigger_ex(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_raw_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.triggered(code)))
}

fn input_press_ex(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_raw_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.pressed(code)))
}

fn input_repeat_ex(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_raw_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.repeated(code)))
}

fn input_release_ex(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let code = input_raw_code(vm, args)?;
    Ok(Value::bool(state_mut(vm).input.released(code)))
}

fn input_text_mode(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::bool(state_mut(vm).input.text_input))
}

fn input_set_text_mode(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let enabled = args.first().copied().unwrap_or(Value::Nil).truthy();
    let input = &mut state_mut(vm).input;
    if input.text_input != enabled {
        input.text.clear();
    }
    input.text_input = enabled;
    Ok(Value::bool(enabled))
}

fn input_gets(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let text = std::mem::take(&mut state_mut(vm).input.text);
    Ok(vm.str_new(text.as_bytes()))
}

fn input_clipboard(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let text = state_mut(vm).input.clipboard.clone();
    Ok(vm.str_new(text.as_bytes()))
}

fn input_set_clipboard(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let value = args.first().copied().unwrap_or(Value::Nil);
    let text = if value.is_nil() {
        String::new()
    } else {
        String::from_utf8_lossy(&vm.expect_str(value, "clipboard text")?).into_owned()
    };
    state_mut(vm).input.clipboard = text;
    Ok(value)
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
    Ok(Value::bool(state_mut(vm).input.repeated(code)))
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
    let direction =
        if (input.pressed(2) && input.pressed(8)) || (input.pressed(4) && input.pressed(6)) {
            0
        } else {
            [2, 4, 6, 8]
                .into_iter()
                .find(|code| input.pressed(*code))
                .unwrap_or(0)
        };
    Ok(Value::Int(direction.into()))
}

fn input_dir8(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let input = &state_mut(vm).input;
    let x = i64::from(input.pressed(6)) - i64::from(input.pressed(4));
    let y = i64::from(input.pressed(2)) - i64::from(input.pressed(8));
    Ok(Value::Int(if x == 0 && y == 0 { 0 } else { 5 + x - y * 3 }))
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

fn audio_bgm_volume(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    if let Some(value) = args.first() {
        let volume = value_i64(*value, 100).clamp(0, 100) as u8;
        state_mut(vm).bgm_volume = volume;
        Ok(Value::Nil)
    } else {
        Ok(Value::Int(i64::from(state_mut(vm).bgm_volume)))
    }
}

fn audio_bgm_set_volume(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let volume =
        value_i64(args.first().copied().unwrap_or(Value::Int(100)), 100).clamp(0, 100) as u8;
    state_mut(vm).bgm_volume = volume;
    Ok(Value::Nil)
}

fn audio_reset(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let state = state_mut(vm);
    state.audio_commands.clear();
    for channel in [
        AudioChannel::Bgm,
        AudioChannel::Bgs,
        AudioChannel::Me,
        AudioChannel::Se,
    ] {
        state.audio_commands.push(AudioCommand {
            channel,
            path: String::new(),
            volume: 0,
            pitch: 100,
        });
    }
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

fn system_user_name(vm: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    // Identity supplied by the desktop OS, not a game configuration option.
    // Match rsxp's fallback so an empty name entry always has a usable name.
    let name = ["USER", "USERNAME"]
        .into_iter()
        .find_map(|key| {
            std::env::var(key)
                .ok()
                .filter(|name| !name.trim().is_empty())
        })
        .unwrap_or_else(|| "Player".to_owned());
    Ok(vm.str_new(name.as_bytes()))
}

fn system_is_windows(_: &mut Vm, _: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    Ok(Value::bool(cfg!(windows)))
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
            251 => Ok(0),
            252..=255 => {
                let width = usize::from(256u16 - u16::from(tag));
                Ok(self.read_unsigned(width)? - (1i64 << (width * 8)))
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
        self.vm.str_set_binary(value, true);
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
            let default = self.value()?;
            if let Some(object) = hash.obj() {
                if let ObjKind::Hash(data) = &mut self.vm.heap.get_mut(object).kind {
                    data.default.set(default);
                }
            }
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
            if matches!(name.as_str(), "Table" | "Color" | "Tone" | "Rect") {
                let object = self.vm.object_new(class);
                self.objects.push(object);
                decode_rgss_payload(self.vm, object, &name, &bytes)?;
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

    fn parse_ivar_wrapper(&mut self) -> Result<Value, VmError> {
        let object = self.value()?;
        for _ in 0..self.length()? {
            let symbol = self.symbol_value()?;
            let name = self.vm.sym_name(symbol);
            let value = self.value()?;
            // `:E` is MRI's String encoding flag. SabiRuby strings are UTF-8/
            // byte strings already, so retaining the data is sufficient.
            if name == "E" {
                self.vm.str_set_binary(object, value == Value::False);
            } else {
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

    fn decode_stream(&mut self) -> Result<Value, VmError> {
        if self.byte()? != 4 || self.byte()? != 8 {
            return self.fail("unsupported Marshal version");
        }
        self.value()
    }

    fn decode(mut self) -> Result<Value, VmError> {
        self.decode_stream()
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
fn file_mtime(vm: &mut Vm, _: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let path = vfs_arg(vm, args)?;
    let vfs = state_mut(vm).vfs.clone();
    let metadata = vfs
        .metadata(&path)
        .map_err(|error| vm.raise_arg(&format!("cannot get mtime of {path}: {error}")))?;
    let ns = metadata.modified_ns.unwrap_or(0);
    let sec = ns.div_euclid(1_000_000_000);
    let usec = ns.rem_euclid(1_000_000_000) / 1_000;
    let Some(time) = vm.const_path_get("Time").and_then(Value::obj) else {
        return Ok(Value::Nil);
    };
    let at = vm.intern("at");
    vm.funcall(
        Value::Obj(time),
        at,
        &[Value::Int(sec), Value::Int(usec)],
        Value::Nil,
    )
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

struct DecodedImage {
    width: i64,
    height: i64,
    pixels: Vec<u8>,
    frames: Vec<Vec<u8>>,
    frame_delays_ms: Vec<u64>,
}

fn decode_image(bytes: &[u8]) -> Option<DecodedImage> {
    if media::gif::is_gif(bytes) {
        let animation = media::gif::decode(bytes).ok()?;
        let first_pixels = animation.frames.first()?.pixels.clone();
        let animated = animation.frames.len() > 1;
        return Some(DecodedImage {
            width: animation.width.max(1) as i64,
            height: animation.height.max(1) as i64,
            pixels: first_pixels,
            frames: if animated {
                animation
                    .frames
                    .into_iter()
                    .map(|frame| frame.pixels)
                    .collect()
            } else {
                Vec::new()
            },
            frame_delays_ms: if animated {
                animation.delays_ms
            } else {
                Vec::new()
            },
        });
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
                .ok_or_else(|| vm.raise_type("Bitmap path must be a String"))?;
            let vfs = state_mut(vm).vfs.clone();
            let bytes =
                read_asset_bytes(vfs.as_ref(), &path, &["png", "gif", "jpg", "jpeg", "bmp"])
                    .ok_or_else(|| {
                        vm.raise(
                            vm.core.runtime_error,
                            &format!("Bitmap asset not found: {path}"),
                        )
                    })?;
            let decoded = decode_image(&bytes).ok_or_else(|| {
                vm.raise(
                    vm.core.runtime_error,
                    &format!("Bitmap asset could not be decoded: {path}"),
                )
            })?;
            let (width, height) = (decoded.width, decoded.height);
            (width, height, Some(path), Some(decoded))
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
            snapshot_pixels: OnceLock::new(),
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

fn bitmap_mega(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let width = value_i64(vm.ivar_get(object, "@width"), 0);
    let height = value_i64(vm.ivar_get(object, "@height"), 0);
    Ok(Value::bool(width > 8192 || height > 8192))
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
        bitmap.snapshot_pixels.take();
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
        bitmap.snapshot_pixels.take();
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
    data.snapshot_pixels.take();
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
    // Keep this in sync with the desktop renderer's guaranteed texture limit.
    // Essentials uses this value to split very tall tilesets before upload.
    Ok(Value::Int(8_192))
}

fn object_instance(vm: &mut Vm, receiver: Value) -> Result<ObjId, VmError> {
    receiver
        .obj()
        .ok_or_else(|| vm.raise_type("receiver is not an object"))
}

fn table_dimensions_from_sizes(_x: i64, y: i64, z: i64) -> i64 {
    if z > 1 {
        3
    } else if y > 1 {
        2
    } else {
        1
    }
}

fn table_initialize(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let x = value_i64(args.first().copied().unwrap_or(Value::Int(0)), 0).max(0);
    let y = value_i64(args.get(1).copied().unwrap_or(Value::Int(1)), 1).max(0);
    let z = value_i64(args.get(2).copied().unwrap_or(Value::Int(1)), 1).max(0);
    let Some(count) = x.checked_mul(y).and_then(|value| value.checked_mul(z)) else {
        return Err(vm.raise_arg("invalid Table dimensions"));
    };
    if count > 16_777_216 {
        return Err(vm.raise_arg("Table exceeds the 16 million element limit"));
    }
    vm.ivar_set(object, "@xsize", Value::Int(x));
    vm.ivar_set(object, "@ysize", Value::Int(y));
    vm.ivar_set(object, "@zsize", Value::Int(z));
    vm.ivar_set(
        object,
        "@dimensions",
        Value::Int(table_dimensions_from_sizes(x, y, z)),
    );
    let data = vm.ary_new(vec![Value::Int(0); count as usize]);
    vm.ivar_set(object, "@data", data);
    vm.ivar_set(object, "@__sabiruby_revision", Value::Int(1));
    Ok(receiver)
}

fn table_dimensions(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let x = value_i64(vm.ivar_get(object, "@xsize"), 0);
    let y = value_i64(vm.ivar_get(object, "@ysize"), 0);
    let z = value_i64(vm.ivar_get(object, "@zsize"), 0);
    Ok(vm.ary_new(vec![Value::Int(x), Value::Int(y), Value::Int(z)]))
}

fn table_revision(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    Ok(vm.ivar_get(object, "@__sabiruby_revision"))
}

fn table_coordinates(vm: &mut Vm, object: ObjId, args: &[Value]) -> Option<usize> {
    let x = value_i64(args.first().copied()?, -1);
    let y = value_i64(args.get(1).copied().unwrap_or(Value::Int(0)), -1);
    let z = value_i64(args.get(2).copied().unwrap_or(Value::Int(0)), -1);
    let xsize = value_i64(vm.ivar_get(object, "@xsize"), 0);
    let ysize = value_i64(vm.ivar_get(object, "@ysize"), 0);
    let zsize = value_i64(vm.ivar_get(object, "@zsize"), 0);
    if x < 0 || y < 0 || z < 0 || x >= xsize || y >= ysize || z >= zsize {
        return None;
    }
    let index = z
        .checked_mul(ysize)?
        .checked_add(y)?
        .checked_mul(xsize)?
        .checked_add(x)?;
    usize::try_from(index).ok()
}

fn table_get(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let Some(index) = table_coordinates(vm, object, args) else {
        return Ok(Value::Nil);
    };
    let data = vm.ivar_get(object, "@data");
    Ok(data
        .obj()
        .and_then(|id| match &vm.heap.get(id).kind {
            ObjKind::Array(values) => values.get(index).map(|slot| slot.get()),
            _ => None,
        })
        .unwrap_or(Value::Nil))
}

fn table_set(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let value = value_i64(args.get(3).copied().unwrap_or(Value::Int(0)), 0) as i16 as i64;
    let Some(index) = table_coordinates(vm, object, args) else {
        return Ok(Value::Int(value));
    };
    let data = vm.ivar_get(object, "@data");
    if let Some(data_object) = data.obj() {
        if let ObjKind::Array(values) = &mut vm.heap.get_mut(data_object).kind {
            if let Some(slot) = values.get_mut(index) {
                slot.set(Value::Int(value));
            }
        }
    }
    let revision = value_i64(vm.ivar_get(object, "@__sabiruby_revision"), 0);
    vm.ivar_set(
        object,
        "@__sabiruby_revision",
        Value::Int(revision.saturating_add(1)),
    );
    Ok(Value::Int(value))
}

fn table_resize(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let x = value_i64(args.first().copied().unwrap_or(Value::Int(0)), 0).max(0);
    let y = value_i64(args.get(1).copied().unwrap_or(Value::Int(1)), 1).max(0);
    let z = value_i64(args.get(2).copied().unwrap_or(Value::Int(1)), 1).max(0);
    let Some(count) = x.checked_mul(y).and_then(|value| value.checked_mul(z)) else {
        return Err(vm.raise_arg("invalid Table dimensions"));
    };
    if count > 16_777_216 {
        return Err(vm.raise_arg("Table exceeds the 16 million element limit"));
    }
    let old_x = value_i64(vm.ivar_get(object, "@xsize"), 0);
    let old_y = value_i64(vm.ivar_get(object, "@ysize"), 0);
    let old_z = value_i64(vm.ivar_get(object, "@zsize"), 0);
    let old = vm
        .ary_vals(vm.ivar_get(object, "@data"))
        .unwrap_or_default();
    let mut next = vec![Value::Int(0); count as usize];
    for z_index in 0..z.min(old_z) {
        for y_index in 0..y.min(old_y) {
            for x_index in 0..x.min(old_x) {
                let old_index = (z_index * old_y + y_index) * old_x + x_index;
                let new_index = (z_index * y + y_index) * x + x_index;
                if let (Some(source), Some(destination)) = (
                    usize::try_from(old_index)
                        .ok()
                        .and_then(|index| old.get(index)),
                    usize::try_from(new_index)
                        .ok()
                        .and_then(|index| next.get_mut(index)),
                ) {
                    *destination = *source;
                }
            }
        }
    }
    vm.ivar_set(object, "@xsize", Value::Int(x));
    vm.ivar_set(object, "@ysize", Value::Int(y));
    vm.ivar_set(object, "@zsize", Value::Int(z));
    vm.ivar_set(
        object,
        "@dimensions",
        Value::Int(table_dimensions_from_sizes(x, y, z)),
    );
    let data = vm.ary_new(next);
    vm.ivar_set(object, "@data", data);
    let revision = value_i64(vm.ivar_get(object, "@__sabiruby_revision"), 0);
    vm.ivar_set(
        object,
        "@__sabiruby_revision",
        Value::Int(revision.saturating_add(1)),
    );
    Ok(receiver)
}

fn table_dump(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let x = value_i64(vm.ivar_get(object, "@xsize"), 0);
    let y = value_i64(vm.ivar_get(object, "@ysize"), 0);
    let z = value_i64(vm.ivar_get(object, "@zsize"), 0);
    let values = vm
        .ary_vals(vm.ivar_get(object, "@data"))
        .unwrap_or_default();
    let mut bytes = Vec::with_capacity(20 + values.len() * 2);
    for value in [
        table_dimensions_from_sizes(x, y, z),
        x,
        y,
        z,
        values.len() as i64,
    ] {
        bytes.extend_from_slice(&(value as i32).to_le_bytes());
    }
    for value in values {
        bytes.extend_from_slice(&(value_i64(value, 0) as i16).to_le_bytes());
    }
    Ok(vm.str_new(&bytes))
}

fn decode_rgss_payload(
    vm: &mut Vm,
    object: Value,
    name: &str,
    bytes: &[u8],
) -> Result<(), VmError> {
    if name != "Table" {
        let (fields, component_size): ([&str; 4], usize) = match name {
            "Color" => (["@red", "@green", "@blue", "@alpha"], 8),
            "Tone" => (["@red", "@green", "@blue", "@gray"], 8),
            "Rect" => (["@x", "@y", "@width", "@height"], 4),
            _ => return Err(vm.raise_arg("unknown RGSS value type")),
        };
        if bytes.len() != component_size * 4 {
            return Err(vm.raise_arg(&format!("bad {name} payload")));
        }
        let id = object_instance(vm, object)?;
        for (field, bytes) in fields.into_iter().zip(bytes.chunks_exact(component_size)) {
            let value = if component_size == 8 {
                Value::Float(f64::from_le_bytes(bytes.try_into().expect("RGSS double")))
            } else {
                Value::Int(i64::from(i32::from_le_bytes(
                    bytes.try_into().expect("RGSS integer"),
                )))
            };
            vm.ivar_set(id, field, value);
        }
        return Ok(());
    }
    if bytes.len() < 20 {
        return Err(vm.raise_arg("bad Table payload"));
    }
    let read = |offset: usize| -> i64 {
        i32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("Table header")) as i64
    };
    let dimensions = read(0);
    let x = read(4);
    let y = read(8);
    let z = read(12);
    let count = read(16);
    if !(1..=3).contains(&dimensions)
        || [x, y, z, count].iter().any(|value| *value < 0)
        || count > 16_777_216
        || x.checked_mul(y).and_then(|value| value.checked_mul(z)) != Some(count)
        || bytes.len() != 20 + count as usize * 2
    {
        return Err(vm.raise_arg("bad Table payload"));
    }
    // RGSS stores signed 16-bit cells after five 32-bit header words.
    // Reading pairs of cells as i32 corrupts tile IDs and passage flags.
    let values = bytes[20..]
        .chunks_exact(2)
        .map(|bytes| {
            Value::Int(i64::from(i16::from_le_bytes(
                bytes.try_into().expect("Table cell"),
            )))
        })
        .collect();
    let id = object_instance(vm, object)?;
    for (field, value) in [
        ("@dimensions", dimensions),
        ("@xsize", x),
        ("@ysize", y),
        ("@zsize", z),
        ("@__sabiruby_revision", 1),
    ] {
        vm.ivar_set(id, field, Value::Int(value));
    }
    let data = vm.ary_new(values);
    vm.ivar_set(id, "@data", data);
    Ok(())
}

fn table_load(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    let bytes = vm.expect_str(args.first().copied().unwrap_or(Value::Nil), "Table payload")?;
    let class = receiver
        .obj()
        .ok_or_else(|| vm.raise_type("Table._load receiver is not a class"))?;
    let table = vm.object_new(class);
    decode_rgss_payload(vm, table, "Table", &bytes)?;
    Ok(table)
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

fn rgss_value_equal(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    class: &str,
    fields: [&str; 4],
) -> Result<Value, VmError> {
    let other = args.first().copied().unwrap_or(Value::Nil);
    let Some(class) = vm.const_path_get(class).and_then(Value::obj) else {
        return Ok(Value::False);
    };
    if !vm.obj_is_kind_of(other, class) {
        return Ok(Value::False);
    }
    let left = object_instance(vm, receiver)?;
    let right = object_instance(vm, other)?;
    Ok(Value::bool(fields.into_iter().all(|field| {
        match (vm.ivar_get(left, field), vm.ivar_get(right, field)) {
            (Value::Int(left), Value::Int(right)) => left == right,
            (left, right) => value_f64(left, 0.0) == value_f64(right, 0.0),
        }
    })))
}

fn tone_equal(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    rgss_value_equal(
        vm,
        receiver,
        args,
        "Tone",
        ["@red", "@green", "@blue", "@gray"],
    )
}

fn color_equal(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    rgss_value_equal(
        vm,
        receiver,
        args,
        "Color",
        ["@red", "@green", "@blue", "@alpha"],
    )
}

fn rect_equal(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    rgss_value_equal(
        vm,
        receiver,
        args,
        "Rect",
        ["@x", "@y", "@width", "@height"],
    )
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
    let object = object_instance(vm, receiver)?;
    if vm.ivar_get(object, "@__sabiruby_registered").truthy() {
        return Ok(());
    }
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
    vm.ary_push(instances, receiver)?;
    vm.ivar_set(object, "@__sabiruby_registered", Value::True);
    Ok(())
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

fn font_component_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    name: &str,
) -> Result<Value, VmError> {
    let object = object_instance(vm, receiver)?;
    let value = args.first().copied().unwrap_or(Value::Nil);
    vm.ivar_set(object, name, value);
    Ok(value)
}

fn font_name(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@name")
}

fn font_name_set(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@name")
}

fn font_size(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@size")
}

fn font_size_set(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@size")
}

fn font_bold(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@bold")
}

fn font_bold_set(vm: &mut Vm, receiver: Value, args: &[Value], _: Value) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@bold")
}

fn font_italic(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@italic")
}

fn font_italic_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@italic")
}

fn font_outline(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@outline")
}

fn font_outline_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@outline")
}

fn font_shadow(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@shadow")
}

fn font_shadow_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@shadow")
}

fn font_color(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@color")
}

fn font_color_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@color")
}

fn font_out_color(vm: &mut Vm, receiver: Value, _: &[Value], _: Value) -> Result<Value, VmError> {
    font_component(vm, receiver, "@out_color")
}

fn font_out_color_set(
    vm: &mut Vm,
    receiver: Value,
    args: &[Value],
    _: Value,
) -> Result<Value, VmError> {
    font_component_set(vm, receiver, args, "@out_color")
}

fn fontdue_font(vm: &mut Vm, font: Value) -> Option<Arc<DueFont>> {
    let names = font_names(vm, font);
    if let Some(font) = state_mut(vm).fonts.get(&names) {
        return font.clone();
    }
    let font = font_asset_bytes(vm, &names)
        .and_then(|bytes| DueFont::from_bytes(bytes, FontSettings::default()).ok())
        .map(Arc::new);
    let cache = &mut state_mut(vm).fonts;
    // A game normally uses only a handful of font families. Bound dynamic
    // requests (and misses) rather than retaining arbitrary names forever.
    if cache.len() >= 64 {
        cache.clear();
    }
    cache.insert(names, font.clone());
    font
}

fn line_width(font: &DueFont, scale: f32, text: &str) -> f32 {
    text.chars()
        .map(|character| font.metrics(character, scale).advance_width)
        .sum()
}

fn font_names(vm: &Vm, font: Value) -> Vec<String> {
    let name = font
        .obj()
        .map(|object| vm.ivar_get(object, "@name"))
        .unwrap_or(Value::Nil);
    if let Some(values) = vm.ary_vals(name) {
        values
            .into_iter()
            .filter_map(|value| vm.str_bytes(value))
            .map(|bytes| String::from_utf8_lossy(bytes).into_owned())
            .collect::<Vec<_>>()
    } else {
        vm.str_bytes(name)
            .map(|bytes| vec![String::from_utf8_lossy(bytes).into_owned()])
            .unwrap_or_default()
    }
}

fn font_asset_bytes(vm: &mut Vm, names: &[String]) -> Option<Vec<u8>> {
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
    // Snapshots retain their immutable pixels independently of the Ruby object.
    state_mut(vm).bitmaps.remove(&object);
    if vm.ivar_get(object, "@__sabiruby_registered").truthy() {
        if let Some(class) = vm.const_path_get("Sprite").and_then(Value::obj) {
            if let Some(registry) = vm.ivar_get(class, "@__sabiruby_instances").obj() {
                if let ObjKind::Array(instances) = &mut vm.heap.get_mut(registry).kind {
                    if let Some(index) = instances.iter().position(|slot| slot.get() == receiver) {
                        instances.remove(index);
                    }
                }
            }
        }
        vm.ivar_set(object, "@__sabiruby_registered", Value::False);
    }
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
    bindings::install(vm, project, Arc::new(OsVfs::new(project.root.clone())));
}

/// Install RGSS bindings using an embedding-selected filesystem.
pub fn install_rgss_with_vfs(vm: &mut Vm, project: &Project, vfs: Arc<dyn VirtualFileSystem>) {
    bindings::install(vm, project, vfs);
}

// Symbols are resolved once per projection, rather than once per property
// of every sprite. Component arrays stay on the stack.
struct RenderReader<'a> {
    vm: &'a Vm,
    internal_sabiruby_instances: Option<sabiruby::symbol::Sym>,
    internal_sabiruby_plane: Option<sabiruby::symbol::Sym>,
    alpha: Option<sabiruby::symbol::Sym>,
    angle: Option<sabiruby::symbol::Sym>,
    bitmap: Option<sabiruby::symbol::Sym>,
    blend_type: Option<sabiruby::symbol::Sym>,
    blue: Option<sabiruby::symbol::Sym>,
    color: Option<sabiruby::symbol::Sym>,
    disposed: Option<sabiruby::symbol::Sym>,
    gray: Option<sabiruby::symbol::Sym>,
    green: Option<sabiruby::symbol::Sym>,
    height: Option<sabiruby::symbol::Sym>,
    mirror: Option<sabiruby::symbol::Sym>,
    opacity: Option<sabiruby::symbol::Sym>,
    ox: Option<sabiruby::symbol::Sym>,
    oy: Option<sabiruby::symbol::Sym>,
    path: Option<sabiruby::symbol::Sym>,
    rect: Option<sabiruby::symbol::Sym>,
    red: Option<sabiruby::symbol::Sym>,
    src_rect: Option<sabiruby::symbol::Sym>,
    tone: Option<sabiruby::symbol::Sym>,
    viewport: Option<sabiruby::symbol::Sym>,
    visible: Option<sabiruby::symbol::Sym>,
    width: Option<sabiruby::symbol::Sym>,
    x: Option<sabiruby::symbol::Sym>,
    y: Option<sabiruby::symbol::Sym>,
    z: Option<sabiruby::symbol::Sym>,
    zoom_x: Option<sabiruby::symbol::Sym>,
    zoom_y: Option<sabiruby::symbol::Sym>,
}

impl<'a> RenderReader<'a> {
    fn new(vm: &'a Vm) -> Self {
        Self {
            vm,
            internal_sabiruby_instances: vm.syms.lookup_str("@__sabiruby_instances"),
            internal_sabiruby_plane: vm.syms.lookup_str("@__sabiruby_plane"),
            alpha: vm.syms.lookup_str("@alpha"),
            angle: vm.syms.lookup_str("@angle"),
            bitmap: vm.syms.lookup_str("@bitmap"),
            blend_type: vm.syms.lookup_str("@blend_type"),
            blue: vm.syms.lookup_str("@blue"),
            color: vm.syms.lookup_str("@color"),
            disposed: vm.syms.lookup_str("@disposed"),
            gray: vm.syms.lookup_str("@gray"),
            green: vm.syms.lookup_str("@green"),
            height: vm.syms.lookup_str("@height"),
            mirror: vm.syms.lookup_str("@mirror"),
            opacity: vm.syms.lookup_str("@opacity"),
            ox: vm.syms.lookup_str("@ox"),
            oy: vm.syms.lookup_str("@oy"),
            path: vm.syms.lookup_str("@path"),
            rect: vm.syms.lookup_str("@rect"),
            red: vm.syms.lookup_str("@red"),
            src_rect: vm.syms.lookup_str("@src_rect"),
            tone: vm.syms.lookup_str("@tone"),
            viewport: vm.syms.lookup_str("@viewport"),
            visible: vm.syms.lookup_str("@visible"),
            width: vm.syms.lookup_str("@width"),
            x: vm.syms.lookup_str("@x"),
            y: vm.syms.lookup_str("@y"),
            z: vm.syms.lookup_str("@z"),
            zoom_x: vm.syms.lookup_str("@zoom_x"),
            zoom_y: vm.syms.lookup_str("@zoom_y"),
        }
    }
    #[inline]
    fn ivar_get(&self, object: ObjId, name: &str) -> Value {
        let symbol = match name {
            "@__sabiruby_instances" => self.internal_sabiruby_instances,
            "@__sabiruby_plane" => self.internal_sabiruby_plane,
            "@alpha" => self.alpha,
            "@angle" => self.angle,
            "@bitmap" => self.bitmap,
            "@blend_type" => self.blend_type,
            "@blue" => self.blue,
            "@color" => self.color,
            "@disposed" => self.disposed,
            "@gray" => self.gray,
            "@green" => self.green,
            "@height" => self.height,
            "@mirror" => self.mirror,
            "@opacity" => self.opacity,
            "@ox" => self.ox,
            "@oy" => self.oy,
            "@path" => self.path,
            "@rect" => self.rect,
            "@red" => self.red,
            "@src_rect" => self.src_rect,
            "@tone" => self.tone,
            "@viewport" => self.viewport,
            "@visible" => self.visible,
            "@width" => self.width,
            "@x" => self.x,
            "@y" => self.y,
            "@z" => self.z,
            "@zoom_x" => self.zoom_x,
            "@zoom_y" => self.zoom_y,
            _ => unreachable!("unknown render property"),
        };
        symbol.map_or(Value::Nil, |symbol| self.vm.heap.ivar_get(object, symbol))
    }
    fn color(&self, value: Value, defaults: [i64; 4]) -> [i64; 4] {
        let Some(object) = value.obj() else {
            return defaults;
        };
        let names = ["@red", "@green", "@blue", "@alpha"];
        core::array::from_fn(|i| {
            value_i64(self.ivar_get(object, names[i]), defaults[i]).clamp(0, 255)
        })
    }
    fn tone(&self, value: Value) -> [i16; 4] {
        let Some(object) = value.obj() else {
            return [0; 4];
        };
        let values = ["@red", "@green", "@blue", "@gray"]
            .map(|name| value_i64(self.ivar_get(object, name), 0));
        [
            values[0].clamp(-255, 255) as i16,
            values[1].clamp(-255, 255) as i16,
            values[2].clamp(-255, 255) as i16,
            values[3].clamp(0, 255) as i16,
        ]
    }
}

#[derive(Clone, Copy)]
struct RenderViewport {
    clip_rect: Option<[i64; 4]>,
    dx: i64,
    dy: i64,
    z: i64,
    hidden: bool,
    color: [u8; 4],
    tone: [i16; 4],
}

impl RenderReader<'_> {
    fn viewport_info(&self, viewport: ObjId, width: u32, height: u32) -> RenderViewport {
        let clip_rect = self.ivar_get(viewport, "@rect").obj().map(|rect| {
            [
                value_i64(self.ivar_get(rect, "@x"), 0),
                value_i64(self.ivar_get(rect, "@y"), 0),
                value_i64(self.ivar_get(rect, "@width"), width.into()),
                value_i64(self.ivar_get(rect, "@height"), height.into()),
            ]
        });
        let rect = clip_rect.unwrap_or([0, 0, width.into(), height.into()]);
        RenderViewport {
            clip_rect,
            dx: rect[0] - value_i64(self.ivar_get(viewport, "@ox"), 0),
            dy: rect[1] - value_i64(self.ivar_get(viewport, "@oy"), 0),
            z: value_i64(self.ivar_get(viewport, "@z"), 0),
            hidden: self.ivar_get(viewport, "@disposed").truthy()
                || !self.ivar_get(viewport, "@visible").truthy(),
            color: self
                .color(self.ivar_get(viewport, "@color"), [255, 255, 255, 0])
                .map(|v| v as u8),
            tone: self.tone(self.ivar_get(viewport, "@tone")),
        }
    }
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
        Ok(Self { project, vm })
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

    /// Monotonic count of `Graphics.update` boundaries. Unlike the Ruby-visible
    /// frame count, scripts cannot reset this value while loading a new scene.
    /// Hosts should present only when it advances, not on a budget-only pause.
    pub fn completed_frames(&self) -> u64 {
        self.vm
            .host_state::<RgssState>()
            .map_or(0, |state| state.frames)
    }

    /// Snapshot the Ruby sprite registry for an embedding renderer.  Ruby
    /// remains authoritative for every property; this is intentionally a
    /// read-only projection at the frame boundary.
    pub fn render_snapshot(&self) -> RenderSnapshot {
        let reader = RenderReader::new(&self.vm);
        let (width, height) = self
            .vm
            .host_state::<RgssState>()
            .map_or((512, 384), |state| (state.width, state.height));
        let registry = self
            .vm
            .const_path_get("Sprite")
            .and_then(Value::obj)
            .and_then(|class| reader.ivar_get(class, "@__sabiruby_instances").obj());
        let sprites = registry
            .and_then(|id| match &self.vm.heap.get(id).kind {
                ObjKind::Array(values) => Some(values.as_slice()),
                _ => None,
            })
            .unwrap_or_default();
        let state = self.vm.host_state::<RgssState>();
        let mut viewports = HashMap::new();
        let mut bitmaps = HashMap::new();
        let mut snapshot = Vec::with_capacity(sprites.len());
        for (creation_order, value) in sprites.iter().enumerate() {
            let Some(object) = value.get().obj() else {
                continue;
            };
            if reader.ivar_get(object, "@disposed").truthy() {
                continue;
            }
            let bitmap = reader.ivar_get(object, "@bitmap");
            let Some(bitmap) = bitmap.obj() else {
                continue;
            };
            if reader.ivar_get(bitmap, "@disposed").truthy() {
                continue;
            }
            let info = bitmaps.entry(bitmap).or_insert_with(|| {
                let bitmap_width = value_i64(reader.ivar_get(bitmap, "@width"), 1).max(1) as u32;
                let bitmap_height = value_i64(reader.ivar_get(bitmap, "@height"), 1).max(1) as u32;
                let data = state.and_then(|state| state.bitmaps.get(&bitmap));
                let path = self
                    .vm
                    .str_bytes(reader.ivar_get(bitmap, "@path"))
                    .filter(|_| data.is_none_or(|data| data.pixels.is_empty()))
                    .map(|bytes| String::from_utf8_lossy(bytes).into_owned());
                (bitmap_width, bitmap_height, data, path)
            });
            let (bitmap_width, bitmap_height, bitmap_data, bitmap_path) =
                (info.0, info.1, info.2, info.3.clone());
            let src_rect = reader.ivar_get(object, "@src_rect").obj();
            let (src_x, src_y, src_width, src_height) = src_rect
                .map(|rect| {
                    (
                        value_i64(reader.ivar_get(rect, "@x"), 0),
                        value_i64(reader.ivar_get(rect, "@y"), 0),
                        value_i64(reader.ivar_get(rect, "@width"), bitmap_width as i64),
                        value_i64(reader.ivar_get(rect, "@height"), bitmap_height as i64),
                    )
                })
                .unwrap_or((0, 0, bitmap_width as i64, bitmap_height as i64));
            // Ruby scripts frequently retain a source rectangle while
            // swapping an AnimatedBitmap.  Clamp stale dimensions to the
            // current bitmap so a renderer cannot sample outside the texture
            // (which otherwise appears as a cropped or stretched sprite).
            let src_x = src_x.clamp(0, i64::from(bitmap_width));
            let src_y = src_y.clamp(0, i64::from(bitmap_height));
            let src_width = src_width.max(0).min(i64::from(bitmap_width) - src_x);
            let src_height = src_height.max(0).min(i64::from(bitmap_height) - src_y);
            let viewport = reader.ivar_get(object, "@viewport").obj();
            let view = viewport.map(|viewport| {
                *viewports
                    .entry(viewport)
                    .or_insert_with(|| reader.viewport_info(viewport, width, height))
            });
            if view.is_some_and(|view| view.hidden) {
                continue;
            }
            let clip_rect = view.and_then(|view| view.clip_rect);
            let (dx, dy) = view.map_or((0, 0), |view| (view.dx, view.dy));
            let z = value_i64(reader.ivar_get(object, "@z"), 0);
            let y = value_i64(reader.ivar_get(object, "@y"), 0);
            // A viewport is a compositing group. Child z values cannot escape
            // their viewport and cover pictures/UI in a higher viewport.
            // Equal-z sprites retain creation order, regardless of y. Sorting
            // by y would draw a naming-screen shadow over the character's feet.
            let order = (
                view.map_or(z, |view| view.z),
                viewport.map_or(0, |v| v.0 + 1),
                z,
                creation_order,
            );
            let mut color = reader
                .color(reader.ivar_get(object, "@color"), [255, 255, 255, 0])
                .map(|component| component as u8);
            let mut tone = reader.tone(reader.ivar_get(object, "@tone"));
            if let Some(view) = view {
                if view.color[3] != 0 {
                    color = overlay_colors(color, view.color);
                }
                if view.tone != [0; 4] {
                    tone = add_tones(tone, view.tone);
                }
            }
            snapshot.push((
                order,
                SpriteSnapshot {
                    bitmap_id: bitmap.0,
                    bitmap_path,
                    bitmap_rgba: bitmap_data
                        .filter(|data| !data.pixels.is_empty())
                        .map(|data| {
                            data.snapshot_pixels
                                .get_or_init(|| Arc::from(data.pixels.as_slice()))
                                .clone()
                        }),
                    bitmap_width,
                    bitmap_height,
                    bitmap_revision: bitmap_data.map_or(0, |data| data.revision),
                    src_x,
                    src_y,
                    src_width,
                    src_height,
                    x: value_i64(reader.ivar_get(object, "@x"), 0) + dx,
                    y: y + dy,
                    ox: value_i64(reader.ivar_get(object, "@ox"), 0),
                    oy: value_i64(reader.ivar_get(object, "@oy"), 0),
                    zoom_x: value_f64(reader.ivar_get(object, "@zoom_x"), 1.0),
                    zoom_y: value_f64(reader.ivar_get(object, "@zoom_y"), 1.0),
                    opacity: value_i64(reader.ivar_get(object, "@opacity"), 255).clamp(0, 255)
                        as u8,
                    angle: value_f64(reader.ivar_get(object, "@angle"), 0.0),
                    mirror: reader.ivar_get(object, "@mirror").truthy(),
                    color,
                    tone,
                    blend_type: value_i64(reader.ivar_get(object, "@blend_type"), 0),
                    visible: reader.ivar_get(object, "@visible").truthy(),
                    z,
                    tile: reader.ivar_get(object, "@__sabiruby_plane").truthy(),
                    clip_rect,
                },
            ));
        }
        snapshot.sort_by_key(|(order, _)| *order);
        let snapshot = snapshot.into_iter().map(|(_, sprite)| sprite).collect();
        let movie = self
            .vm
            .host_state::<RgssState>()
            .and_then(|state| state.movie.as_ref())
            .and_then(|movie| {
                movie.current.as_ref().map(|frame| MovieSnapshot {
                    id: movie.id,
                    width: frame.width,
                    height: frame.height,
                    rgba: Arc::from(frame.pixels.clone().into_boxed_slice()),
                    revision: movie.revision,
                    finished: movie.finished,
                })
            });
        RenderSnapshot {
            width,
            height,
            sprites: snapshot,
            movie,
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
                "png", "gif", "jpg", "jpeg", "bmp", "ogv", "theora", "ogg", "wav", "mp3", "mid",
                "midi",
            ],
        )
    }

    /// Open a Theora movie through the same VFS used by Ruby.  The returned
    /// decoder is host-neutral; call [`media::movie::Movie::frame_at`] from a
    /// render loop and upload the returned RGBA frame in the host backend.
    pub fn open_movie(&self, path: &str) -> Result<media::movie::Movie, RgssError> {
        let vfs = self
            .vm
            .host_state::<RgssState>()
            .map(|state| state.vfs.clone())
            .ok_or_else(|| RgssError::Runtime("RGSS state is not installed".to_owned()))?;
        let bytes = read_asset_bytes(vfs.as_ref(), path, &["ogv", "theora", "ogg"])
            .ok_or_else(|| RgssError::Runtime(format!("movie asset not found: {path}")))?;
        media::movie::Movie::open(bytes).map_err(RgssError::Runtime)
    }

    /// Whether the host should enable native keyboard/IME text entry.
    pub fn text_input_enabled(&self) -> bool {
        self.vm
            .host_state::<RgssState>()
            .is_some_and(|state| state.input.text_input)
    }

    /// Supply committed UTF-8 text from the host keyboard or input method.
    /// Ruby consumes it once through `Input.gets`; button events remain separate.
    pub fn input_text(&mut self, text: &str) {
        if let Some(state) = self.vm.host_state_mut::<RgssState>() {
            state.input.push_text(text);
        }
    }

    /// Queue a host keyboard/button press for the next Ruby `Input.update`.
    /// Use RGSS action IDs (2/4/6/8 for directions, 13 for confirm), or
    /// [`RAW_KEY_BASE`] plus an SDL scancode for physical keyboard queries.
    pub fn input_press(&mut self, code: i32) {
        if let Some(state) = self.vm.host_state_mut::<RgssState>() {
            state.input.press(code);
        }
    }

    /// Queue a host keyboard/button release for the next Ruby `Input.update`.
    /// Codes use the same action/raw namespaces as [`Self::input_press`].
    pub fn input_release(&mut self, code: i32) {
        if let Some(state) = self.vm.host_state_mut::<RgssState>() {
            state.input.release(code);
        }
    }

    /// Install small compatibility shims after the game's own scripts have
    /// loaded, so they wrap the project's helpers without editing its archive.
    fn install_script_compat(&mut self) -> Result<(), RgssError> {
        let lerp = self.vm.intern("lerp");
        if self.vm.find_method(self.vm.core.object, lerp).is_some() {
            let object = self.vm.core.object;
            self.vm
                .define_method_args(object, "lerp", Aspec::arg(4, 1), compat_lerp);
        }
        if bindings::idle::install(&mut self.vm)? {
            self.run_source("<SabiRuby tile dispatcher>", bindings::idle::DISPATCH)?;
        }
        Ok(())
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
        self.install_script_compat()?;
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
        self.install_script_compat()?;
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

#[cfg(test)]
mod tests {
    use super::*;
    mod tile_idle;

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
                    modified_ns: None,
                }),
                "Data/Scripts/virtual.rb" => Ok(VfsMetadata {
                    len: 8,
                    is_file: true,
                    is_dir: false,
                    modified_ns: Some(1_700_000_000_123_456_789),
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
                        modified_ns: Some(1_700_000_000_123_456_789),
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
    fn missing_bitmap_assets_fail_instead_of_becoming_empty_textures() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        let error = runtime
            .run_source(
                "missing-bitmap.rb",
                br#"Bitmap.new("Graphics/Pictures/does_not_exist")"#,
            )
            .expect_err("missing bitmap must stop the scene");
        let message = error.to_string();
        assert!(message.contains("Bitmap asset not found"), "{message}");
        assert!(message.contains("does_not_exist"), "{message}");
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
    fn font_instance_setters_update_bitmap_text_state() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "font-setters.rb",
                br#"
                  font = Font.new
                  color = Color.new(12, 34, 56, 255)
                  font.name = "power green"
                  font.size = 18
                  font.bold = true
                  font.italic = true
                  font.color = color
                  puts [font.name, font.size, font.bold, font.italic,
                        font.color.red, font.color.green, font.color.blue].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "power green,18,true,true,12,34,56\n"
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
    fn rgss_marshaled_values_match_rsxp_tiles_passages_and_tones() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "rgss-values.rb",
                include_bytes!("../tests/fixtures/rgss_values.rb"),
            )
            .unwrap();
        assert_eq!(
            runtime.take_output(),
            include_bytes!("../tests/fixtures/rgss_values.out")
        );
        let data = runtime
            .vm
            .str_new(include_bytes!("../tests/fixtures/rgss_values.marshal"));
        runtime.vm.global_set("$rgss_marshaled", data);
        runtime
            .run_source(
                "rgss-values-load.rb",
                b"$rgss_values = Marshal.load($rgss_marshaled); describe_rgss_values",
            )
            .unwrap();
        assert_eq!(
            runtime.take_output(),
            include_bytes!("../tests/fixtures/rgss_values.out")
        );
        runtime
            .run_source(
                "table-roundtrip.rb",
                br#"
          original = $rgss_values[0]
          bytes = original._dump(0)
          restored = Table._load(bytes)
          puts [bytes.bytesize, restored[0, 0, 0], restored[1, 0, 0],
                restored[0, 1, 1], restored[1, 1, 1]].join(",")
        "#,
            )
            .unwrap();
        assert_eq!(runtime.take_output(), b"36,384,385,-1,32767\n");
    }

    #[test]
    fn shared_bitmap_snapshots_reuse_pixels_until_the_bitmap_changes() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "shared-tiles.rb",
                br#"
                  $tileset = Bitmap.new(64, 64)
                  128.times do
                    tile = Sprite.new
                    tile.bitmap = $tileset
                    tile.src_rect.set(0, 0, 32, 32)
                  end
                "#,
            )
            .unwrap();
        let first = runtime.render_snapshot();
        let original = first.sprites[0].bitmap_rgba.as_ref().unwrap();
        assert!(
            first
                .sprites
                .iter()
                .all(|sprite| { Arc::ptr_eq(original, sprite.bitmap_rgba.as_ref().unwrap()) })
        );
        let unchanged = runtime.render_snapshot();
        assert!(Arc::ptr_eq(
            original,
            unchanged.sprites[0].bitmap_rgba.as_ref().unwrap()
        ));
        runtime
            .run_source(
                "edit-tiles.rb",
                b"$tileset.set_pixel(0, 0, Color.new(20, 40, 60, 255))",
            )
            .unwrap();
        let changed = runtime.render_snapshot();
        let updated = changed.sprites[0].bitmap_rgba.as_ref().unwrap();
        assert!(!Arc::ptr_eq(original, updated));
        assert_eq!(&original[..4], &[0, 0, 0, 0]);
        assert_eq!(&updated[..4], &[20, 40, 60, 255]);
        assert!(changed.sprites[0].bitmap_revision > first.sprites[0].bitmap_revision);
        assert!(
            changed
                .sprites
                .iter()
                .all(|sprite| { Arc::ptr_eq(updated, sprite.bitmap_rgba.as_ref().unwrap()) })
        );
        let bitmap_id = runtime.vm.global_get("$tileset").obj().unwrap();
        runtime
            .run_source("dispose-tiles.rb", b"$tileset.dispose")
            .unwrap();
        assert!(!state_mut(&mut runtime.vm).bitmaps.contains_key(&bitmap_id));
        assert!(runtime.render_snapshot().sprites.is_empty());
        assert_eq!(&updated[..4], &[20, 40, 60, 255]);
    }

    #[test]
    fn animated_bitmaps_refresh_shared_snapshot_pixels() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "animation.rb",
                b"$animation = Bitmap.new(1, 1); sprite = Sprite.new; sprite.bitmap = $animation",
            )
            .unwrap();
        let bitmap_id = runtime.vm.global_get("$animation").obj().unwrap();
        {
            let bitmap = state_mut(&mut runtime.vm)
                .bitmaps
                .get_mut(&bitmap_id)
                .unwrap();
            bitmap.frames = vec![vec![255, 0, 0, 255], vec![0, 255, 0, 255]];
            bitmap.frame_delays_ms = vec![1, 1];
        }
        runtime
            .run_source("stop.rb", b"$animation.goto_and_stop(0)")
            .unwrap();
        let red = runtime.render_snapshot().sprites[0]
            .bitmap_rgba
            .clone()
            .unwrap();
        runtime
            .run_source("play.rb", b"$animation.goto_and_play(1)")
            .unwrap();
        let green = runtime.render_snapshot().sprites[0]
            .bitmap_rgba
            .clone()
            .unwrap();
        assert_eq!(&*red, &[255, 0, 0, 255]);
        assert_eq!(&*green, &[0, 255, 0, 255]);
        {
            let state = state_mut(&mut runtime.vm);
            state.last_animation_ms = 0;
            state
                .bitmaps
                .get_mut(&bitmap_id)
                .unwrap()
                .animation_elapsed_ms = 1;
        }
        advance_animated_bitmaps(&mut runtime.vm);
        let advanced = runtime.render_snapshot().sprites[0]
            .bitmap_rgba
            .clone()
            .unwrap();
        assert_eq!(&*advanced, &*red);
        assert!(!Arc::ptr_eq(&green, &advanced));
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
    fn cloned_rgss_effect_values_match_rsxp_equality_and_skip_unchanged_tiles() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "value-equality.rb",
                include_bytes!("../tests/fixtures/value_equality.rb"),
            )
            .unwrap();
        assert_eq!(
            runtime.take_output(),
            include_bytes!("../tests/fixtures/value_equality.out")
        );
    }

    #[test]
    fn parsed_fonts_are_shared_but_font_names_and_sizes_still_take_effect() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "font-cache.rb",
                br#"
          $font = Font.new
          $font.name = "power green"
        "#,
            )
            .unwrap();
        let font = runtime.vm.global_get("$font");
        let first = fontdue_font(&mut runtime.vm, font).unwrap();
        let repeated = fontdue_font(&mut runtime.vm, font).unwrap();
        assert!(Arc::ptr_eq(&first, &repeated));
        runtime
            .run_source(
                "font-size.rb",
                b"$font.size = 40; $font.name = ['power green']",
            )
            .unwrap();
        let resized = fontdue_font(&mut runtime.vm, font).unwrap();
        assert!(Arc::ptr_eq(&first, &resized));
        assert!(line_width(&resized, 40.0, "Pokemon") > line_width(&first, 20.0, "Pokemon"));
        runtime
            .run_source(
                "font-family.rb",
                b"$font.name = ['missing font', 'power clear']",
            )
            .unwrap();
        let changed = fontdue_font(&mut runtime.vm, font).unwrap();
        assert!(!Arc::ptr_eq(&first, &changed));
        assert_eq!(state_mut(&mut runtime.vm).fonts.len(), 2);
    }

    #[test]
    fn empty_player_name_can_use_the_games_username_suggestion() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let source = fs::read_to_string(
            project
                .root
                .join("Data/Scripts/058_Utilities/002_Utilities.rb"),
        )
        .unwrap();
        let start = source.find("def pbSuggestTrainerName(gender)").unwrap();
        let end = source[start..].find("def getRandomNameEx").unwrap() + start;
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "name-settings.rb",
                b"module Settings; MAX_PLAYER_NAME_SIZE = 10; end",
            )
            .unwrap();
        runtime
            .run_source("name-suggestion.rb", source[start..end].as_bytes())
            .unwrap();
        runtime.run_source("empty-name.rb",br#"
          name = ""
          name = pbSuggestTrainerName(0) if name.empty?
          puts [System.user_name.is_a?(String), !name.empty?, name.length <= Settings::MAX_PLAYER_NAME_SIZE].join(",")
        "#).unwrap();
        assert_eq!(runtime.take_output(), b"true,true,true\n");
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
    fn legacy_native_surface_is_available_without_a_desktop_abi() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "legacy-native-surface.rb",
                br#"
                  table = Table.new(2, 3, 1)
                  table[1, 2] = 7
                  table.__set(0, 1, 0, 9)
                  payload = table._dump(0)
                  restored = Table._load(payload)
                  api = Win32API.new("user32", "GetForegroundWindow", [], "L")
                  tilemap = Tilemap.new
                  Audio.bgm_set_volume(73)
                  puts [table.xsize, table.ysize, table.zsize, table[1, 2],
                        restored[0, 1], api.Call, tilemap.animation,
                        Audio.bgm_volume].join(",")
                "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "2,3,1,7,9,42,0,73\n"
        );
    }

    #[test]
    fn color_component_writers_reach_native_snapshot_values() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "color-writer.rb",
                br#"color = Color.black; color.alpha = 0; puts color.alpha"#,
            )
            .unwrap();
        assert_eq!(String::from_utf8(runtime.take_output()).unwrap(), "0\n");
    }

    #[test]
    fn sprite_color_mutation_is_preserved_at_render_boundary() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "sprite-color.rb",
                br#"sprite = Sprite.new; sprite.bitmap = Bitmap.new(1, 1); sprite.color = Color.black; sprite.color.alpha = 0;"#,
            )
            .unwrap();
        let snapshot = runtime.render_snapshot();
        assert_eq!(snapshot.sprites[0].color[3], 0);
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
    fn file_mtime_reads_vfs_modification_time_as_time() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open_with_vfs(project, Arc::new(MemoryVfs)).unwrap();
        runtime
            .run_source(
                "mtime.rb",
                br#"m = File.mtime("Data/Scripts/virtual.rb")
puts m.to_i
puts m.usec
puts(m > Time.at(0))
puts File.mtime("Data/Scripts").to_i"#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "1700000000\n123456\ntrue\n0\n"
        );
    }

    #[test]
    fn file_mtime_reads_os_modification_time() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "mtime.rb",
                br#"m = File.mtime("Game.ini")
puts(m.is_a?(Time))
puts(m.to_i > 0)"#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "true\ntrue\n"
        );
    }

    #[test]
    fn deprecated_file_exists_alias_reads_through_the_vfs() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open_with_vfs(project, Arc::new(MemoryVfs)).unwrap();
        runtime
            .run_source(
                "exists.rb",
                br#"puts File.exists?("Data/Scripts/virtual.rb")
puts FileTest.exists?("Data/Scripts/missing.rb")"#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "true\nfalse\n"
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
    fn equal_z_sprites_keep_creation_order_when_their_y_positions_change() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "sprite-order.rb",
                include_bytes!("../tests/fixtures/sprite_order.rb"),
            )
            .unwrap();
        for viewport in [false, true] {
            runtime
                .run_source(
                    "sprite-order-scene.rb",
                    if viewport {
                        b"build_sprite_order_scene(Viewport.new(0, 0, 512, 384))"
                    } else {
                        b"build_sprite_order_scene(nil)"
                    },
                )
                .unwrap();
            let snapshot = runtime.render_snapshot();
            assert_eq!(
                snapshot
                    .sprites
                    .iter()
                    .map(|s| s.bitmap_width)
                    .collect::<Vec<_>>(),
                vec![512, 44, 48, 288, 16]
            );
            runtime
                .run_source("sprite-order-move.rb", b"$order_sprites[1].y = -100")
                .unwrap();
            let snapshot = runtime.render_snapshot();
            assert_eq!(
                snapshot
                    .sprites
                    .iter()
                    .map(|s| s.bitmap_width)
                    .collect::<Vec<_>>(),
                vec![512, 44, 48, 288, 16]
            );
            runtime
                .run_source(
                    "sprite-order-dispose.rb",
                    b"$order_sprites.each { |s| s.dispose }",
                )
                .unwrap();
        }
    }

    #[test]
    fn budget_pauses_during_map_loading_do_not_complete_frames() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .start_source(
                "map-loading.rb",
                br#"
          sprite = Sprite.new
          sprite.bitmap = Bitmap.new(16, 16)
          Graphics.update
          sprite.bitmap = nil
          $loading = true
          500.times { |i| i * i }
          Graphics.frame_count = 0
          sprite.bitmap = Bitmap.new(32, 32)
          $loading = false
          Graphics.update
        "#,
            )
            .unwrap();
        let mut loading_pauses = 0;
        for _ in 0..1000 {
            assert!(matches!(runtime.step(100).unwrap(), Step::Paused));
            if runtime.completed_frames() == 2 {
                break;
            }
            if runtime.vm.global_get("$loading").truthy() {
                loading_pauses += 1;
                assert_eq!(runtime.completed_frames(), 1);
                assert!(runtime.render_snapshot().sprites.is_empty());
            }
        }
        assert!(
            loading_pauses > 1,
            "the load must span multiple host slices"
        );
        assert_eq!(runtime.completed_frames(), 2);
        assert_eq!(runtime.frame_count(), 1, "Ruby reset its frame counter");
        assert_eq!(runtime.render_snapshot().sprites[0].bitmap_width, 32);
    }

    #[test]
    fn viewport_layers_keep_map_sprites_below_pictures_and_naming_ui() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "viewport-layers.rb",
                br#"
          map = Viewport.new(0, 0, 512, 384)
          map.z = 0
          pictures = Viewport.new(0, 0, 512, 384)
          pictures.z = 200
          naming = Viewport.new(0, 0, 512, 384)
          naming.z = 99999
          # Deliberately create sprites in reverse display order.
          ui = Sprite.new(naming)
          ui.bitmap = Bitmap.new(32, 32)
          ui.z = -100
          picture = Sprite.new(pictures)
          picture.bitmap = Bitmap.new(16, 16)
          picture.z = 1
          shadow = Sprite.new(map)
          shadow.bitmap = Bitmap.new(8, 8)
          shadow.z = 9999
        "#,
            )
            .unwrap();
        let snapshot = runtime.render_snapshot();
        assert_eq!(
            snapshot
                .sprites
                .iter()
                .map(|sprite| sprite.bitmap_width)
                .collect::<Vec<_>>(),
            vec![8, 16, 32]
        );
    }

    #[test]
    fn stale_screen_interpolation_endpoints_do_not_raise_nil_arithmetic() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "lerp.rb",
                b"def lerp(start_val, end_val, duration, delta, now=nil); start_val + ((end_val - start_val) * delta / duration.to_f); end;",
            )
            .unwrap();
        runtime.install_script_compat().unwrap();
        runtime
            .run_source(
                "lerp-stale.rb",
                b"puts lerp(nil, 5, 1, 0); puts lerp(3, nil, 1, 0); puts lerp(nil, nil, 0, nil)",
            )
            .unwrap();
        assert_eq!(runtime.take_output(), b"0\n3\n0\n");
    }

    #[test]
    fn viewport_positions_scrolling_and_lifecycle_reach_the_renderer() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "viewport-geometry.rb",
                br#"
          $view = Viewport.new(10, 20, 30, 40)
          $view.ox = 2
          $view.oy = 3
          sprite = Sprite.new($view)
          sprite.bitmap = Bitmap.new(8, 8)
          sprite.x = 5
          sprite.y = 6
        "#,
            )
            .unwrap();
        let snapshot = runtime.render_snapshot();
        assert_eq!(snapshot.sprites.len(), 1);
        assert_eq!((snapshot.sprites[0].x, snapshot.sprites[0].y), (13, 23));
        assert_eq!(snapshot.sprites[0].clip_rect, Some([10, 20, 30, 40]));
        runtime
            .run_source("viewport-hide.rb", b"$view.visible = false")
            .unwrap();
        assert!(runtime.render_snapshot().sprites.is_empty());
        runtime
            .run_source(
                "viewport-dispose.rb",
                b"$view.visible = true; $view.dispose",
            )
            .unwrap();
        assert!(runtime.render_snapshot().sprites.is_empty());
    }

    #[test]
    fn map_transfer_clears_the_cache_without_disposing_retained_bitmaps() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "cache-clear.rb",
                br#"
          bitmap = Bitmap.new(2, 2)
          RPG::Cache.instance_variable_set(:@cache, { "retained" => bitmap })
          old_cache = RPG::Cache.instance_variable_get(:@cache)
          RPG::Cache.clear
          puts RPG::Cache.instance_variable_get(:@cache).empty?
          puts old_cache["retained"].equal?(bitmap)
          puts bitmap.disposed?
          RPG::Cache.clear
          puts RPG::Cache.instance_variable_get(:@cache).empty?
        "#,
            )
            .unwrap();
        assert_eq!(runtime.take_output(), b"true\ntrue\nfalse\ntrue\n");
    }

    #[test]
    fn text_entry_accepts_unicode_edits_and_confirms_with_raw_enter() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime.input_text("ignored before text mode");
        runtime
            .start_source(
                "name-entry.rb",
                br#"
          Input.text_input = true
          name = ""
          loop do
            Graphics.update
            Input.update
            name << Input.gets
            name = name.chars[0...-1].join if Input.triggerex?(:BACKSPACE)
            if Input.triggerex?(:RETURN)
              puts name
              Input.text_input = false
              break
            end
          end
        "#,
            )
            .unwrap();
        assert!(matches!(runtime.step(100_000).unwrap(), Step::Paused));
        assert!(runtime.text_input_enabled());
        runtime.input_text("Ál");
        runtime.input_text("exx\n\r\t");
        assert!(matches!(runtime.step(100_000).unwrap(), Step::Paused));
        runtime.input_press(RAW_KEY_BASE + 42);
        assert!(matches!(runtime.step(100_000).unwrap(), Step::Paused));
        runtime.input_release(RAW_KEY_BASE + 42);
        runtime.input_press(RAW_KEY_BASE + 40);
        assert!(matches!(runtime.step(100_000).unwrap(), Step::Finished(_)));
        assert_eq!(String::from_utf8(runtime.take_output()).unwrap(), "Álex\n");
        assert!(!runtime.text_input_enabled());
        runtime.input_text("ignored after text mode");
        runtime.run_source("text-drained.rb", b"puts Input.gets.empty?; puts Input.clipboard.empty?; Input.clipboard = 'name'; puts Input.clipboard").unwrap();
        assert_eq!(runtime.take_output(), b"true\ntrue\nname\n");
    }

    #[test]
    fn marshal_copies_choice_graphs_and_matches_mri_bytes() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "marshal-choices.rb",
                include_bytes!("../tests/fixtures/marshal_choices.rb"),
            )
            .unwrap();
        assert_eq!(
            runtime.take_output(),
            include_bytes!("../tests/fixtures/marshal_choices.out")
        );
        let graph = runtime.vm.global_get("$marshal_graph");
        let module = runtime.vm.const_path_get("Marshal").unwrap();
        let dump = runtime.vm.intern("dump");
        let data = runtime
            .vm
            .funcall(module, dump, &[graph], Value::Nil)
            .unwrap();
        assert_eq!(
            runtime.vm.str_bytes(data).unwrap(),
            include_bytes!("../tests/fixtures/marshal_choices.marshal")
        );
    }

    #[test]
    fn marshal_io_streams_preserve_object_boundaries_and_depth_limits() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "marshal-io.rb",
                br#"
          class MarshalStream
            attr_accessor :pos
            def initialize
              @data = ""
              @pos = 0
            end
            def write(data)
              @data << data
              @pos = @data.bytesize
              data.bytesize
            end
            def read
              data = @data.byteslice(@pos, @data.bytesize - @pos)
              @pos = @data.bytesize
              data
            end
          end
          stream = MarshalStream.new
          puts Marshal.dump("first", stream).equal?(stream)
          Marshal.dump("second", stream)
          stream.pos = 0
          puts Marshal.load(stream)
          puts Marshal.load(stream)
          puts Marshal.load(Marshal.dump([1], 2)) == [1]
          begin
            Marshal.dump([1], 1)
          rescue ArgumentError
            puts "depth enforced"
          end
          begin
            Marshal.load("invalid")
          rescue ArgumentError
            puts "invalid rejected"
          end
        "#,
            )
            .unwrap();
        assert_eq!(
            runtime.take_output(),
            b"true\nfirst\nsecond\ntrue\ndepth enforced\ninvalid rejected\n"
        );
    }

    #[test]
    fn marshal_deep_copies_real_map_event_commands() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .run_source(
                "marshal-events.rb",
                br#"
          map = load_data("Data/Map001.rxdata")
          commands = map.events[4].pages[0].list
          copy = Marshal.load(Marshal.dump(commands))
          puts copy.length == commands.length
          puts copy[0].class == commands[0].class
          puts copy[0].parameters == commands[0].parameters
          copy[0].parameters << "extra"
          puts copy[0].parameters.length == commands[0].parameters.length + 1
        "#,
            )
            .unwrap();
        assert_eq!(runtime.take_output(), b"true\ntrue\ntrue\ntrue\n");
    }

    #[test]
    fn numeric_event_buttons_select_and_confirm_a_character() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime
            .start_source(
                "input-selection.rb",
                include_bytes!("../tests/fixtures/input_selection.rb"),
            )
            .unwrap();
        assert!(matches!(runtime.step(100_000).unwrap(), Step::Paused));
        runtime.input_press(6);
        assert!(matches!(runtime.step(100_000).unwrap(), Step::Paused));
        runtime.input_release(6);
        runtime.input_press(13);
        assert!(matches!(runtime.step(100_000).unwrap(), Step::Finished(_)));
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            include_str!("../tests/fixtures/input_selection.out")
        );
    }

    #[test]
    fn input_repeat_edges_and_quick_taps() {
        let mut input = InputState::default();
        input.press(6);
        for frame in 1..=36 {
            input.press(6); // Host auto-repeat must not generate new triggers.
            input.update();
            assert_eq!(input.triggered(6), frame == 1);
            assert_eq!(
                input.repeated(6),
                frame == 1 || (frame >= 24 && frame % 6 == 0)
            );
        }
        input.release(6);
        input.update();
        assert!(input.released(6));
        assert!(!input.pressed(6));
        assert!(!input.repeated(6));
        input.update();
        assert!(!input.released(6));
        input.press(13);
        input.release(13); // A whole tap between updates must still trigger once.
        input.update();
        assert!(input.triggered(13));
        assert!(input.released(13));
        assert!(!input.pressed(13));
        assert!(input.repeated(13));
        input.update();
        assert!(!input.triggered(13));
    }

    #[test]
    fn extended_input_distinguishes_physical_keys_from_actions() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime.input_press(13); // Z confirms, but is not the physical C or Enter key.
        runtime.input_press(RAW_KEY_BASE + 29);
        runtime
            .run_source(
                "raw-input.rb",
                br#"
          Input.update
          puts Input.trigger?(:C)
          puts Input.triggerex?(:Z)
          puts Input.pressex?(29)
          puts Input.pressex?(:C)
          puts Input.pressex?(:RETURN)
          Input.update
          puts Input.repeatex?(:Z)
          puts Input.press?(nil)
        "#,
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "true\ntrue\ntrue\nfalse\nfalse\nfalse\nfalse\n"
        );
        runtime.input_release(RAW_KEY_BASE + 29);
        runtime
            .run_source("raw-release.rb", b"Input.update; puts Input.releaseex?(:Z)")
            .unwrap();
        assert_eq!(runtime.take_output(), b"true\n");
    }

    #[test]
    fn opposite_directions_cancel_and_diagonals_match_the_keypad() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        for (codes, expected) in [
            ([2, 4], "2,1\n"),
            ([8, 6], "6,9\n"),
            ([2, 8], "0,0\n"),
            ([4, 6], "0,0\n"),
        ] {
            for code in codes {
                runtime.input_press(code);
            }
            runtime
                .run_source(
                    "directions.rb",
                    b"Input.update; puts [Input.dir4, Input.dir8].join(',')",
                )
                .unwrap();
            assert_eq!(String::from_utf8(runtime.take_output()).unwrap(), expected);
            for code in codes {
                runtime.input_release(code);
            }
        }
    }

    #[test]
    fn input_symbols_and_direction_use_host_events() {
        let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
        let mut runtime = RgssRuntime::open(project).unwrap();
        runtime.input_press(4);
        runtime
            .run_source(
                "input.rb",
                b"Input.update; puts Input.press?(:LEFT); puts Input.dir4; puts Input.dir8",
            )
            .unwrap();
        assert_eq!(
            String::from_utf8(runtime.take_output()).unwrap(),
            "true\n4\n4\n"
        );
    }
}
