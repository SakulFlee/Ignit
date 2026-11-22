use cgmath::{InnerSpace, Vector2, Vector3};
use ignit_mesh::MeshDescriptor;
use ignit_procgeo::shapes::*;

#[derive(Debug, Default)]
struct WindingAudit {
    agree: usize,
    disagree: usize,
    degenerate: usize,
}

fn audit(mesh: &MeshDescriptor) -> WindingAudit {
    let mut audit = WindingAudit::default();

    for triangle in mesh.indices.chunks_exact(3) {
        let [i, j, k] = [
            triangle[0] as usize,
            triangle[1] as usize,
            triangle[2] as usize,
        ];
        let (p0, p1, p2) = (
            mesh.vertices[i].position,
            mesh.vertices[j].position,
            mesh.vertices[k].position,
        );
        let vertex_normal =
            (mesh.vertices[i].normal + mesh.vertices[j].normal + mesh.vertices[k].normal) / 3.0;
        let geometric_normal = (p1 - p0).cross(p2 - p0);

        if geometric_normal.magnitude2() < 1e-12 || vertex_normal.magnitude2() < 1e-12 {
            audit.degenerate += 1;
            continue;
        }

        if geometric_normal.normalize().dot(vertex_normal.normalize()) >= 0.0 {
            audit.agree += 1;
        } else {
            audit.disagree += 1;
        }
    }

    audit
}

/// A shape may use either winding convention, but it must not mix them:
/// mixing is what makes one-sided culling keep one part of a shape and drop
/// another.
fn assert_consistent(name: &str, mesh: &MeshDescriptor) {
    let audit = audit(mesh);
    assert!(
        audit.agree == 0 || audit.disagree == 0,
        "{name} mixes triangle winding: {} triangles follow the vertex normals, \
         {} oppose them, {} are degenerate",
        audit.agree,
        audit.disagree,
        audit.degenerate,
    );
}

#[test]
fn procedural_shapes_use_consistent_triangle_winding() {
    assert_consistent("plane", &plane(Vector2::new(2.0, 2.0), 2));
    assert_consistent("grid", &grid(2.0, 2.0, 2, 2));
    assert_consistent("disk", &disk(1.0, 16));
    assert_consistent("box", &box_(Vector3::new(1.0, 1.0, 1.0)));
    assert_consistent("uv_sphere", &uv_sphere(1.0, 16, 8));
    assert_consistent("icosphere", &icosphere(1.0, 2));
    assert_consistent("cylinder", &cylinder(0.5, 1.0, 16));
    assert_consistent("cone", &cone(0.5, 1.0, 16));
    assert_consistent("torus", &torus(0.75, 0.25, 16, 8));
    assert_consistent("capsule", &capsule(0.4, 0.8, 12, 6));
}
