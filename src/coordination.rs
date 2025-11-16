use cgmath::num_traits::Num;

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

