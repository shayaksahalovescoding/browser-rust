#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    pub const fn to_bgra(self) -> [u8; 4] {
        [self.b, self.g, self.r, self.a]
    }

    /// Alpha-composites `self` over `dst` (`[B, G, R, A]`).
    #[inline]
    pub fn blend_over(self, dst: [u8; 4]) -> [u8; 4] {
        if self.a == 255 {
            return self.to_bgra();
        }
        if self.a == 0 {
            return dst;
        }
        let sa = self.a as u32;
        let inv = 255 - sa;
        let b = (self.b as u32 * sa + dst[0] as u32 * inv + 127) / 255;
        let g = (self.g as u32 * sa + dst[1] as u32 * inv + 127) / 255;
        let r = (self.r as u32 * sa + dst[2] as u32 * inv + 127) / 255;
        [b as u8, g as u8, r as u8, 255]
    }
}
