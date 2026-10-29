//! Unchanged-frame fast path for the standard Essentials tile renderer.
//! All changed frames still run its original Ruby implementation, outside any
//! native call, so Graphics.update and extensions retain normal suspension.
use super::*;
use sabiruby::object::MethodRef;
use sabiruby::symbol::Sym;
use sabiruby::value::Slot;

macro_rules! keys {
    ($($field:ident => $name:literal),* $(,)?) => {
        struct Keys { $($field: Sym,)* layouts: Vec<ObjId> }
        impl Keys {
            fn new(vm: &mut Vm) -> Self { Self { $($field:vm.intern($name),)* layouts: ["TilemapRenderer", "TilemapRenderer::TilesetBitmaps", "TilemapRenderer::AutotileBitmaps", "TilemapRenderer::TileSprite", "Game_Map", "PokemonMapFactory"].into_iter().map(|path|vm.const_path_get(path).and_then(Value::obj).expect("validated tile layout")).collect() } }
            #[inline]
            fn get(&self, vm: &Vm, object: ObjId, name: &str) -> Value {
                let key = match name { $($name => self.$field,)* _ => unreachable!("unknown tile-cache field") };
                vm.heap.ivar_get(object,key)
            }
        }
    }
}

keys! {
    signature=>"@__sabiruby_tile_signature", guard_signature=>"@__sabiruby_tile_guard_signature", roots=>"@__sabiruby_tile_guards",
    tiles=>"@tiles", tilesets=>"@tilesets", autotiles=>"@autotiles", viewport=>"@viewport",
    rect=>"@rect", ox=>"@ox", oy=>"@oy", x=>"@x", y=>"@y", width=>"@width", height=>"@height",
    tone=>"@tone", old_tone=>"@old_tone", color=>"@color", old_color=>"@old_color",
    red=>"@red", green=>"@green", blue=>"@blue", gray=>"@gray", alpha=>"@alpha",
    need_refresh=>"@need_refresh", tile_id=>"@tile_id", bitmap=>"@bitmap", animated=>"@animated",
    horizontal=>"@tiles_horizontal_count", vertical=>"@tiles_vertical_count",
    current_map=>"@current_map_id", tile_x=>"@tile_offset_x", tile_y=>"@tile_offset_y",
    pixel_x=>"@pixel_offset_x", pixel_y=>"@pixel_offset_y", bridge=>"@bridge",
    map=>"@map", maps=>"@maps", map_id=>"@map_id", display_x=>"@display_x", display_y=>"@display_y",
    data=>"@data", revision=>"@__sabiruby_revision", tileset_id=>"@tileset_id",
    changed=>"@changed", bitmaps=>"@bitmaps", wraps=>"@bitmap_wraps", counts=>"@frame_counts",
    durations=>"@frame_durations", frames=>"@current_frames", timer=>"@timer_start",
    disposed=>"@disposed", disabled=>"@autotile_animations",
    old_ox=>"@old_viewport_ox", old_oy=>"@old_viewport_oy",
    screen_moved=>"@screen_moved", screen_vertical=>"@screen_moved_vertically",
    last_display_x=>"@__sabiruby_last_display_x", last_display_y=>"@__sabiruby_last_display_y",
}

fn array(vm: &Vm, value: Value) -> Option<&[Slot]> {
    match &vm.heap.get(value.obj()?).kind {
        ObjKind::Array(values) => Some(values.as_slice()),
        _ => None,
    }
}

fn plain_reader(vm: &Vm, receiver: ObjId, method: &str, ivar: Sym) -> bool {
    let Some(name) = vm.syms.lookup_str(method) else {
        return false;
    };
    matches!(vm.find_method_ref(vm.class_of(Value::Obj(receiver)),name),Some((MethodRef::AttrReader(key),_)) if key==ivar)
}

fn method_matches(left: MethodRef, right: MethodRef) -> bool {
    match (left, right) {
        (MethodRef::Ruby(a), MethodRef::Ruby(b)) => a == b,
        (MethodRef::AttrReader(a), MethodRef::AttrReader(b)) => a == b,
        (MethodRef::Native(a, aa), MethodRef::Native(b, bb)) => {
            core::ptr::fn_addr_eq(a, b) && aa == bb
        }
        _ => false,
    }
}

#[derive(Clone, Copy)]
enum GuardTarget {
    Renderer,
    Tilesets,
    Autotiles,
    Map,
    Factory,
}

struct Guard {
    target: GuardTarget,
    method: Sym,
    expected: MethodRef,
}

/// Install only against the standard renderer and its pure helper methods.
pub(crate) fn install(vm: &mut Vm) -> Result<bool, VmError> {
    let Some(class) = vm.const_path_get("TilemapRenderer").and_then(Value::obj) else {
        return Ok(false);
    };
    if vm
        .const_path_get("TilemapRenderer::TileSprite")
        .and_then(Value::obj)
        .is_none()
    {
        return Ok(false);
    }
    let ready = vm.intern("__sabiruby_tiles_unchanged?");
    if vm.find_method_ref(class, ready).is_some() {
        return Ok(false);
    }
    let mut guards = Vec::new();
    let mut roots = Vec::new();
    for (path, names, source, target) in [
        (
            "TilemapRenderer",
            &[
                "update",
                "check_if_screen_moved",
                "refresh_tile",
                "refresh_tile_frame",
                "refresh_tile_coordinates",
                "refresh_tile_z",
            ][..],
            "TilemapRenderer",
            GuardTarget::Renderer,
        ),
        (
            "TilemapRenderer::TilesetBitmaps",
            &["update"][..],
            "TilemapRenderer",
            GuardTarget::Tilesets,
        ),
        (
            "TilemapRenderer::AutotileBitmaps",
            &["update", "set_current_frame", "frame_count"][..],
            "TilemapRenderer",
            GuardTarget::Autotiles,
        ),
        (
            "Game_Map",
            &[
                "data",
                "width",
                "height",
                "display_x",
                "display_y",
                "map_id",
                "tileset_id",
                "autotile_names",
                "priorities",
                "terrain_tags",
            ][..],
            "Game_Map",
            GuardTarget::Map,
        ),
        (
            "PokemonMapFactory",
            &["maps"][..],
            "Game_MapFactory",
            GuardTarget::Factory,
        ),
    ] {
        let Some(owner) = vm.const_path_get(path).and_then(Value::obj) else {
            return Ok(false);
        };
        roots.push(Value::Obj(owner));
        for name in names {
            let method = vm.intern(name);
            let Some((expected, _)) = vm.find_method_ref(owner, method) else {
                return Ok(false);
            };
            match expected {
                MethodRef::Ruby(proc) => {
                    let irep = vm.heap.proc_data(proc).irep;
                    if !vm.ireps[irep]
                        .filename
                        .as_deref()
                        .is_some_and(|file| file.contains(source))
                    {
                        return Ok(false);
                    }
                    roots.push(Value::Obj(proc));
                }
                MethodRef::AttrReader(_) => {}
                _ => return Ok(false),
            }
            // update is intentionally replaced by the small Ruby dispatcher.
            if path != "TilemapRenderer" || *name != "update" {
                guards.push(Guard {
                    target,
                    method,
                    expected,
                });
            }
        }
    }
    let original = vm.intern("__sabiruby_original_tile_update");
    let update = vm.intern("update");
    vm.alias_method(class, original, update)?;
    let keys = Arc::new(Keys::new(vm));
    roots.extend(keys.layouts.iter().copied().map(Value::Obj));
    let root_array = vm.ary_new(roots);
    vm.heap.ivar_set(class, keys.roots, root_array);
    let read_keys = keys.clone();
    vm.define_closure(
        class,
        "__sabiruby_tiles_unchanged?",
        move |vm, receiver, args, _| {
            let Some(object) = receiver.obj() else {
                return Ok(Value::False);
            };
            if read_keys.get(vm, class, "@__sabiruby_tile_guards").is_nil() {
                return Ok(Value::False);
            }
            let guard_class = |target: GuardTarget| -> Option<ObjId> {
                match target {
                    GuardTarget::Renderer => Some(vm.class_of(Value::Obj(object))),
                    GuardTarget::Tilesets => read_keys
                        .get(vm, object, "@tilesets")
                        .obj()
                        .map(|value| vm.class_of(Value::Obj(value))),
                    GuardTarget::Autotiles => read_keys
                        .get(vm, object, "@autotiles")
                        .obj()
                        .map(|value| vm.class_of(Value::Obj(value))),
                    GuardTarget::Map => vm
                        .global_get("$game_map")
                        .obj()
                        .map(|value| vm.class_of(Value::Obj(value))),
                    GuardTarget::Factory => vm
                        .global_get("$map_factory")
                        .obj()
                        .map(|value| vm.class_of(Value::Obj(value))),
                }
            };
            for guard in &guards {
                let Some(class) = guard_class(guard.target) else {
                    return Ok(Value::False);
                };
                if !vm
                    .find_method_ref(class, guard.method)
                    .is_some_and(|(current, _)| method_matches(current, guard.expected))
                {
                    return Ok(Value::False);
                }
            }
            let Some(expected) = array(vm, read_keys.get(vm, object, "@__sabiruby_tile_signature"))
            else {
                return Ok(Value::False);
            };
            let mut cursor = 0;
            let guard_valid = array(
                vm,
                read_keys.get(vm, object, "@__sabiruby_tile_guard_signature"),
            )
            .and_then(|expected| {
                let mut guard_cursor = 0;
                visit(
                    vm,
                    &read_keys,
                    object,
                    args.first().copied(),
                    true,
                    false,
                    false,
                    &mut |value| {
                        let same = expected
                            .get(guard_cursor)
                            .is_some_and(|slot| slot.get() == value);
                        guard_cursor += 1;
                        same
                    },
                )
                .is_some()
                .then_some(guard_cursor == expected.len())
            })
            .unwrap_or(false);
            if guard_valid {
                let current_map = vm.global_get("$game_map").obj();
                let current_x = current_map
                    .map(|map| read_keys.get(vm, map, "@display_x"))
                    .unwrap_or(Value::Nil);
                let current_y = current_map
                    .map(|map| read_keys.get(vm, map, "@display_y"))
                    .unwrap_or(Value::Nil);
                let moved = current_x != read_keys.get(vm, object, "@__sabiruby_last_display_x")
                    || current_y != read_keys.get(vm, object, "@__sabiruby_last_display_y");
                if moved {
                    return Ok(Value::False);
                }
                vm.heap
                    .ivar_set(object, read_keys.screen_moved, Value::False);
                vm.heap
                    .ivar_set(object, read_keys.screen_vertical, Value::False);
                if let Some(state) = vm.host_state_mut::<RgssState>() {
                    state.tile_fast_hits = state.tile_fast_hits.saturating_add(1);
                }
                return Ok(Value::True);
            }
            let valid = visit(
                vm,
                &read_keys,
                object,
                args.first().copied(),
                true,
                true,
                true,
                &mut |value| {
                    let same = expected.get(cursor).is_some_and(|slot| slot.get() == value);
                    cursor += 1;
                    same
                },
            )
            .is_some()
                && cursor == expected.len();
            if valid {
                vm.heap
                    .ivar_set(object, read_keys.screen_moved, Value::False);
                vm.heap
                    .ivar_set(object, read_keys.screen_vertical, Value::False);
                if let Some(state) = vm.host_state_mut::<RgssState>() {
                    state.tile_fast_hits = state.tile_fast_hits.saturating_add(1);
                }
            } else if let Some(state) = vm.host_state_mut::<RgssState>() {
                state.tile_fast_misses = state.tile_fast_misses.saturating_add(1);
            }
            Ok(Value::bool(valid))
        },
    );
    vm.define_closure(
        class,
        "__sabiruby_record_tiles",
        move |vm, receiver, _, _| {
            let object = object_instance(vm, receiver)?;
            // The VM-wide method serial is not a usable guard here: any runtime
            // `def` (menus, event handlers, plugins) bumps it and would deopt the
            // renderer forever. `__sabiruby_tiles_unchanged?` checks the specific
            // renderer helpers instead.
            let mut values = Vec::with_capacity(3000);
            let valid = visit(vm, &keys, object, None, false, true, true, &mut |value| {
                values.push(value);
                true
            })
            .is_some();
            let signature = if valid {
                vm.ary_new(values)
            } else {
                Value::Nil
            };
            vm.heap.ivar_set(object, keys.signature, signature);
            let mut guard_values = Vec::with_capacity(256);
            let guard_valid = visit(vm, &keys, object, None, false, false, false, &mut |value| {
                guard_values.push(value);
                true
            })
            .is_some();
            let guard_signature = if guard_valid {
                vm.ary_new(guard_values)
            } else {
                Value::Nil
            };
            vm.heap
                .ivar_set(object, keys.guard_signature, guard_signature);
            if let Some(map) = vm.global_get("$game_map").obj() {
                vm.heap
                    .ivar_set(object, keys.last_display_x, keys.get(vm, map, "@display_x"));
                vm.heap
                    .ivar_set(object, keys.last_display_y, keys.get(vm, map, "@display_y"));
            }
            Ok(Value::Nil)
        },
    );
    Ok(true)
}

fn visit(
    vm: &Vm,
    keys: &Keys,
    object: ObjId,
    now: Option<Value>,
    check_animation: bool,
    include_tile_state: bool,
    include_view_position: bool,
    emit: &mut impl FnMut(Value) -> bool,
) -> Option<()> {
    macro_rules! out {
        ($v:expr) => {
            if !emit($v) {
                return None;
            }
        };
    }
    macro_rules! fields { ($o:expr; $($name:literal),* $(,)?) => { $(out!(keys.get(vm,$o,$name));)* }; }
    if !vm.obj_is_kind_of(Value::Obj(object), keys.layouts[0]) {
        return None;
    }
    if keys.get(vm, object, "@need_refresh").truthy() {
        return None;
    }
    fields!(object;"@tiles","@tiles_horizontal_count","@tiles_vertical_count","@current_map_id","@bridge");
    if include_view_position {
        fields!(object;"@tile_offset_x","@tile_offset_y","@pixel_offset_x","@pixel_offset_y","@old_viewport_ox","@old_viewport_oy");
    }
    let Some(state) = vm.host_state::<RgssState>() else {
        return None;
    };
    out!(Value::Int(state.width as i64));
    out!(Value::Int(state.height as i64));
    for name in ["@tone", "@old_tone", "@color", "@old_color"] {
        let Some(value) = keys.get(vm, object, name).obj() else {
            return None;
        };
        fields!(value;"@red","@green","@blue");
        out!(keys.get(
            vm,
            value,
            if name.contains("tone") {
                "@gray"
            } else {
                "@alpha"
            }
        ));
    }
    let Some(viewport) = keys.get(vm, object, "@viewport").obj() else {
        return None;
    };
    out!(Value::Obj(viewport));
    fields!(viewport;"@ox","@oy","@disposed");
    let Some(rect) = keys.get(vm, viewport, "@rect").obj() else {
        return None;
    };
    fields!(rect;"@x","@y","@width","@height");
    let Some(global) = vm.global_get("$PokemonGlobal").obj() else {
        return None;
    };
    if !plain_reader(vm, global, "bridge", keys.bridge) {
        return None;
    }
    fields!(global;"@bridge");
    let Some(settings) = vm.global_get("$PokemonSystem").obj() else {
        return None;
    };
    if !plain_reader(vm, settings, "autotile_animations", keys.disabled) {
        return None;
    }
    let disabled = keys.get(vm, settings, "@autotile_animations");
    out!(disabled);
    let Some(current) = vm.global_get("$game_map").obj() else {
        return None;
    };
    out!(Value::Obj(current));
    fields!(current;"@map_id");
    if include_view_position {
        fields!(current;"@display_x","@display_y");
    }
    if !vm.obj_is_kind_of(Value::Obj(current), keys.layouts[4]) {
        return None;
    }
    let Some(factory) = vm.global_get("$map_factory").obj() else {
        return None;
    };
    if !vm.obj_is_kind_of(Value::Obj(factory), keys.layouts[5]) {
        return None;
    }
    let Some(maps) = array(vm, keys.get(vm, factory, "@maps")) else {
        return None;
    };
    out!(Value::Int(maps.len() as i64));
    for map in maps {
        let Some(map) = map.get().obj() else {
            return None;
        };
        if !vm.obj_is_kind_of(Value::Obj(map), keys.layouts[4]) {
            return None;
        }
        out!(Value::Obj(map));
        fields!(map;"@map_id");
        if include_view_position {
            fields!(map;"@display_x","@display_y");
        }
        let Some(data_map) = keys.get(vm, map, "@map").obj() else {
            return None;
        };
        out!(Value::Obj(data_map));
        fields!(data_map;"@width","@height","@tileset_id");
        let Some(table) = keys.get(vm, data_map, "@data").obj() else {
            return None;
        };
        out!(Value::Obj(table));
        fields!(table;"@data","@__sabiruby_revision");
    }
    for name in ["@tilesets", "@autotiles"] {
        let Some(bitmaps) = keys.get(vm, object, name).obj() else {
            return None;
        };
        if !vm.obj_is_kind_of(
            Value::Obj(bitmaps),
            keys.layouts[if name == "@tilesets" { 1 } else { 2 }],
        ) {
            return None;
        }
        out!(Value::Obj(bitmaps));
        fields!(bitmaps;"@bridge","@changed");
        let Some(entries) = vm.hash_entries(keys.get(vm, bitmaps, "@bitmaps")) else {
            return None;
        };
        out!(Value::Int(entries.len() as i64));
        for (filename, bitmap) in entries {
            out!(filename);
            out!(bitmap);
            let Some(bitmap) = bitmap.obj() else {
                return None;
            };
            out!(keys.get(vm, bitmap, "@disposed"));
        }
        if name == "@autotiles" {
            for field in [
                "@frame_counts",
                "@frame_durations",
                "@current_frames",
                "@bitmap_wraps",
            ] {
                let Some(entries) = vm.hash_entries(keys.get(vm, bitmaps, field)) else {
                    return None;
                };
                out!(Value::Int(entries.len() as i64));
                for (key, value) in entries {
                    out!(key);
                    out!(value);
                }
            }
            let timer = keys.get(vm, bitmaps, "@timer_start");
            out!(timer);
            if check_animation && disabled != Value::Int(1) {
                let Some(now) = now else {
                    return None;
                };
                let time = value_f64(now, f64::NAN);
                if !time.is_finite() {
                    return None;
                }
                let Some(durations) = vm.hash_entries(keys.get(vm, bitmaps, "@frame_durations"))
                else {
                    return None;
                };
                let Some(frames) = vm.hash_entries(keys.get(vm, bitmaps, "@current_frames")) else {
                    return None;
                };
                let Some(counts) = vm.hash_entries(keys.get(vm, bitmaps, "@frame_counts")) else {
                    return None;
                };
                for (key, count) in counts {
                    let count = value_i64(count, 0);
                    if count < 2 {
                        continue;
                    }
                    let Some(source) = vm.str_bytes(key) else {
                        return None;
                    };
                    let Some(duration) = durations
                        .iter()
                        .find(|(key, _)| vm.str_bytes(*key) == Some(source))
                        .map(|(_, v)| value_f64(*v, 0.0))
                    else {
                        return None;
                    };
                    if duration <= 0.0 {
                        return None;
                    }
                    let frame = ((time - value_f64(timer, 0.0)) / duration).floor() as i64;
                    let Some(old) = frames
                        .iter()
                        .find(|(key, _)| vm.str_bytes(*key) == Some(source))
                        .map(|(_, v)| value_i64(*v, -1))
                    else {
                        return None;
                    };
                    if frame.rem_euclid(count) != old {
                        return None;
                    }
                }
            }
        }
    }
    if !include_tile_state {
        return Some(());
    }
    let Some(columns) = array(vm, keys.get(vm, object, "@tiles")) else {
        return None;
    };
    out!(Value::Int(columns.len() as i64));
    for col in columns {
        out!(col.get());
        let Some(rows) = array(vm, col.get()) else {
            return None;
        };
        out!(Value::Int(rows.len() as i64));
        for row in rows {
            out!(row.get());
            let Some(tiles) = array(vm, row.get()) else {
                return None;
            };
            out!(Value::Int(tiles.len() as i64));
            for tile in tiles {
                out!(tile.get());
                if !vm.obj_is_kind_of(tile.get(), keys.layouts[3]) {
                    return None;
                }
                let Some(tile) = tile.get().obj() else {
                    return None;
                };
                // `@bitmap` and `@animated` are written only by
                // `refresh_tile_bitmap`, together with `@tile_id`.
                fields!(tile;"@need_refresh","@tile_id");
            }
        }
    }
    Some(())
}

pub(crate) const DISPATCH: &[u8] = br#"
class TilemapRenderer
  def update
    return false if __sabiruby_tiles_unchanged?(System.uptime)
    result = __sabiruby_original_tile_update
    __sabiruby_record_tiles
    result
  end
end
"#;
