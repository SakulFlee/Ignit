use wgpu::Buffer;

use ignit_texture::Texture;

#[derive(Debug, PartialEq, Eq, Hash)]
pub enum Variable {
    Buffer(Buffer),
    Texture(Texture),
}
