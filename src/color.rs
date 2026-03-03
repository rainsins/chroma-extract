//! 所有算法共用的核心颜色类型。

use std::fmt;

/// 带有占比信息的 sRGB 颜色。
#[derive(Debug, Clone, PartialEq)]
pub struct Color {
    /// 红色通道（0–255）。
    pub r: u8,
    /// 绿色通道（0–255）。
    pub g: u8,
    /// 蓝色通道（0–255）。
    pub b: u8,
    /// 该颜色在图片中所占的像素比例（0.0–1.0）。
    pub percentage: f32,
}

impl Color {
    /// 构造一个新颜色。
    #[inline]
    pub fn new(r: u8, g: u8, b: u8, percentage: f32) -> Self {
        Self { r, g, b, percentage }
    }

    /// 打包为 24 位十六进制整数（`0xRRGGBB`）。
    #[inline]
    pub fn to_hex(&self) -> u32 {
        ((self.r as u32) << 16) | ((self.g as u32) << 8) | (self.b as u32)
    }

    /// 以 `[r, g, b]` 形式返回 `f64` 值，便于数值计算。
    #[inline]
    pub(crate) fn to_f64(&self) -> [f64; 3] {
        [self.r as f64, self.g as f64, self.b as f64]
    }

    /// 计算两个像素在 RGB 空间中的欧氏距离平方。
    #[inline]
    pub(crate) fn sq_distance_rgb(a: &[u8; 3], b: &[u8; 3]) -> f64 {
        let dr = a[0] as f64 - b[0] as f64;
        let dg = a[1] as f64 - b[1] as f64;
        let db = a[2] as f64 - b[2] as f64;
        dr * dr + dg * dg + db * db
    }
}

impl fmt::Display for Color {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "#{:02X}{:02X}{:02X} ({:.1}%)",
            self.r,
            self.g,
            self.b,
            self.percentage * 100.0
        )
    }
}

/// 主色调列表（有序）。
pub type ColorPalette = Vec<Color>;

/// 计算一组像素的均值颜色。
///
/// `pixels` 为空时返回 `None`。
pub(crate) fn mean_color(pixels: &[[u8; 3]], percentage: f32) -> Option<Color> {
    if pixels.is_empty() {
        return None;
    }
    let n = pixels.len() as f64;
    let r = pixels.iter().map(|p| p[0] as f64).sum::<f64>() / n;
    let g = pixels.iter().map(|p| p[1] as f64).sum::<f64>() / n;
    let b = pixels.iter().map(|p| p[2] as f64).sum::<f64>() / n;
    Some(Color::new(r.round() as u8, g.round() as u8, b.round() as u8, percentage))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_to_hex() {
        let c = Color::new(0xFF, 0xAB, 0x00, 1.0);
        assert_eq!(c.to_hex(), 0xFF_AB_00);
    }

    #[test]
    fn test_display() {
        let c = Color::new(255, 0, 128, 0.5);
        assert_eq!(format!("{c}"), "#FF0080 (50.0%)");
    }

    #[test]
    fn test_sq_distance() {
        // (3,4,0) 到原点的距离平方 = 9+16 = 25
        let a = [0u8, 0, 0];
        let b = [3u8, 4u8, 0];
        assert!((Color::sq_distance_rgb(&a, &b) - 25.0).abs() < 1e-10);
    }

    #[test]
    fn test_mean_color_empty() {
        // 空像素集应返回 None
        assert!(mean_color(&[], 1.0).is_none());
    }

    #[test]
    fn test_mean_color_single() {
        // 单像素的均值就是其本身
        let c = mean_color(&[[10, 20, 30]], 1.0).unwrap();
        assert_eq!((c.r, c.g, c.b), (10, 20, 30));
    }

    #[test]
    fn test_mean_color_average() {
        // (0,0,0) 与 (100,100,100) 的均值应为 (50,50,50)
        let pixels = vec![[0u8, 0, 0], [100, 100, 100]];
        let c = mean_color(&pixels, 1.0).unwrap();
        assert_eq!((c.r, c.g, c.b), (50, 50, 50));
    }
}
