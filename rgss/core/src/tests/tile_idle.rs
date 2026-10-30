use super::*;

fn runtime(optimized: bool) -> RgssRuntime {
    let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
    let renderer = fs::read(
        project
            .root()
            .join("Data/Scripts/016_Map renderer/002_TilemapRenderer.rb"),
    )
    .unwrap();
    let mut runtime = RgssRuntime::open(project).unwrap();
    runtime
        .run_source(
            "Game_Map",
            include_bytes!("../../tests/fixtures/tile_scene_setup.rb"),
        )
        .unwrap();
    runtime.run_source("TilemapRenderer", &renderer).unwrap();
    if optimized {
        assert!(rgss_optimizations::tile_idle::install(&mut runtime.vm).unwrap());
        runtime
            .run_source("tile-dispatch", rgss_optimizations::tile_idle::DISPATCH)
            .unwrap();
    }
    runtime
}

#[test]
#[ignore = "requires the local Anil compatibility project"]
fn unchanged_tiles_match_ruby_through_edits_scrolling_effects_animation_and_gc() {
    let mut reference = runtime(false);
    let mut optimized = runtime(true);
    let source = include_bytes!("../../tests/fixtures/tile_scene_steps.rb");
    reference.run_source("tile-scene", source).unwrap();
    optimized.run_source("tile-scene", source).unwrap();
    assert_eq!(optimized.take_output(), reference.take_output());
    let before = reference.vm.instructions;
    reference
        .run_source("repeat", b"$tile_renderer.update")
        .unwrap();
    let reference_instructions = reference.vm.instructions - before;
    let before = optimized.vm.instructions;
    optimized
        .run_source("repeat", b"$tile_renderer.update")
        .unwrap();
    assert!(
        optimized.vm.instructions - before < reference_instructions / 2,
        "unchanged frames must bypass tile traversal"
    );
}

#[test]
#[ignore = "requires the local Anil compatibility project"]
fn monkey_patched_helpers_fall_back_to_resumable_ruby() {
    let mut runtime = runtime(true);
    runtime
        .run_source(
            "tile-scene",
            include_bytes!("../../tests/fixtures/tile_scene_steps.rb"),
        )
        .unwrap();
    runtime.take_output();
    runtime
        .run_source(
            "custom-helper",
            br#"
      class TilemapRenderer
        alias old_check_for_test check_if_screen_moved
        def check_if_screen_moved
          Graphics.update
          old_check_for_test
        end
      end
    "#,
        )
        .unwrap();
    runtime
        .start_source("custom-frame", b"$tile_renderer.update; puts 'finished'")
        .unwrap();
    assert!(matches!(runtime.step(100_000).unwrap(), Step::Paused));
    assert!(runtime.take_output().is_empty());
    assert!(matches!(runtime.step(100_000).unwrap(), Step::Finished(_)));
    assert_eq!(runtime.take_output(), b"finished\n");
}

#[test]
#[ignore = "requires the local Anil compatibility project"]
fn singleton_helpers_keep_running_on_unchanged_frames() {
    let mut runtime = runtime(true);
    runtime
        .run_source(
            "tile-scene",
            include_bytes!("../../tests/fixtures/tile_scene_steps.rb"),
        )
        .unwrap();
    runtime.take_output();
    runtime
        .run_source(
            "singleton-helper",
            br#"
      def $tile_renderer.check_if_screen_moved
        $custom_checks = ($custom_checks || 0) + 1
        super
      end
      3.times { $tile_renderer.update }
      puts $custom_checks
    "#,
        )
        .unwrap();
    assert_eq!(runtime.take_output(), b"3\n");
}

#[test]
#[ignore = "requires the local Anil compatibility project"]
fn unrelated_runtime_defs_and_map_singletons_do_not_disable_the_fast_path() {
    let mut runtime = runtime(true);
    runtime
        .run_source(
            "tile-scene",
            include_bytes!("../../tests/fixtures/tile_scene_steps.rb"),
        )
        .unwrap();
    runtime.take_output();
    runtime
        .run_source(
            "mutations",
            br#"
      def __sabiruby_unrelated_runtime_method; end
      $game_map.instance_eval { @map }
      $tile_renderer.update
      puts $tile_renderer.__sabiruby_tiles_unchanged?(System.uptime)
    "#,
        )
        .unwrap();
    assert_eq!(runtime.take_output(), b"true\n");
}

#[test]
fn render_registry_prunes_disposed_objects_without_duplicate_registration() {
    let project = Project::open(concat!(env!("CARGO_MANIFEST_DIR"), "/../../anil")).unwrap();
    let mut runtime = RgssRuntime::open(project).unwrap();
    runtime
        .run_source(
            "registry",
            br#"
      $sprite = Sprite.new
      $sprite.send(:initialize)
      500.times { Sprite.new.dispose }
      puts Sprite.instance_variable_get(:@__sabiruby_instances).length
      $sprite.dispose
      $sprite.dispose
      puts Sprite.instance_variable_get(:@__sabiruby_instances).length
      $sprite.send(:initialize)
      puts Sprite.instance_variable_get(:@__sabiruby_instances).length
      class CustomSprite < Sprite
        def x; super + 3; end
        def x=(value); super(value * 2); end
      end
      sprite = CustomSprite.new
      sprite.x = 7
      sprite.custom_property = 11
      puts sprite.x
      puts sprite.custom_property
    "#,
        )
        .unwrap();
    assert_eq!(runtime.take_output(), b"1\n0\n1\n17\n11\n");
}
