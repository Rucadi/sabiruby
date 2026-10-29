//! Ruby-facing RGSS native bindings.
//!
//! This module is the registration boundary for every native method. Keeping
//! the boundary in the core (rather than in the WGPU/player crates) lets an
//! embedding host install the same VFS, graphics, input, audio and media
//! contract with a different frontend.

use super::*;

mod compat;
pub(super) mod idle;
mod marshal;
mod prelude;
mod tilemap;
mod win32;

pub(super) fn install(vm: &mut Vm, project: &Project, vfs: Arc<dyn VirtualFileSystem>) {
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
            ("__sabiruby_play_movie", graphics_play_movie),
            ("__sabiruby_movie_active?", graphics_movie_active),
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
            ("is_windows?", system_is_windows),
            ("user_language", system_user_language),
            ("user_name", system_user_name),
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
            ("triggerex?", input_trigger_ex),
            ("pressex?", input_press_ex),
            ("repeatex?", input_repeat_ex),
            ("releaseex?", input_release_ex),
            ("dir4", input_dir4),
            ("dir8", input_dir8),
            ("text_input", input_text_mode),
            ("text_input=", input_set_text_mode),
            ("gets", input_gets),
            ("mouse_x", zero_value),
            ("mouse_y", zero_value),
            ("mouse_in_window?", true_value),
            ("clipboard", input_clipboard),
            ("clipboard=", input_set_clipboard),
        ],
    );
    let _ = input;
    marshal::install(vm);
    let audio = install_module(
        vm,
        "Audio",
        &[
            ("bgm_play", bgm_play),
            ("bgm_stop", bgm_stop),
            ("bgm_fade", noop),
            ("bgm_pos", zero_value),
            ("bgm_volume", audio_bgm_volume),
            ("bgm_set_volume", audio_bgm_set_volume),
            ("bgs_play", bgs_play),
            ("bgs_stop", bgs_stop),
            ("bgs_fade", noop),
            ("bgs_pos", zero_value),
            ("me_play", me_play),
            ("me_stop", me_stop),
            ("me_fade", noop),
            ("se_play", se_play),
            ("se_stop", se_stop),
            ("setup_midi", noop),
            ("__reset__", audio_reset),
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
            vm.define_method(class_id, "dispose", object_dispose);
            vm.define_method(class_id, "disposed?", object_disposed);
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
        if class == "Table" {
            vm.define_method(class_id, "initialize", table_initialize);
            vm.define_method(class_id, "__initialize", table_initialize);
            vm.define_method(class_id, "__get", table_get);
            vm.define_method(class_id, "__set", table_set);
            vm.define_method(class_id, "__resize", table_resize);
            vm.define_method(class_id, "__dimensions", table_dimensions);
            vm.define_method(class_id, "__fanplay_revision", table_revision);
            vm.define_method(class_id, "_dump", table_dump);
            install_class_function(vm, class_id, "_load", table_load);
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
            vm.define_method(class_id, "==", rect_equal);
            vm.define_method(class_id, "===", rect_equal);
            vm.define_method(class_id, "initialize", rect_initialize);
            vm.define_method(class_id, "x", rect_x);
            vm.define_method(class_id, "y", rect_y);
            vm.define_method(class_id, "width", rect_width);
            vm.define_method(class_id, "height", rect_height);
            vm.define_method(class_id, "set", rect_set);
            vm.define_method(class_id, "empty?", rect_empty);
        }
        if class == "Color" {
            vm.define_method(class_id, "==", color_equal);
            vm.define_method(class_id, "===", color_equal);
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
            vm.define_method(class_id, "==", tone_equal);
            vm.define_method(class_id, "===", tone_equal);
            vm.define_method(class_id, "initialize", tone_initialize);
            vm.define_method(class_id, "red", tone_red);
            vm.define_method(class_id, "green", tone_green);
            vm.define_method(class_id, "blue", tone_blue);
            vm.define_method(class_id, "gray", tone_gray);
        }
        if class == "Font" {
            vm.define_method(class_id, "initialize", font_initialize);
            vm.define_method(class_id, "name", font_name);
            vm.define_method(class_id, "name=", font_name_set);
            vm.define_method(class_id, "size", font_size);
            vm.define_method(class_id, "size=", font_size_set);
            vm.define_method(class_id, "bold", font_bold);
            vm.define_method(class_id, "bold=", font_bold_set);
            vm.define_method(class_id, "italic", font_italic);
            vm.define_method(class_id, "italic=", font_italic_set);
            vm.define_method(class_id, "outline", font_outline);
            vm.define_method(class_id, "outline=", font_outline_set);
            vm.define_method(class_id, "shadow", font_shadow);
            vm.define_method(class_id, "shadow=", font_shadow_set);
            vm.define_method(class_id, "color", font_color);
            vm.define_method(class_id, "color=", font_color_set);
            vm.define_method(class_id, "out_color", font_out_color);
            vm.define_method(class_id, "out_color=", font_out_color_set);
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
            vm.define_method(class_id, "mega?", bitmap_mega);
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
    let tilemap_class = install_class(vm, "Tilemap");
    vm.define_method(tilemap_class, "initialize", tilemap::initialize);
    vm.define_method(tilemap_class, "update", tilemap::update);
    vm.define_method(tilemap_class, "refresh", tilemap::refresh);
    vm.define_method(tilemap_class, "dispose", object_dispose);
    vm.define_method(tilemap_class, "disposed?", object_disposed);
    install_class_function(vm, tilemap_class, "__sync_all", tilemap::sync_all);

    win32::install(vm);
    compat::install(vm);
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
        ("mtime", file_mtime),
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
    prelude::install(vm).expect("RGSS compatibility prelude");
}
