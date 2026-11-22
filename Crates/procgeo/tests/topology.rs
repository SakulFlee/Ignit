use cgmath::{Vector2, Vector3};
use ignit_mesh::{MeshDescriptor, has_boundary_edges};
use ignit_procgeo::shapes::*;

fn is_open(mesh: &MeshDescriptor) -> bool {
    has_boundary_edges(&mesh.vertices, &mesh.indices)
}

#[test]
fn open_and_closed_procedural_shapes() {
    // Single-layer surfaces: the shadow pass must render them two-sided.
    assert!(is_open(&plane(Vector2::new(2.0, 2.0), 2)));
    assert!(is_open(&grid(2.0, 2.0, 2, 2)));
    assert!(is_open(&disk(1.0, 16)));

    // Closed shells: they can use one-sided shadow culling.
    assert!(!is_open(&box_(Vector3::new(1.0, 1.0, 1.0))));
    assert!(!is_open(&uv_sphere(1.0, 16, 8)));
    assert!(!is_open(&icosphere(1.0, 2)));
    assert!(!is_open(&cylinder(0.5, 1.0, 16)));
    assert!(!is_open(&cone(0.5, 1.0, 16)));
    assert!(!is_open(&torus(0.75, 0.25, 16, 8)));
    assert!(!is_open(&capsule(0.4, 0.8, 12, 6)));
}
