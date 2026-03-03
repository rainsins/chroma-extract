//! # dominant-colors
//!
//! 使用三种经典算法从图片中提取主色调：
//!
//! - **K-Means 聚类** — 基于质心的迭代式颜色量化
//! - **中位切分（Median Cut）** — 沿最长轴递归划分颜色空间
//! - **八叉树量化（Octree）** — 对 RGB 立方体进行层次化空间细分
//!
//! ## 快速上手
//!
//! ```rust,no_run
//! use dominant_colors::{DominantColors, Algorithm, Config};
//!
//! // 加载图片，用 K-Means 提取 5 种主色调
//! let img = image::open("photo.jpg").unwrap();
//! let palette = DominantColors::new(img)
//!     .config(Config::default().max_colors(5))
//!     .extract(Algorithm::KMeans)
//!     .unwrap();
//!
//! for color in &palette {
//!     println!("#{:02X}{:02X}{:02X}  ({:.1}%)", color.r, color.g, color.b, color.percentage * 100.0);
//! }
//! ```
//!
//! ## 算法对比
//!
//! | 算法         | 速度 | 质量 | 确定性          |
//! |-------------|------|------|----------------|
//! | K-Means     | 中等 | 高   | 否（可设种子）  |
//! | Median Cut  | 快   | 良   | 是              |
//! | Octree      | 快   | 良   | 是              |

#![warn(missing_docs)]
#![warn(clippy::all)]

pub mod algorithms;
mod color;
mod error;

// WASM 绑定层：仅在编译目标为 wasm32 时启用
pub mod wasm;

pub use color::{Color, ColorPalette};
pub use error::{DominantColorError, Result};

use image::DynamicImage;

/// 主色调提取算法枚举。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Algorithm {
    /// K-Means 聚类：迭代优化颜色质心。
    /// 结果质量高，但因随机初始化而非确定性（可通过固定种子复现）。
    #[default]
    KMeans,
    /// 中位切分：沿颜色范围最宽的通道递归对半分割颜色空间。
    /// 速度快，结果完全确定。
    MedianCut,
    /// 八叉树量化：对 RGB 立方体进行层次化细分。
    /// 速度快、结果确定、内存占用低。
    Octree,
}

/// 所有算法共用的配置项。
#[derive(Debug, Clone)]
pub struct Config {
    /// 提取的最大主色数量（默认：8）。
    pub max_colors: usize,
    /// 处理前将图片缩放到此尺寸（最长边像素数，默认：256）。
    /// 设为 `None` 则处理原始分辨率。
    pub sample_size: Option<u32>,
    /// K-Means 随机种子（其他算法忽略此字段，默认：42）。
    pub kmeans_seed: u64,
    /// K-Means 最大迭代次数（默认：100）。
    pub kmeans_max_iterations: usize,
    /// K-Means 收敛阈值，单位为 RGB 欧氏距离（默认：1.0）。
    /// 所有质心移动量均低于此值时提前终止迭代。
    pub kmeans_convergence_threshold: f64,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            max_colors: 8,
            sample_size: Some(256),
            kmeans_seed: 42,
            kmeans_max_iterations: 100,
            kmeans_convergence_threshold: 1.0,
        }
    }
}

impl Config {
    /// 设置提取的最大颜色数量。
    ///
    /// # Panics
    ///
    /// `n` 为零时 panic。
    #[must_use]
    pub fn max_colors(mut self, n: usize) -> Self {
        assert!(n > 0, "max_colors must be at least 1");
        self.max_colors = n;
        self
    }

    /// 设置缩放尺寸（最长边像素数）。传入 `None` 则使用原始尺寸。
    #[must_use]
    pub fn sample_size(mut self, size: Option<u32>) -> Self {
        self.sample_size = size;
        self
    }

    /// 设置 K-Means 随机种子。
    #[must_use]
    pub fn kmeans_seed(mut self, seed: u64) -> Self {
        self.kmeans_seed = seed;
        self
    }

    /// 设置 K-Means 最大迭代次数。
    #[must_use]
    pub fn kmeans_max_iterations(mut self, iters: usize) -> Self {
        self.kmeans_max_iterations = iters;
        self
    }
}

/// 主色调提取的构建器（Builder）。
///
/// # 示例
///
/// ```rust,no_run
/// use dominant_colors::{DominantColors, Algorithm, Config};
///
/// let img = image::open("photo.jpg").unwrap();
/// let palette = DominantColors::new(img)
///     .config(Config::default().max_colors(6))
///     .extract(Algorithm::Octree)
///     .unwrap();
/// ```
pub struct DominantColors {
    image: DynamicImage,
    config: Config,
}

impl DominantColors {
    /// 从 [`DynamicImage`] 创建提取器。
    pub fn new(image: DynamicImage) -> Self {
        Self {
            image,
            config: Config::default(),
        }
    }

    /// 覆盖默认配置。
    #[must_use]
    pub fn config(mut self, config: Config) -> Self {
        self.config = config;
        self
    }

    /// 运行指定算法，返回按占比降序排列的 [`ColorPalette`]。
    ///
    /// 最主要的颜色排在最前面。
    ///
    /// # 错误
    ///
    /// 图片无像素或违反算法约束时返回 [`DominantColorError`]。
    pub fn extract(self, algorithm: Algorithm) -> Result<ColorPalette> {
        let pixels = self.sample_pixels();

        if pixels.is_empty() {
            return Err(DominantColorError::EmptyImage);
        }

        let mut palette = match algorithm {
            Algorithm::KMeans => algorithms::kmeans::extract(&pixels, &self.config)?,
            Algorithm::MedianCut => algorithms::median_cut::extract(&pixels, &self.config)?,
            Algorithm::Octree => algorithms::octree::extract(&pixels, &self.config)?,
        };

        // 按占比降序排列，最主要的颜色排在最前
        palette.sort_by(|a, b| b.percentage.partial_cmp(&a.percentage).unwrap());
        Ok(palette)
    }

    /// 将图片等比缩放到 `config.sample_size` 后收集所有 RGB 像素。
    fn sample_pixels(&self) -> Vec<[u8; 3]> {
        let img = if let Some(size) = self.config.sample_size {
            let (w, h) = (self.image.width(), self.image.height());
            if w > size || h > size {
                // 等比缩放，最长边不超过 size
                self.image.thumbnail(size, size).into_rgb8()
            } else {
                self.image.to_rgb8()
            }
        } else {
            self.image.to_rgb8()
        };

        img.pixels().map(|p| [p.0[0], p.0[1], p.0[2]]).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    /// 从 RGB 三元组列表构造合成测试图片（宽度 = 像素数，高度 = 1）。
    fn make_image(pixels: &[[u8; 3]]) -> DynamicImage {
        let width = pixels.len() as u32;
        let buf: ImageBuffer<Rgb<u8>, Vec<u8>> = ImageBuffer::from_fn(width, 1, |x, _| {
            let p = pixels[x as usize];
            Rgb([p[0], p[1], p[2]])
        });
        DynamicImage::ImageRgb8(buf)
    }

    #[test]
    fn test_empty_image_error() {
        // 空图片（0×0）应对所有算法均返回 EmptyImage 错误
        let img = DynamicImage::ImageRgb8(ImageBuffer::new(0, 0));
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let result = DominantColors::new(img.clone())
                .config(Config::default().sample_size(None))
                .extract(alg);
            assert!(
                matches!(result, Err(DominantColorError::EmptyImage)),
                "{alg:?} 应返回 EmptyImage"
            );
        }
    }

    #[test]
    fn test_single_color_image() {
        // 纯红色图片：调色板不为空，且各颜色占比之和 ≈ 1.0
        let pixels = vec![[255u8, 0, 0]; 100];
        let img = make_image(&pixels);
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let palette = DominantColors::new(img.clone())
                .config(Config::default().max_colors(3).sample_size(None))
                .extract(alg)
                .expect("纯色图片应成功");
            assert!(!palette.is_empty(), "{alg:?} 调色板不应为空");
            assert!(
                palette.iter().map(|c| c.percentage).sum::<f32>() > 0.99,
                "{alg:?} 占比之和应约等于 1.0"
            );
        }
    }

    #[test]
    fn test_two_color_image_separation() {
        // 50 个红色像素 + 50 个蓝色像素，两种颜色各占约 50%
        let mut pixels = vec![[255u8, 0, 0]; 50];
        pixels.extend(vec![[0u8, 0, 255]; 50]);
        let img = make_image(&pixels);

        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let palette = DominantColors::new(img.clone())
                .config(Config::default().max_colors(2).sample_size(None))
                .extract(alg)
                .expect("双色图片应成功");

            assert_eq!(palette.len(), 2, "{alg:?} 应识别出 2 种颜色");
            for color in &palette {
                assert!(
                    (color.percentage - 0.5).abs() < 0.1,
                    "{alg:?}: 期望约 50%，实际 {:.1}%",
                    color.percentage * 100.0
                );
            }
        }
    }

    #[test]
    fn test_palette_sorted_descending() {
        // 红 60% / 绿 30% / 蓝 10%，验证调色板按占比降序排列
        let mut pixels = vec![[255u8, 0, 0]; 60];
        pixels.extend(vec![[0u8, 255, 0]; 30]);
        pixels.extend(vec![[0u8, 0, 255]; 10]);
        let img = make_image(&pixels);

        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let palette = DominantColors::new(img.clone())
                .config(Config::default().max_colors(3).sample_size(None))
                .extract(alg)
                .expect("三色图片应成功");

            let percentages: Vec<f32> = palette.iter().map(|c| c.percentage).collect();
            let mut sorted = percentages.clone();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
            assert_eq!(percentages, sorted, "{alg:?} 调色板应按占比降序排列");
        }
    }

    #[test]
    fn test_config_builder() {
        // 验证 Builder 链式调用可正确设置各字段
        let cfg = Config::default()
            .max_colors(10)
            .sample_size(Some(128))
            .kmeans_seed(99)
            .kmeans_max_iterations(50);
        assert_eq!(cfg.max_colors, 10);
        assert_eq!(cfg.sample_size, Some(128));
        assert_eq!(cfg.kmeans_seed, 99);
        assert_eq!(cfg.kmeans_max_iterations, 50);
    }

    #[test]
    #[should_panic(expected = "max_colors must be at least 1")]
    fn test_config_zero_colors_panics() {
        // max_colors 设为 0 时应 panic
        let _ = Config::default().max_colors(0);
    }
}