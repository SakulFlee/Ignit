//! CPU draw list with a scissor clip per batch.

use bytemuck::{Pod, Zeroable};

use crate::layout::Rect;

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct DrawVertex {
    pub position: [f32; 2],
    pub color: [f32; 4],
    pub uv: [f32; 2],
    pub params: [f32; 4],
}

#[derive(Clone, Copy, Debug)]
pub struct Batch {
    pub start: u32,
    pub count: u32,
    pub clip: Rect,
    pub mode: u32,
}

#[derive(Default)]
pub struct DrawList {
    pub vertices: Vec<DrawVertex>,
    pub batches: Vec<Batch>,
}

impl DrawList {
    pub fn clear(&mut self) {
        self.vertices.clear();
        self.batches.clear();
    }

    fn begin_batch(&mut self, clip: Rect, mode: u32) {
        let start = self.vertices.len() as u32;
        self.batches.push(Batch {
            start,
            count: 0,
            clip,
            mode,
        });
    }

    fn maybe_batch(&mut self, clip: Rect, mode: u32) {
        let same = self.batches.last().is_some_and(|batch| {
            batch.mode == mode
                && (batch.clip.x - clip.x).abs() < 0.01
                && (batch.clip.y - clip.y).abs() < 0.01
                && (batch.clip.w - clip.w).abs() < 0.01
                && (batch.clip.h - clip.h).abs() < 0.01
        });
        if !same {
            self.begin_batch(clip, mode);
        }
    }

    fn push_quad(
        &mut self,
        rect: Rect,
        color: [f32; 4],
        uv: [f32; 4],
        params: [f32; 4],
        clip: Rect,
        mode: u32,
    ) {
        self.maybe_batch(clip, mode);
        let x = rect.x;
        let y = rect.y;
        let w = rect.w.max(0.0);
        let h = rect.h.max(0.0);
        let [u0, v0, u1, v1] = uv;
        self.vertices.extend_from_slice(&[
            DrawVertex {
                position: [x, y],
                color,
                uv: [u0, v0],
                params,
            },
            DrawVertex {
                position: [x + w, y],
                color,
                uv: [u1, v0],
                params,
            },
            DrawVertex {
                position: [x + w, y + h],
                color,
                uv: [u1, v1],
                params,
            },
            DrawVertex {
                position: [x, y],
                color,
                uv: [u0, v0],
                params,
            },
            DrawVertex {
                position: [x + w, y + h],
                color,
                uv: [u1, v1],
                params,
            },
            DrawVertex {
                position: [x, y + h],
                color,
                uv: [u0, v1],
                params,
            },
        ]);
        self.batches.last_mut().unwrap().count += 6;
    }

    pub fn rect(&mut self, rect: Rect, color: [f32; 4], radius: f32, clip: Rect) {
        self.push_quad(
            rect,
            color,
            [0.0, 0.0, 1.0, 1.0],
            [radius, 0.0, rect.w, rect.h],
            clip,
            0,
        );
    }

    pub fn shadow(&mut self, rect: Rect, color: [f32; 4], radius: f32, clip: Rect) {
        self.rect(rect, color, radius, clip);
    }

    pub fn glyph(&mut self, rect: Rect, uv: [f32; 4], color: [f32; 4], clip: Rect) {
        self.push_quad(rect, color, uv, [0.0, 1.0, rect.w, rect.h], clip, 1);
    }

    pub fn image(&mut self, rect: Rect, color: [f32; 4], uv: [f32; 4], clip: Rect) {
        self.push_quad(rect, color, uv, [0.0, 2.0, rect.w, rect.h], clip, 2);
    }

    pub fn line(&mut self, rect: Rect, color: [f32; 4], clip: Rect) {
        self.push_quad(
            rect,
            color,
            [0.0, 0.0, 1.0, 1.0],
            [0.0, 0.0, rect.w, rect.h],
            clip,
            0,
        );
    }
}
