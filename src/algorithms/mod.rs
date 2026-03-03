//! 主色调提取算法集合。
//!
//! 本模块作为三种算法的统一门面，对外暴露 [`extract`] 函数，
//! 调用方只需传入 [`crate::Algorithm`] 枚举值即可，无需直接依赖具体算法模块。
//!
//! ## 各算法适用场景
//!
//! | 算法                     | 首选场景                                   |
//! |-------------------------|--------------------------------------------|
//! | [`Algorithm::KMeans`]   | 对颜色质量要求高、可接受稍慢速度的场景     |
//! | [`Algorithm::MedianCut`]| 追求速度且结果需确定性的自然照片场景       |
//! | [`Algorithm::Octree`]   | 内存敏感或处理大量图片的批量任务场景       |

pub mod kmeans;
pub mod median_cut;
pub mod octree;

use crate::{Algorithm, Color, Config, DominantColorError, Result};

/// 算法能力描述，用于在运行时查询算法特性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlgorithmInfo {
    /// 算法名称。
    pub name: &'static str,
    /// 简短描述。
    pub description: &'static str,
    /// 相同输入是否总产生相同输出（K-Means 因随机初始化而非确定性）。
    pub is_deterministic: bool,
    /// 相对速度评级（1 = 最快，数字越大越慢）。
    pub speed_rank: u8,
}

impl AlgorithmInfo {
    /// 返回指定算法的能力描述。
    ///
    /// # 示例
    ///
    /// ```rust
    /// use dominant_colors::{Algorithm, algorithms::AlgorithmInfo};
    ///
    /// let info = AlgorithmInfo::of(Algorithm::KMeans);
    /// assert!(!info.is_deterministic);
    /// println!("{}: {}", info.name, info.description);
    /// ```
    pub fn of(algorithm: Algorithm) -> Self {
        match algorithm {
            Algorithm::KMeans => Self {
                name: "K-Means 聚类",
                description: "基于质心的迭代聚类，使用 K-Means++ 初始化，颜色质量最高",
                is_deterministic: false,
                speed_rank: 3,
            },
            Algorithm::MedianCut => Self {
                name: "中位切分",
                description: "沿颜色范围最宽的通道递归对半分割，速度快且结果确定",
                is_deterministic: true,
                speed_rank: 1,
            },
            Algorithm::Octree => Self {
                name: "八叉树量化",
                description: "对 RGB 立方体进行层次化细分，速度快、内存占用低且结果确定",
                is_deterministic: true,
                speed_rank: 2,
            },
        }
    }
}

/// 使用指定算法从像素列表中提取主色调的统一入口。
///
/// 这是对三个算法子模块的薄封装，额外提供：
/// - 统一的前置校验（像素为空、`k` 为零）
/// - 结果后处理（按占比降序排列、过滤占比为零的颜色）
///
/// 一般情况下建议通过 [`crate::DominantColors`] builder 调用，
/// 而非直接使用本函数。
///
/// # 参数
///
/// - `pixels`：RGB 像素列表，每个元素为 `[r, g, b]`
/// - `algorithm`：要使用的算法
/// - `config`：算法配置（`max_colors`、K-Means 参数等）
///
/// # 错误
///
/// - [`DominantColorError::EmptyImage`]：`pixels` 为空
/// - [`DominantColorError::InternalError`]：算法内部不变式违反（正常使用下不会出现）
///
/// # 示例
///
/// ```rust
/// use dominant_colors::{Algorithm, Config, algorithms};
///
/// let pixels: Vec<[u8; 3]> = vec![[255, 0, 0]; 50]
///     .into_iter()
///     .chain(vec![[0, 0, 255]; 50])
///     .collect();
///
/// let palette = algorithms::extract(&pixels, Algorithm::MedianCut, &Config::default()).unwrap();
/// assert_eq!(palette.len(), 2);
/// ```
pub fn extract(
    pixels: &[[u8; 3]],
    algorithm: Algorithm,
    config: &Config,
) -> Result<Vec<Color>> {
    if pixels.is_empty() {
        return Err(DominantColorError::EmptyImage);
    }

    // 派发到对应算法实现
    let mut palette = match algorithm {
        Algorithm::KMeans => kmeans::extract(pixels, config)?,
        Algorithm::MedianCut => median_cut::extract(pixels, config)?,
        Algorithm::Octree => octree::extract(pixels, config)?,
    };

    // 过滤掉占比为零的颜色（极端情况下可能出现）
    palette.retain(|c| c.percentage > 0.0);

    // 按占比降序排列，最主要的颜色排在最前
    palette.sort_by(|a, b| {
        b.percentage
            .partial_cmp(&a.percentage)
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    Ok(palette)
}

/// 对同一组像素同时运行所有三种算法，便于并排比较结果。
///
/// 返回值为 `[(Algorithm, ColorPalette)]` 有序列表，顺序为
/// `[KMeans, MedianCut, Octree]`。
///
/// 任一算法失败时立即返回其错误，不继续执行后续算法。
///
/// # 示例
///
/// ```rust
/// use dominant_colors::{Config, algorithms};
///
/// let pixels: Vec<[u8; 3]> = (0..100u8).map(|i| [i, i, i]).collect();
/// let results = algorithms::extract_all(&pixels, &Config::default()).unwrap();
///
/// for (alg, palette) in &results {
///     println!("{:?}: {} 种颜色", alg, palette.len());
/// }
/// ```
pub fn extract_all(
    pixels: &[[u8; 3]],
    config: &Config,
) -> Result<Vec<(Algorithm, Vec<Color>)>> {
    [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree]
        .iter()
        .map(|&alg| extract(pixels, alg, config).map(|palette| (alg, palette)))
        .collect()
}

// ── 单元测试 ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn two_color_pixels() -> Vec<[u8; 3]> {
        let mut v = vec![[255u8, 0, 0]; 50];
        v.extend(vec![[0u8, 0, 255]; 50]);
        v
    }

    fn cfg(k: usize) -> Config {
        Config::default().max_colors(k).sample_size(None)
    }

    // ── AlgorithmInfo ────────────────────────────────────────────────────────

    #[test]
    fn test_info_kmeans_not_deterministic() {
        let info = AlgorithmInfo::of(Algorithm::KMeans);
        assert!(!info.is_deterministic, "K-Means 因随机初始化而非确定性");
    }

    #[test]
    fn test_info_median_cut_deterministic() {
        let info = AlgorithmInfo::of(Algorithm::MedianCut);
        assert!(info.is_deterministic);
    }

    #[test]
    fn test_info_octree_deterministic() {
        let info = AlgorithmInfo::of(Algorithm::Octree);
        assert!(info.is_deterministic);
    }

    #[test]
    fn test_info_speed_ranks_distinct() {
        // 三种算法的速度评级应各不相同
        let ranks: Vec<u8> = [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree]
            .iter()
            .map(|&a| AlgorithmInfo::of(a).speed_rank)
            .collect();
        let mut sorted = ranks.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 3, "三种算法的速度评级应各不相同：{ranks:?}");
    }

    #[test]
    fn test_info_name_nonempty() {
        // 每种算法都应有非空名称和描述
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let info = AlgorithmInfo::of(alg);
            assert!(!info.name.is_empty(), "{alg:?} 名称不应为空");
            assert!(!info.description.is_empty(), "{alg:?} 描述不应为空");
        }
    }

    // ── extract（统一入口）────────────────────────────────────────────────────

    #[test]
    fn test_extract_empty_pixels_error() {
        // 空像素集应对所有算法均返回 EmptyImage 错误
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let result = extract(&[], alg, &cfg(4));
            assert_eq!(
                result,
                Err(DominantColorError::EmptyImage),
                "{alg:?} 应返回 EmptyImage"
            );
        }
    }

    #[test]
    fn test_extract_sorted_descending() {
        // 统一入口应保证结果按占比降序排列
        let pixels = two_color_pixels();
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let palette = extract(&pixels, alg, &cfg(2)).unwrap();
            let pcts: Vec<f32> = palette.iter().map(|c| c.percentage).collect();
            let mut sorted = pcts.clone();
            sorted.sort_by(|a, b| b.partial_cmp(a).unwrap());
            assert_eq!(pcts, sorted, "{alg:?} 结果应按占比降序排列");
        }
    }

    #[test]
    fn test_extract_no_zero_percentage() {
        // 统一入口应过滤掉占比为零的颜色
        let pixels: Vec<[u8; 3]> = (0..50u8).map(|i| [i * 5, i, 100]).collect();
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let palette = extract(&pixels, alg, &cfg(6)).unwrap();
            for color in &palette {
                assert!(
                    color.percentage > 0.0,
                    "{alg:?} 不应包含占比为零的颜色"
                );
            }
        }
    }

    #[test]
    fn test_extract_percentages_sum_to_one() {
        // 各颜色占比之和应约等于 1.0
        let pixels = two_color_pixels();
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            let palette = extract(&pixels, alg, &cfg(4)).unwrap();
            let total: f32 = palette.iter().map(|c| c.percentage).sum();
            assert!(
                (total - 1.0).abs() < 1e-4,
                "{alg:?} 占比之和应为 1.0，实际 {total}"
            );
        }
    }

    // ── extract_all ───────────────────────────────────────────────────────────

    #[test]
    fn test_extract_all_returns_three_results() {
        // extract_all 应返回三种算法各自的结果
        let pixels = two_color_pixels();
        let results = extract_all(&pixels, &cfg(2)).unwrap();
        assert_eq!(results.len(), 3, "应返回三种算法的结果");
    }

    #[test]
    fn test_extract_all_algorithm_order() {
        // 结果顺序应为 KMeans → MedianCut → Octree
        let pixels = two_color_pixels();
        let results = extract_all(&pixels, &cfg(2)).unwrap();
        assert_eq!(results[0].0, Algorithm::KMeans);
        assert_eq!(results[1].0, Algorithm::MedianCut);
        assert_eq!(results[2].0, Algorithm::Octree);
    }

    #[test]
    fn test_extract_all_each_palette_nonempty() {
        // 每种算法的调色板都不应为空
        let pixels: Vec<[u8; 3]> = (0..100u8).map(|i| [i, i, i]).collect();
        let results = extract_all(&pixels, &cfg(4)).unwrap();
        for (alg, palette) in &results {
            assert!(!palette.is_empty(), "{alg:?} 调色板不应为空");
        }
    }

    #[test]
    fn test_extract_all_empty_pixels_error() {
        // 空像素集应立即返回错误，不继续执行后续算法
        let result = extract_all(&[], &cfg(4));
        assert_eq!(result, Err(DominantColorError::EmptyImage));
    }

    #[test]
    fn test_extract_all_consistent_with_individual() {
        // extract_all 的每个结果应与单独调用 extract 一致
        let pixels = two_color_pixels();
        let config = cfg(2);
        let all_results = extract_all(&pixels, &config).unwrap();

        for (alg, palette_from_all) in &all_results {
            let palette_single = extract(&pixels, *alg, &config).unwrap();
            assert_eq!(
                palette_from_all.len(),
                palette_single.len(),
                "{alg:?} extract_all 与单独调用结果数量不一致"
            );
            for (a, b) in palette_from_all.iter().zip(palette_single.iter()) {
                assert_eq!(
                    (a.r, a.g, a.b),
                    (b.r, b.g, b.b),
                    "{alg:?} extract_all 与单独调用颜色值不一致"
                );
            }
        }
    }
}