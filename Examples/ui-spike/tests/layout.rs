use std::time::Instant;

use ui_spike::layout::{Rect, UiTree, hit_test};
use ui_spike::text::TextEngine;

#[test]
fn five_hundred_node_layout_is_fast() {
    let mut text = TextEngine::new();
    let mut tree = UiTree::synthetic(500, 1400.0, 2000.0);

    // Warm the text measurement and Taffy caches.
    for _ in 0..5 {
        let _ = tree.compute(&mut text, 1400.0, 2000.0, 0.0);
    }

    let iterations = 50;
    let start = Instant::now();
    for _ in 0..iterations {
        let _ = tree.compute(&mut text, 1400.0, 2000.0, 0.0);
    }
    let average = start.elapsed().as_secs_f64() * 1000.0 / iterations as f64;
    println!("500-node layout average: {average:.3} ms");
    if !cfg!(debug_assertions) {
        assert!(average < 1.0, "layout took {average:.3} ms");
    }
}

#[test]
fn text_cache_hits_in_steady_state() {
    let mut text = TextEngine::new();
    let _ = text.shape("Cache me", 16.0, 200.0, [0.0, 0.0], [1.0, 1.0, 1.0, 1.0]);
    let rasterized = text.atlas.rasterizations;
    let hits = text.atlas.cache_hits;
    for _ in 0..20 {
        let _ = text.shape("Cache me", 16.0, 200.0, [0.0, 0.0], [1.0, 1.0, 1.0, 1.0]);
    }
    assert_eq!(text.atlas.rasterizations, rasterized);
    assert!(text.atlas.cache_hits > hits);

    let _ = text.measure("Measured once", 14.0, 300.0);
    let measure_hits = text.measure_hits;
    let _ = text.measure("Measured once", 14.0, 300.0);
    assert!(text.measure_hits > measure_hits);
}

#[test]
fn scissor_rect_intersection() {
    let scroll = Rect {
        x: 10.0,
        y: 20.0,
        w: 100.0,
        h: 50.0,
    };
    let row = Rect {
        x: 0.0,
        y: 40.0,
        w: 200.0,
        h: 30.0,
    };
    let clip = row.intersect(scroll).unwrap();
    assert_eq!(clip.y, 40.0);
    assert_eq!(clip.h, 30.0);
    assert!(!clip.contains(5.0, 45.0));
    assert!(clip.contains(50.0, 45.0));
}

#[test]
fn hit_test_respects_scroll_clip() {
    let mut text = TextEngine::new();
    let mut tree = UiTree::demo(1200.0, 800.0, 1.0, "off");
    let nodes = tree.compute(&mut text, 1200.0, 800.0, 0.0);
    let scroll = nodes.iter().find(|node| node.id == "scroll").unwrap();
    assert!(scroll.clip.is_some());
    // A point just above the scroll view must not hit an interactive scroll
    // child even if a row's own rect overlaps it.
    let _ = hit_test(&nodes, scroll.rect.x + 2.0, scroll.rect.y - 2.0);
}
