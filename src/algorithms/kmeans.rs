//! K-Means 聚类算法实现。
//!
//! ## 算法流程
//!
//! 1. **初始化**：使用 K-Means++ 策略选取 `k` 个初始质心（加权随机，使质心尽量分散）。
//! 2. **分配**：将每个像素分配给欧氏距离最近的质心（平方距离比较，避免开方运算）。
//! 3. **更新**：将每个质心重新计算为其所有分配像素的均值。
//! 4. **重复**：循环步骤 2–3，直到达到最大迭代次数，或所有质心的移动量均低于收敛阈值。
//! 5. **空簇恢复**：若某个簇变为空簇（dead centroid），从像素最多的簇中随机抽取一个像素作为新质心。

use rand::rngs::SmallRng;
use rand::{Rng, SeedableRng};

use crate::color::{mean_color, Color, ColorPalette};
use crate::error::{DominantColorError, Result};
use crate::Config;

/// 使用 K-Means++ 聚类提取主色调。
pub fn extract(pixels: &[[u8; 3]], config: &Config) -> Result<ColorPalette> {
    // k 不能超过像素总数（否则会出现永久空簇）
    let k = config.max_colors.min(pixels.len());
    if k == 0 {
        return Err(DominantColorError::EmptyImage);
    }

    let mut rng = SmallRng::seed_from_u64(config.kmeans_seed);
    let mut centroids = kmeans_plus_plus_init(pixels, k, &mut rng);

    for _ in 0..config.kmeans_max_iterations {
        let assignments = assign_pixels(pixels, &centroids);
        let new_centroids = update_centroids(pixels, &assignments, k, &mut rng);

        // 检查最大质心移动量，决定是否提前收敛
        let max_shift = centroids
            .iter()
            .zip(new_centroids.iter())
            .map(|(old, new)| Color::sq_distance_rgb(old, new).sqrt())
            .fold(0.0_f64, f64::max);

        centroids = new_centroids;

        if max_shift < config.kmeans_convergence_threshold {
            break; // 所有质心移动量均低于阈值，提前结束
        }
    }

    // 最终分配一次，统计每个簇的像素数以计算占比
    let assignments = assign_pixels(pixels, &centroids);
    let total = pixels.len() as f32;
    let mut cluster_counts = vec![0usize; k];
    for &idx in &assignments {
        cluster_counts[idx] += 1;
    }

    // 对每个非空簇计算均值颜色
    let palette: ColorPalette = (0..k)
        .filter(|&i| cluster_counts[i] > 0)
        .filter_map(|i| {
            let cluster_pixels: Vec<[u8; 3]> = pixels
                .iter()
                .zip(assignments.iter())
                .filter(|(_, &a)| a == i)
                .map(|(&p, _)| p)
                .collect();
            mean_color(&cluster_pixels, cluster_counts[i] as f32 / total)
        })
        .collect();

    if palette.is_empty() {
        return Err(DominantColorError::internal("收敛后所有簇均为空"));
    }

    Ok(palette)
}

// ── 内部实现 ──────────────────────────────────────────────────────────────────

/// K-Means++ 初始化：通过加权随机策略使初始质心尽量分散，降低收敛到局部最优的概率。
///
/// 每个新质心以 D²（到最近已有质心的距离平方）为权重随机选取，
/// 距离已有质心越远的像素被选中的概率越高。
fn kmeans_plus_plus_init(pixels: &[[u8; 3]], k: usize, rng: &mut SmallRng) -> Vec<[u8; 3]> {
    let mut centroids: Vec<[u8; 3]> = Vec::with_capacity(k);

    // 第一个质心：从所有像素中均匀随机选取
    centroids.push(pixels[rng.gen_range(0..pixels.len())]);

    for _ in 1..k {
        // 计算每个像素到最近质心的距离平方（D² 权重）
        let weights: Vec<f64> = pixels
            .iter()
            .map(|p| {
                centroids
                    .iter()
                    .map(|c| Color::sq_distance_rgb(p, c))
                    .fold(f64::MAX, f64::min)
            })
            .collect();

        let total: f64 = weights.iter().sum();
        if total == 0.0 {
            // 所有像素颜色完全相同，重复质心无妨
            centroids.push(pixels[rng.gen_range(0..pixels.len())]);
            continue;
        }

        // 轮盘赌加权随机选取下一个质心
        let mut dart = rng.gen::<f64>() * total;
        let mut chosen = pixels.len() - 1;
        for (i, &w) in weights.iter().enumerate() {
            dart -= w;
            if dart <= 0.0 {
                chosen = i;
                break;
            }
        }
        centroids.push(pixels[chosen]);
    }

    centroids
}

/// 将每个像素分配给最近质心的索引（使用平方欧氏距离，避免开方）。
fn assign_pixels(pixels: &[[u8; 3]], centroids: &[[u8; 3]]) -> Vec<usize> {
    pixels
        .iter()
        .map(|p| {
            centroids
                .iter()
                .enumerate()
                .map(|(i, c)| (i, Color::sq_distance_rgb(p, c)))
                .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap())
                .map(|(i, _)| i)
                .unwrap_or(0)
        })
        .collect()
}

/// 将每个质心更新为其分配像素的均值。
///
/// 若某个簇变为空簇（dead centroid），则从像素最多的簇中随机借用一个像素作为新质心，
/// 防止有效聚类数量减少。
fn update_centroids(
    pixels: &[[u8; 3]],
    assignments: &[usize],
    k: usize,
    rng: &mut SmallRng,
) -> Vec<[u8; 3]> {
    // 累加每个簇的颜色分量之和及像素计数
    let mut sums = vec![[0u64; 3]; k];
    let mut counts = vec![0usize; k];

    for (&p, &idx) in pixels.iter().zip(assignments.iter()) {
        sums[idx][0] += p[0] as u64;
        sums[idx][1] += p[1] as u64;
        sums[idx][2] += p[2] as u64;
        counts[idx] += 1;
    }

    // 找出像素最多的簇，用于空簇恢复
    let largest = counts
        .iter()
        .enumerate()
        .max_by_key(|(_, &c)| c)
        .map(|(i, _)| i)
        .unwrap_or(0);

    (0..k)
        .map(|i| {
            if counts[i] == 0 {
                // 空簇恢复：从最大簇中随机抽取一个像素作为新质心
                let candidates: Vec<[u8; 3]> = pixels
                    .iter()
                    .zip(assignments.iter())
                    .filter(|(_, &a)| a == largest)
                    .map(|(&p, _)| p)
                    .collect();
                if candidates.is_empty() {
                    [128, 128, 128] // 兜底：返回中灰色
                } else {
                    candidates[rng.gen_range(0..candidates.len())]
                }
            } else {
                // 正常更新：计算分量均值
                let n = counts[i] as u64;
                [
                    (sums[i][0] / n) as u8,
                    (sums[i][1] / n) as u8,
                    (sums[i][2] / n) as u8,
                ]
            }
        })
        .collect()
}

// ── 单元测试 ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(k: usize) -> Config {
        Config::default()
            .max_colors(k)
            .sample_size(None)
            .kmeans_seed(0)
            .kmeans_max_iterations(200)
    }

    #[test]
    fn test_extract_single_color() {
        // 纯色图片：第一个颜色的占比应超过 90%
        let pixels = vec![[255u8, 0, 0]; 50];
        let palette = extract(&pixels, &cfg(3)).unwrap();
        assert!(!palette.is_empty());
        let top = &palette[0];
        assert!(top.percentage > 0.9, "纯色图片：首位颜色占比应超过 90%");
    }

    #[test]
    fn test_extract_two_clusters() {
        // 蓝色 50 个 + 红色 50 个：两个簇各占约 50%
        let mut pixels = vec![[0u8, 0, 255]; 50];
        pixels.extend(vec![[255u8, 0, 0]; 50]);
        let palette = extract(&pixels, &cfg(2)).unwrap();
        assert_eq!(palette.len(), 2);
        for c in &palette {
            assert!((c.percentage - 0.5).abs() < 0.15, "每个簇应占约 50%");
        }
    }

    #[test]
    fn test_percentages_sum_to_one() {
        // 所有颜色占比之和应精确等于 1.0
        let pixels: Vec<[u8; 3]> = (0..255u8).map(|i| [i, i, i]).collect();
        let palette = extract(&pixels, &cfg(5)).unwrap();
        let total: f32 = palette.iter().map(|c| c.percentage).sum();
        assert!((total - 1.0).abs() < 1e-5, "占比之和应为 1.0，实际 {total}");
    }

    #[test]
    fn test_k_clamped_to_pixel_count() {
        // 只有 2 种唯一像素时，请求 10 种颜色应优雅降级
        let pixels = vec![[0u8, 0, 0], [255, 255, 255]];
        let palette = extract(&pixels, &cfg(10)).unwrap();
        assert!(palette.len() <= 2);
    }

    #[test]
    fn test_kmeans_plus_plus_k_equals_1() {
        // k=1 时 K-Means++ 应只返回 1 个质心
        let pixels = vec![[1u8, 2, 3]; 10];
        let mut rng = SmallRng::seed_from_u64(0);
        let centroids = kmeans_plus_plus_init(&pixels, 1, &mut rng);
        assert_eq!(centroids.len(), 1);
    }

    #[test]
    fn test_assign_pixels_nearest() {
        // [0,0,0] 更接近质心 [0,0,0]；[200,200,200] 更接近质心 [255,255,255]
        let pixels = vec![[0u8, 0, 0], [200, 200, 200]];
        let centroids = vec![[0u8, 0, 0], [255, 255, 255]];
        let assignments = assign_pixels(&pixels, &centroids);
        assert_eq!(assignments, vec![0, 1]);
    }

    #[test]
    fn test_deterministic_with_same_seed() {
        // 相同种子应产生完全相同的结果（可复现性）
        let pixels: Vec<[u8; 3]> = (0..100u8)
            .map(|i| [i, i.wrapping_mul(2), i.wrapping_mul(3)])
            .collect();
        let palette1 = extract(&pixels, &cfg(4)).unwrap();
        let palette2 = extract(&pixels, &cfg(4)).unwrap();
        for (a, b) in palette1.iter().zip(palette2.iter()) {
            assert_eq!((a.r, a.g, a.b), (b.r, b.g, b.b), "相同种子结果应一致");
        }
    }
}
