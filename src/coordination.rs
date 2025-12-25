use std::sync::atomic::AtomicBool;

use cgmath::num_traits::Num;
use cgmath::{Quaternion, Vector3, Vector4, Zero};
pub struct Transform {
    position: Vector3<f32>,
    rotation: Quaternion<f32>,
    scale: Vector3<f32>,
    pub(crate) updated: AtomicBool,
}

impl Default for Transform {
    fn default() -> Self {
        Self::new(
            Vector3::new(0.0, 0.0, 0.0), 
            Quaternion::from_sv(0.0, Vector3::zero()), 
            Vector3::new(1.0, 1.0, 1.0),
        )
    }
}

impl Transform {
    pub fn new(pos: Vector3<f32>, rot: Quaternion<f32>, size: Vector3<f32>) -> Self {
        Self {
            position: pos,
            rotation: rot,
            scale: size,
            updated: AtomicBool::new(false),
        }
    }

    pub fn position(&self) -> Vector3<f32> {
        self.position.clone()
    }

    pub fn rotation(&self) -> Quaternion<f32> {
        self.rotation.clone()
    }

    pub fn scale(&self) -> Vector3<f32> {
        self.scale.clone()
    }

    pub fn set_position(&mut self, pos: Vector3<f32>) {
        self.position = pos;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_rotation(&mut self, rot: Quaternion<f32>) {
        self.rotation = rot;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn set_scale(&mut self, size: Vector3<f32>) {
        self.scale = size;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the X component of the position vector.
    pub fn set_position_x(&mut self, x: f32) {
        self.position.x = x;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the Y component of the position vector.
    pub fn set_position_y(&mut self, y: f32) {
        self.position.y = y;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the Z component of the position vector.
    pub fn set_position_z(&mut self, z: f32) {
        self.position.z = z;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the X component of the scale vector.
    pub fn set_scale_x(&mut self, x: f32) {
        self.scale.x = x;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the Y component of the scale vector.
    pub fn set_scale_y(&mut self, y: f32) {
        self.scale.y = y;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the Z component of the scale vector.
    pub fn set_scale_z(&mut self, z: f32) {
        self.scale.z = z;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the X component of the rotation quaternion.
    pub fn set_rotation_x(&mut self, x: f32) {
        self.rotation.v.x = x;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the Y component of the rotation quaternion.
    pub fn set_rotation_y(&mut self, y: f32) {
        self.rotation.v.y = y;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the Z component of the rotation quaternion.
    pub fn set_rotation_z(&mut self, z: f32) {
        self.rotation.v.z = z;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Set the scalar (w) component of the rotation quaternion.
    pub fn set_rotation_w(&mut self, w: f32) {
        self.rotation.s = w;
        self.updated.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct Rectangle<T: Num + Clone + Default> {
    min: Point2D<T>,
    max: Point2D<T>,
}

impl<T: Num + Clone + Copy + Default> Rectangle<T> {
    pub fn new(min: Point2D<T>, max: Point2D<T>) -> Rectangle<T> {
        Rectangle { min, max }
    }

    pub fn min(&self) -> Point2D<T> {
        self.min
    }

    pub fn max(&self) -> Point2D<T> {
        self.max
    }

    pub fn width(&self) -> T {
        self.max.x - self.min.x
    }
    pub fn height(&self) -> T {
        self.max.y - self.min.y
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct Point2D<T: Num + Clone + Default> {
    pub x: T,
    pub y: T,
}

impl<T: Num + Clone + Default> Point2D<T> {
    pub fn new(x: T, y: T) -> Point2D<T> {
        Point2D { x, y }
    }
}


#[derive(Debug, Copy, Clone, PartialEq, Default)]
pub struct TextureRemapping {
    pub binding: u32,
    pub new_binding: u32,
    pub coords: Rectangle<f32>,
}

impl TextureRemapping {
    pub fn new(binding: u32, new_binding: u32, coords: Rectangle<f32>) -> Self {
        TextureRemapping {
            binding,
            new_binding,
            coords,
        }
    }

    /// Create a texture remapping from a sprite.
    ///
    /// # Arguments
    /// * `binding` - The original texture binding in the shader (e.g., 44 for @binding(44))
    /// * `new_binding` - The actual binding slot to use (e.g., 0 for the first texture)
    /// * `sprite` - The sprite containing UV coordinate information
    ///
    /// # Example
    /// ```ignore
    /// // Map shader binding 44 to actual binding 0 with sprite UV coords
    /// let remapping = TextureRemapping::from_sprite(44, 0, &my_sprite);
    /// ```
    pub fn from_sprite(binding: u32, new_binding: u32, sprite: &crate::texture::Sprite) -> Self {
        TextureRemapping {
            binding,
            new_binding,
            coords: sprite.uv_coords(),
        }
    }
}

