use ignit_ui::{UiKey, UiModifiers, UiTextMeasure, UiTextStyle, UiTextWrap};
use ignit_ui_text::{FontStack, TextEditState, TextEngine, UiClipboard};

fn style(size: f32) -> UiTextStyle {
    UiTextStyle {
        size,
        ..Default::default()
    }
}

#[test]
fn shaping_wraps_and_caches() {
    let mut engine = TextEngine::new();
    let text = "The quick brown fox jumps over the lazy dog";
    let first = engine.shape(text, &style(16.0), 120.0);
    assert!(first.lines.len() > 1);
    assert!(first.width <= 121.0);
    assert!(first.height > 0.0);
    let misses = engine.stats().cache_misses;
    let second = engine.shape(text, &style(16.0), 120.0);
    assert_eq!(engine.stats().cache_misses, misses);
    assert!(engine.stats().cache_hits > 0);
    assert_eq!(first.width, second.width);
}

#[test]
fn measurement_tracks_scale_and_width() {
    let mut engine = TextEngine::new();
    let text = "Scale-aware UI text";
    let measured = engine.measure(text, &style(18.0), Some(140.0));
    assert!(measured.width <= 141.0);
    assert!(measured.height > 0.0);
    let before = engine.stats().cache_misses;
    engine.set_scale(2.0);
    let _ = engine.measure(text, &style(18.0), Some(140.0));
    assert!(engine.stats().cache_misses > before);
}

#[test]
fn wrapping_modes_differ() {
    let mut engine = TextEngine::new();
    let mut no_wrap = style(16.0);
    no_wrap.wrap = UiTextWrap::NoWrap;
    let shaped = engine.shape("one two three four", &no_wrap, 60.0);
    assert_eq!(shaped.lines.len(), 1);

    let word = style(16.0);
    let shaped = engine.shape("one two three four", &word, 60.0);
    assert!(shaped.lines.len() > 1);
}

#[test]
fn quads_and_atlas_pages() {
    let mut engine = TextEngine::with_atlas(FontStack::default(), 64, 2);
    engine.begin_frame();
    let shaped = engine.shape("Atlas glyph coverage 0123456789", &style(16.0), 200.0);
    let quads = engine.quads(&shaped);
    assert!(!quads.is_empty());
    for quad in &quads {
        assert!(quad.uv[2] >= quad.uv[0]);
        assert!(quad.uv[3] >= quad.uv[1]);
        assert!(quad.rect[2] > 0.0);
        assert!(quad.rect[3] > 0.0);
    }
    assert!(!engine.atlas.take_dirty_pages().is_empty());
}

#[test]
fn atlas_evicts_when_full() {
    let mut engine = TextEngine::with_atlas(FontStack::default(), 32, 1);
    engine.begin_frame();
    let text = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let shaped = engine.shape(text, &style(18.0), 400.0);
    let _ = engine.quads(&shaped);
    assert!(engine.atlas.evictions > 0);
    assert_eq!(engine.atlas.page_count(), 1);
}

#[test]
fn hit_mapping_selects_nearest_glyph() {
    let mut engine = TextEngine::new();
    let shaped = engine.shape("abcd", &style(24.0), 400.0);
    let glyph = &shaped.glyphs[2];
    let cursor = engine.hit_to_cursor(
        &shaped,
        glyph.rect[0] + glyph.rect[2] * 0.75,
        glyph.rect[1] + glyph.rect[3] * 0.5,
    );
    assert!(cursor >= 2);
    let (x, y) = engine
        .cursor_position(&shaped, cursor)
        .expect("cursor position");
    assert!(x.is_finite());
    assert!(y.is_finite());
}

#[test]
fn editing_operations_and_selection() {
    let mut edit = TextEditState::new("hello");
    edit.backspace();
    assert_eq!(edit.text, "hell");
    edit.insert_text("o!");
    assert_eq!(edit.text, "hello!");

    edit.home(false);
    edit.move_right(true);
    edit.move_right(true);
    assert_eq!(edit.selected_text(), "he");
    edit.insert_text("HE");
    assert_eq!(edit.text, "HEllo!");

    edit.select_all();
    assert_eq!(edit.selected_text(), "HEllo!");
    edit.delete_selection();
    assert!(edit.text.is_empty());
}

#[test]
fn clipboard_copy_cut_paste() {
    struct MockClipboard {
        text: Option<String>,
    }
    impl UiClipboard for MockClipboard {
        fn get_text(&mut self) -> Option<String> {
            self.text.clone()
        }

        fn set_text(&mut self, text: String) {
            self.text = Some(text);
        }
    }

    let mut clipboard = MockClipboard { text: None };
    let mut edit = TextEditState::new("copy me");
    edit.select_all();
    edit.copy(&mut clipboard);
    assert_eq!(clipboard.text.as_deref(), Some("copy me"));

    edit.set_text("cut me");
    edit.select_all();
    edit.cut(&mut clipboard);
    assert_eq!(clipboard.text.as_deref(), Some("cut me"));
    assert!(edit.text.is_empty());

    edit.paste(&mut clipboard);
    assert_eq!(edit.text, "cut me");
}

#[test]
fn enter_submits_and_typing_inserts() {
    let mut edit = TextEditState::new("");
    let mut modifiers = UiModifiers::default();
    assert!(!edit.handle_key(UiKey::Character('a'), modifiers));
    assert_eq!(edit.text, "a");
    modifiers.shift = true;
    assert!(!edit.handle_key(UiKey::Character('B'), modifiers));
    assert_eq!(edit.text, "aB");
    modifiers.shift = false;
    assert!(edit.handle_key(UiKey::Enter, modifiers));
    assert!(!edit.handle_key(UiKey::Escape, modifiers));
}

#[test]
fn steady_state_cache_hit_rate_exceeds_ninety_five_percent() {
    let mut engine = TextEngine::new();
    let text = "Steady state UI label";
    let sized = style(16.0);
    for _ in 0..100 {
        let _ = engine.shape(text, &sized, 200.0);
    }
    let stats = engine.stats();
    let total = stats.cache_hits + stats.cache_misses;
    assert!(total > 0);
    assert!(
        stats.cache_hits as f64 / total as f64 > 0.95,
        "cache hit rate was {:.3}",
        stats.cache_hits as f64 / total as f64
    );
}

#[test]
fn measurement_is_finite_at_one_and_a_half_and_two_x() {
    let mut engine = TextEngine::new();
    for scale in [1.0, 1.5, 2.0] {
        engine.set_scale(scale);
        let size = engine.measure("Scaled", &style(16.0), Some(200.0));
        assert!(size.width.is_finite() && size.width > 0.0);
        assert!(size.height.is_finite() && size.height > 0.0);
    }
}

#[test]
fn integrates_with_core_layout_measurement() {
    use ignit_ecs::World;
    use ignit_ui::{UiBuilder, UiDocumentNode, UiLayout, UiView};

    let mut world = World::new();
    ignit_ui::init(&mut world);
    let mut root = UiDocumentNode::new("root")
        .layout(UiLayout::column().size(300.0, 200.0).padding(10.0))
        .child(UiDocumentNode::new("label").text("measured by cosmic-text"));
    root.view = Some(UiView::new(300.0, 200.0));
    UiBuilder::new().root(root).build(&mut world).unwrap();
    let mut engine = TextEngine::new();
    let stats = ignit_ui::update(&mut world, &mut engine);
    assert_eq!(stats.roots, 1);
    assert!(stats.nodes >= 2);
    let label = world
        .get_resource::<ignit_ui::UiForest>()
        .unwrap()
        .find(&"label".into())
        .unwrap();
    let computed = world
        .get_component_store::<ignit_ui::UiComputed>()
        .unwrap()
        .get_component(label.index)
        .copied()
        .unwrap();
    assert!(computed.rect.height > 0.0);
}

#[test]
fn caret_after_spaces_remains_visible() {
    let mut engine = TextEngine::new();
    let shaped = engine.shape("a ", &style(16.0), 200.0);
    let caret = engine.cursor_rect(&shaped, 2).expect("caret exists");
    assert!(caret[0] > 0.0);
    assert!(caret[3] > 0.0);

    let empty_space = engine.shape(" ", &style(16.0), 200.0);
    let caret = engine
        .cursor_rect(&empty_space, 1)
        .expect("caret after space");
    assert!(caret[0] >= 0.0);
}

#[test]
fn color_change_invalidates_shaped_cache() {
    let mut engine = TextEngine::new();
    let white = UiTextStyle {
        color: ignit_ui::UiColor::rgba([1.0, 1.0, 1.0, 1.0]),
        ..style(16.0)
    };
    let black = UiTextStyle {
        color: ignit_ui::UiColor::rgba([0.0, 0.0, 0.0, 1.0]),
        ..style(16.0)
    };
    let first = engine.shape("Theme text", &white, 200.0);
    let second = engine.shape("Theme text", &black, 200.0);
    assert_eq!(first.glyphs[0].color, [1.0, 1.0, 1.0, 1.0]);
    assert_eq!(second.glyphs[0].color, [0.0, 0.0, 0.0, 1.0]);
}
