//! 八叉树量化（Octree Quantization）算法实现。
//!
//! ## 算法原理
//!
//! 八叉树将 RGB 立方体递归划分为 8 个子立方体：
//! - 深度 0：根节点，代表整个 RGB 空间
//! - 深度 7：叶节点，每个节点对应一个 2³ 子立方体
//! - 深度 8：可精确表示单个 24 位颜色
//!
//! ## 算法流程
//!
//! 1. **插入**：逐像素遍历树——每层取 R、G、B 各一位（共 3 位）确定子节点方向，
//!    在叶节点处累加颜色分量之和与像素计数。
//! 2. **即时规约**：插入过程中，若叶节点数超过 `k × 8`，立即执行一次合并，
//!    将某个拥有多个叶子子节点的内部节点的所有叶子子节点合并到其父节点，
//!    从而控制内存峰值。
//! 3. **最终规约**：插入完成后，继续合并直到叶节点数 ≤ `k`。
//! 4. **收集**：遍历所有叶节点，以像素均值作为代表色构建调色板。
//!
//! 内存复杂度：最坏 O(8^d)，实际 O(n)（稀疏区域不创建子节点）。
//! 时间复杂度：O(n log n)。

use crate::color::{Color, ColorPalette};
use crate::error::{DominantColorError, Result};
use crate::Config;

/// 树的最大深度（每个通道 8 位 → 深度 0–7，叶节点在深度 7）。
const MAX_DEPTH: usize = 7;

/// 使用八叉树量化算法提取主色调。
pub fn extract(pixels: &[[u8; 3]], config: &Config) -> Result<ColorPalette> {
    if pixels.is_empty() {
        return Err(DominantColorError::EmptyImage);
    }

    let k = config.max_colors;
    let mut tree = OctreeNode::new();
    let mut leaf_count = 0usize;

    for &pixel in pixels {
        tree.insert(pixel, 0, &mut leaf_count);

        // 即时规约：叶节点数超过预算的 8 倍时触发，避免内存峰值
        while leaf_count > k * 8 {
            if !tree.reduce(&mut leaf_count) {
                break;
            }
        }
    }

    // 最终规约：将叶节点数压缩到 ≤ k
    while leaf_count > k {
        if !tree.reduce(&mut leaf_count) {
            break;
        }
    }

    let total = pixels.len() as f32;
    let mut palette = ColorPalette::new();
    tree.collect_leaves(&mut palette, total);

    if palette.is_empty() {
        return Err(DominantColorError::internal("八叉树未产生任何叶节点"));
    }

    Ok(palette)
}

// ── 八叉树节点 ────────────────────────────────────────────────────────────────

/// 八叉树的单个节点，可以是内部节点或叶节点。
struct OctreeNode {
    /// 该子树中所有像素的红色分量之和（用于计算均值）。
    r_sum: u64,
    /// 绿色分量之和。
    g_sum: u64,
    /// 蓝色分量之和。
    b_sum: u64,
    /// 该子树中的像素总数。
    pixel_count: u64,
    /// 是否为叶节点（无叶子子节点，直接存储颜色数据）。
    is_leaf: bool,
    /// 最多 8 个子节点（按 RGB 三位索引寻址）。
    children: Box<[Option<Box<OctreeNode>>; 8]>,
}

impl OctreeNode {
    fn new() -> Self {
        Self {
            r_sum: 0,
            g_sum: 0,
            b_sum: 0,
            pixel_count: 0,
            is_leaf: false,
            children: Box::new([None, None, None, None, None, None, None, None]),
        }
    }

    /// 在树的 `depth` 层插入 `pixel`，并更新 `leaf_count`。
    fn insert(&mut self, pixel: [u8; 3], depth: usize, leaf_count: &mut usize) {
        if depth == MAX_DEPTH {
            // 到达叶节点：累加颜色分量
            self.r_sum += pixel[0] as u64;
            self.g_sum += pixel[1] as u64;
            self.b_sum += pixel[2] as u64;
            self.pixel_count += 1;
            if !self.is_leaf {
                self.is_leaf = true;
                *leaf_count += 1;
            }
            return;
        }

        // 计算子节点索引并递归插入
        let idx = octant_index(pixel, depth);
        if self.children[idx].is_none() {
            self.children[idx] = Some(Box::new(OctreeNode::new()));
        }
        self.children[idx]
            .as_mut()
            .unwrap()
            .insert(pixel, depth + 1, leaf_count);
    }

    /// 将最深处拥有多个叶子子节点的内部节点的所有叶子合并到该节点自身。
    ///
    /// 返回 `true` 表示本次调用成功执行了一次合并。
    fn reduce(&mut self, leaf_count: &mut usize) -> bool {
        // 优先向更深层递归，优先合并最细粒度的节点
        for child in self.children.iter_mut().flatten() {
            if child.reduce(leaf_count) {
                return true;
            }
        }

        // 检查自身是否有多个叶子子节点可以合并
        let leaf_children: Vec<usize> = self
            .children
            .iter()
            .enumerate()
            .filter(|(_, c)| c.as_ref().map_or(false, |n| n.is_leaf))
            .map(|(i, _)| i)
            .collect();

        if leaf_children.len() <= 1 {
            return false; // 叶子子节点不足，无法在此合并
        }

        // 将所有叶子子节点合并到当前节点（标准八叉树规约）
        for i in leaf_children {
            let child = self.children[i].take().unwrap();
            self.r_sum += child.r_sum;
            self.g_sum += child.g_sum;
            self.b_sum += child.b_sum;
            self.pixel_count += child.pixel_count;
            *leaf_count = leaf_count.saturating_sub(1); // 每移除一个叶子 -1
        }

        // 当前节点升级为叶节点
        if !self.is_leaf {
            self.is_leaf = true;
            *leaf_count += 1;
        }

        true
    }

    /// 深度优先遍历，收集所有叶节点并构建调色板。
    fn collect_leaves(&self, palette: &mut ColorPalette, total: f32) {
        if self.is_leaf && self.pixel_count > 0 {
            let n = self.pixel_count as f64;
            let color = Color::new(
                (self.r_sum as f64 / n).round() as u8,
                (self.g_sum as f64 / n).round() as u8,
                (self.b_sum as f64 / n).round() as u8,
                self.pixel_count as f32 / total,
            );
            palette.push(color);
            return;
        }
        // 递归收集子节点
        for child in self.children.iter().flatten() {
            child.collect_leaves(palette, total);
        }
    }
}

/// 计算 `pixel` 在树 `depth` 层对应的子节点索引（0–7）。
///
/// 每层从每个通道的当前位（MSB 优先）各取 1 位，拼成 3 位索引：
/// `index = (r_bit << 2) | (g_bit << 1) | b_bit`
#[inline]
fn octant_index(pixel: [u8; 3], depth: usize) -> usize {
    let shift = MAX_DEPTH - depth; // 第 0 层取最高位（bit 7），第 7 层取最低位（bit 0）
    let r_bit = ((pixel[0] >> shift) & 1) as usize;
    let g_bit = ((pixel[1] >> shift) & 1) as usize;
    let b_bit = ((pixel[2] >> shift) & 1) as usize;
    (r_bit << 2) | (g_bit << 1) | b_bit
}

// ── 单元测试 ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(k: usize) -> Config {
        Config::default().max_colors(k).sample_size(None)
    }

    #[test]
    fn test_empty_returns_error() {
        // 空像素集应返回 EmptyImage 错误
        assert_eq!(extract(&[], &cfg(4)), Err(DominantColorError::EmptyImage));
    }

    #[test]
    fn test_single_pixel() {
        // 单像素：返回 1 种颜色，占比为 100%
        let pixels = vec![[200u8, 100, 50]];
        let palette = extract(&pixels, &cfg(4)).unwrap();
        assert_eq!(palette.len(), 1);
        assert!((palette[0].percentage - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_two_colors_separated() {
        // 红色 60 个 + 蓝色 40 个：应识别出红、蓝两种颜色
        let mut pixels = vec![[255u8, 0, 0]; 60];
        pixels.extend(vec![[0u8, 0, 255]; 40]);
        let palette = extract(&pixels, &cfg(2)).unwrap();
        assert_eq!(palette.len(), 2);
        let red = palette.iter().find(|c| c.r > 200 && c.b < 50);
        let blue = palette.iter().find(|c| c.b > 200 && c.r < 50);
        assert!(red.is_some(), "未找到红色簇：{palette:?}");
        assert!(blue.is_some(), "未找到蓝色簇：{palette:?}");
    }

    #[test]
    fn test_percentages_sum_to_one() {
        // 各颜色占比之和应精确等于 1.0
        let pixels: Vec<[u8; 3]> = (0..128u8)
            .map(|i| [i, i.wrapping_mul(2), 255 - i])
            .collect();
        let palette = extract(&pixels, &cfg(6)).unwrap();
        let total: f32 = palette.iter().map(|c| c.percentage).sum();
        assert!((total - 1.0).abs() < 1e-4, "占比之和 = {total}");
    }

    #[test]
    fn test_leaf_count_respects_k() {
        // 最终颜色数量不应超过 k
        let pixels: Vec<[u8; 3]> = (0..=255u8).map(|i| [i, i, i]).collect();
        let k = 5;
        let palette = extract(&pixels, &cfg(k)).unwrap();
        assert!(
            palette.len() <= k,
            "期望 ≤{k} 种颜色，实际得到 {} 种",
            palette.len()
        );
    }

    #[test]
    fn test_octant_index_range() {
        // octant_index 在所有可能输入下应始终返回 0–7 范围内的值
        for r in [0u8, 127, 255] {
            for g in [0u8, 127, 255] {
                for b in [0u8, 127, 255] {
                    for depth in 0..=MAX_DEPTH {
                        let idx = octant_index([r, g, b], depth);
                        assert!(idx < 8, "深度 {depth} 时 octant 越界");
                    }
                }
            }
        }
    }

    #[test]
    fn test_deterministic() {
        // 算法无随机性，相同输入应产生完全相同的结果
        let pixels: Vec<[u8; 3]> = (0..200u8).map(|i| [i, 200 - i, i / 2]).collect();
        let p1 = extract(&pixels, &cfg(6)).unwrap();
        let p2 = extract(&pixels, &cfg(6)).unwrap();
        assert_eq!(p1.len(), p2.len());
        for (a, b) in p1.iter().zip(p2.iter()) {
            assert_eq!((a.r, a.g, a.b), (b.r, b.g, b.b), "相同输入结果应一致");
        }
    }

    #[test]
    fn test_k1_returns_one_color() {
        // k=1 时应将所有像素合并为 1 种颜色，占比为 100%
        let pixels: Vec<[u8; 3]> = (0..50u8).map(|i| [i * 5, i, 100]).collect();
        let palette = extract(&pixels, &cfg(1)).unwrap();
        assert_eq!(palette.len(), 1);
        assert!((palette[0].percentage - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_many_colors_gradient() {
        // 256 种唯一颜色的红蓝渐变：应提取出偏红和偏蓝两种极端色
        let pixels: Vec<[u8; 3]> = (0..=255u8).map(|i| [i, 0, 255 - i]).collect();
        let k = 8;
        let palette = extract(&pixels, &cfg(k)).unwrap();
        assert!(palette.len() > 0 && palette.len() <= k);
        let has_reddish = palette.iter().any(|c| c.r > 180 && c.b < 80);
        let has_bluish = palette.iter().any(|c| c.b > 180 && c.r < 80);
        assert!(has_reddish, "渐变中应包含偏红色");
        assert!(has_bluish, "渐变中应包含偏蓝色");
    }
}
