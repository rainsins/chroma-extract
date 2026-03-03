//! 中位切分（Median Cut）算法实现。
//!
//! ## 算法流程
//!
//! 1. 将所有像素放入一个**桶**（RGB 空间中的轴对齐包围盒）。
//! 2. 当桶的数量 < `k` 时循环：
//!    a. 选取像素最多的桶。
//!    b. 找出该桶中颜色范围最大的通道（R、G 或 B）。
//!    c. 按该通道对桶内像素排序。
//!    d. 在中位像素处切分，生成两个新桶。
//! 3. 对每个桶，计算其像素均值作为代表色。
//!
//! 时间复杂度：O(n · k · log n)，其中 n = 像素数，k = 请求颜色数。

use crate::color::{mean_color, ColorPalette};
use crate::error::{DominantColorError, Result};
use crate::Config;

/// 使用中位切分算法提取主色调。
pub fn extract(pixels: &[[u8; 3]], config: &Config) -> Result<ColorPalette> {
    let k = config.max_colors.min(pixels.len());
    if pixels.is_empty() {
        return Err(DominantColorError::EmptyImage);
    }

    // 用像素的拥有副本初始化桶列表，以便原地排序
    let mut buckets: Vec<Vec<[u8; 3]>> = vec![pixels.to_vec()];

    // 不断切分直到达到 k 个桶，或无法继续切分
    while buckets.len() < k {
        // 选取像素最多的桶进行切分
        let split_idx = buckets
            .iter()
            .enumerate()
            .max_by_key(|(_, b)| b.len())
            .map(|(i, _)| i)
            .ok_or_else(|| DominantColorError::internal("桶列表意外为空"))?;

        if buckets[split_idx].len() < 2 {
            break; // 单像素桶无法继续切分
        }

        let bucket = buckets.remove(split_idx);
        let (left, right) = split_bucket(bucket);
        buckets.push(left);
        buckets.push(right);
    }

    let total = pixels.len() as f32;
    let palette: ColorPalette = buckets
        .iter()
        .filter(|b| !b.is_empty())
        .filter_map(|b| mean_color(b, b.len() as f32 / total))
        .collect();

    if palette.is_empty() {
        return Err(DominantColorError::internal("中位切分后所有桶均为空"));
    }

    Ok(palette)
}

// ── 内部实现 ──────────────────────────────────────────────────────────────────

/// 通道索引常量。
const R: usize = 0;
const G: usize = 1;
const B: usize = 2;

/// 找出像素集合中颜色范围（max - min）最大的通道索引（[`R`]、[`G`] 或 [`B`]）。
fn longest_axis(pixels: &[[u8; 3]]) -> usize {
    let (mut r_min, mut r_max) = (u8::MAX, u8::MIN);
    let (mut g_min, mut g_max) = (u8::MAX, u8::MIN);
    let (mut b_min, mut b_max) = (u8::MAX, u8::MIN);

    for p in pixels {
        r_min = r_min.min(p[R]);
        r_max = r_max.max(p[R]);
        g_min = g_min.min(p[G]);
        g_max = g_max.max(p[G]);
        b_min = b_min.min(p[B]);
        b_max = b_max.max(p[B]);
    }

    let ranges = [
        r_max.saturating_sub(r_min),
        g_max.saturating_sub(g_min),
        b_max.saturating_sub(b_min),
    ];

    // 返回范围最大的通道（相等时优先 R > G > B）
    ranges
        .iter()
        .enumerate()
        .max_by_key(|(_, &v)| v)
        .map(|(i, _)| i)
        .unwrap_or(R)
}

/// 按最长通道排序后在中位切分桶，返回 `(左半部分, 右半部分)`。
fn split_bucket(mut pixels: Vec<[u8; 3]>) -> (Vec<[u8; 3]>, Vec<[u8; 3]>) {
    let axis = longest_axis(&pixels);
    // 按目标通道升序排序
    pixels.sort_unstable_by_key(|p| p[axis]);
    // 在中点处切分（左半取前一半，右半取后一半）
    let mid = pixels.len() / 2;
    let right = pixels.split_off(mid);
    (pixels, right)
}

// ── 单元测试 ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(k: usize) -> Config {
        Config::default().max_colors(k).sample_size(None)
    }

    #[test]
    fn test_extract_empty() {
        // 空像素集应返回 EmptyImage 错误
        let result = extract(&[], &cfg(4));
        assert_eq!(result, Err(DominantColorError::EmptyImage));
    }

    #[test]
    fn test_single_pixel() {
        // 单像素：返回 1 种颜色，占比为 100%
        let pixels = vec![[128u8, 64, 32]];
        let palette = extract(&pixels, &cfg(3)).unwrap();
        assert_eq!(palette.len(), 1);
        assert!((palette[0].percentage - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_two_distinct_colors() {
        // 蓝色 50 个 + 红色 50 个：各占约 50%
        let mut pixels = vec![[0u8, 0, 255]; 50];
        pixels.extend(vec![[255u8, 0, 0]; 50]);
        let palette = extract(&pixels, &cfg(2)).unwrap();
        assert_eq!(palette.len(), 2);
        for c in &palette {
            assert!((c.percentage - 0.5).abs() < 0.05, "各色应占约 50%");
        }
    }

    #[test]
    fn test_percentages_sum_to_one() {
        // 各颜色占比之和应精确等于 1.0
        let pixels: Vec<[u8; 3]> = (0..200u8).map(|i| [i, i.wrapping_add(10), 100]).collect();
        let palette = extract(&pixels, &cfg(8)).unwrap();
        let total: f32 = palette.iter().map(|c| c.percentage).sum();
        assert!((total - 1.0).abs() < 1e-5, "占比之和 = {total}");
    }

    #[test]
    fn test_k_exceeds_unique_pixels() {
        // 请求颜色数超过唯一像素数时应优雅降级
        let pixels = vec![[255u8, 0, 0]; 3];
        let palette = extract(&pixels, &cfg(10)).unwrap();
        assert!(palette.len() <= 3, "结果颜色数不应超过唯一像素数");
    }

    #[test]
    fn test_longest_axis_red() {
        // 只有 R 通道变化时，最长轴应为 R
        let pixels = vec![[0u8, 5, 5], [255, 5, 5]];
        assert_eq!(longest_axis(&pixels), R);
    }

    #[test]
    fn test_longest_axis_green() {
        // 只有 G 通道变化时，最长轴应为 G
        let pixels = vec![[5u8, 0, 5], [5, 200, 5]];
        assert_eq!(longest_axis(&pixels), G);
    }

    #[test]
    fn test_longest_axis_blue() {
        // 只有 B 通道变化时，最长轴应为 B
        let pixels = vec![[5u8, 5, 10], [5, 5, 250]];
        assert_eq!(longest_axis(&pixels), B);
    }

    #[test]
    fn test_split_bucket_even() {
        // 4 个像素对半切分：各 2 个
        let pixels = vec![[0u8, 0, 0], [100, 0, 0], [200, 0, 0], [255, 0, 0]];
        let (left, right) = split_bucket(pixels);
        assert_eq!(left.len(), 2);
        assert_eq!(right.len(), 2);
    }

    #[test]
    fn test_deterministic() {
        // 算法无随机性，相同输入应产生完全相同的结果
        let pixels: Vec<[u8; 3]> = (0..100u8).map(|i| [i, 255 - i, i / 2]).collect();
        let p1 = extract(&pixels, &cfg(5)).unwrap();
        let p2 = extract(&pixels, &cfg(5)).unwrap();
        assert_eq!(p1.len(), p2.len());
        for (a, b) in p1.iter().zip(p2.iter()) {
            assert_eq!((a.r, a.g, a.b), (b.r, b.g, b.b), "相同输入结果应一致");
        }
    }

    #[test]
    fn test_gradient_image_color_count() {
        // 平滑渐变应被切分为恰好 k 个桶
        let pixels: Vec<[u8; 3]> = (0..=255u8).map(|i| [i, 0, 0]).collect();
        let palette = extract(&pixels, &cfg(8)).unwrap();
        assert_eq!(palette.len(), 8, "渐变图应被切分为 8 个桶");
    }
}
