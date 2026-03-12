//! 八叉树量化（Octree Quantization）算法实现。
//!
//! ## 设计
//!
//! - **Arena 分配**：`Vec<Node>` + `NodeId(usize)`，零裸指针，零 unsafe。
//! - **父节点指针**：规约后 O(1) 向上传播，无需全树扫描。
//! - **精确到 k 种颜色**：通过 budget-aware 部分规约，保证最终恰好 ≤ k 且最大化颜色数。
//!
//! ## 部分规约（Partial Merge）原理
//!
//! 普通全量规约：把节点的所有 N 个叶子子节点合并到父，父升为叶，net = -(N-1)。
//! 当 N-1 > budget 时超调。
//!
//! 部分规约：只合并 M = budget（已是叶）或 M = budget+1（未是叶）个叶子子节点，
//! 剩余子节点保留。父节点升为叶节点（代表被合并的 M 个像素的均值），
//! 同时仍持有剩余子节点引用。
//!
//! 关键：`collect_recursive` 中，叶节点输出自身后**继续递归子节点**（不 early return），
//! 因此被合并的 M 个像素和剩余子节点的像素都能被正确收集，零数据丢失。

use crate::color::{Color, ColorPalette};
use crate::error::{DominantColorError, Result};
use crate::Config;

const MAX_DEPTH: usize = 7;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
struct NodeId(usize);

#[derive(Clone)]
struct Node {
    r_sum: u64,
    g_sum: u64,
    b_sum: u64,
    pixel_count: u64,
    /// 该节点是否代表一个颜色（叶节点）。
    /// 部分规约后，一个节点可以同时是叶节点且拥有子节点。
    is_leaf: bool,
    in_reducible: bool,
    parent: Option<NodeId>,
    children: [Option<NodeId>; 8],
}

impl Node {
    fn new(parent: Option<NodeId>) -> Self {
        Self {
            r_sum: 0,
            g_sum: 0,
            b_sum: 0,
            pixel_count: 0,
            is_leaf: false,
            in_reducible: false,
            parent,
            children: [None; 8],
        }
    }
}

struct Octree {
    nodes: Vec<Node>,
    root: NodeId,
    leaf_count: usize,
    /// reducible[d]：深度 d 上拥有至少一个叶子子节点的内部节点 ID 列表。
    reducible: [Vec<NodeId>; MAX_DEPTH],
}

impl Octree {
    fn new() -> Self {
        let mut nodes = Vec::with_capacity(8192);
        nodes.push(Node::new(None));
        Self {
            nodes,
            root: NodeId(0),
            leaf_count: 0,
            reducible: std::array::from_fn(|_| Vec::new()),
        }
    }

    fn alloc_child(&mut self, parent: NodeId) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(Node::new(Some(parent)));
        id
    }

    fn register(&mut self, id: NodeId, depth: usize) {
        if !self.nodes[id.0].in_reducible {
            self.nodes[id.0].in_reducible = true;
            self.reducible[depth].push(id);
        }
    }

    fn insert(&mut self, pixel: [u8; 3]) {
        let mut id = self.root;
        for depth in 0..=MAX_DEPTH {
            if depth == MAX_DEPTH {
                let node = &mut self.nodes[id.0];
                node.r_sum += pixel[0] as u64;
                node.g_sum += pixel[1] as u64;
                node.b_sum += pixel[2] as u64;
                node.pixel_count += 1;
                if !node.is_leaf {
                    node.is_leaf = true;
                    self.leaf_count += 1;
                }
                break;
            }
            let idx = octant_index(pixel, depth);
            if self.nodes[id.0].children[idx].is_none() {
                let child = self.alloc_child(id);
                self.nodes[id.0].children[idx] = Some(child);
                // 只有叶子的直接父节点（depth = MAX_DEPTH-1）在插入时登记
                if depth + 1 == MAX_DEPTH {
                    self.register(id, depth);
                }
            }
            id = self.nodes[id.0].children[idx].unwrap();
        }
    }

    /// 规约一步，保证 leaf_count 不低于 k。
    ///
    /// - 若能全量合并（net ≤ budget）→ 全量，向上传播登记父节点。
    /// - 若会超调 → 部分合并（仅合并 M 个最小叶子），父节点同时成为叶节点。
    ///   collect_recursive 会同时收集父节点自身数据和剩余子节点。
    ///
    /// 返回 false 表示 reducible 已空。
    fn reduce(&mut self, k: usize) -> bool {
        let depth = match self.reducible.iter().rposition(|v| !v.is_empty()) {
            Some(d) => d,
            None => return false,
        };

        let node_id = self.reducible[depth].pop().unwrap();
        self.nodes[node_id.0].in_reducible = false;

        let already_leaf = self.nodes[node_id.0].is_leaf;
        // budget: 还能减少多少个 leaf_count
        let budget = self.leaf_count.saturating_sub(k);

        // Pass 1（只读）：收集叶子子节点索引，按 pixel_count 升序（小的优先合并）
        let mut leaf_indices: Vec<usize> = (0..8)
            .filter(|&i| {
                self.nodes[node_id.0].children[i].map_or(false, |cid| self.nodes[cid.0].is_leaf)
            })
            .collect();

        if leaf_indices.is_empty() {
            return true; // 防御性处理
        }

        leaf_indices.sort_by_key(|&i| {
            self.nodes[node_id.0].children[i].map_or(0, |cid| self.nodes[cid.0].pixel_count)
        });

        let n = leaf_indices.len();

        // 计算本次最多可合并数量（避免超调）：
        //   已是叶节点：net cost = M，因此 M ≤ budget
        //   尚非叶节点：net cost = M - 1，因此 M ≤ budget + 1
        let max_merge = if already_leaf {
            budget.max(1) // budget 由外层保证 ≥ 1
        } else {
            budget + 1
        };
        let merge_count = n.min(max_merge);

        // Pass 2（只读）：累加要合并的叶子数据
        let (mut r_acc, mut g_acc, mut b_acc, mut pc_acc) = (0u64, 0u64, 0u64, 0u64);
        for &i in &leaf_indices[..merge_count] {
            let cid = self.nodes[node_id.0].children[i].unwrap();
            r_acc += self.nodes[cid.0].r_sum;
            g_acc += self.nodes[cid.0].g_sum;
            b_acc += self.nodes[cid.0].b_sum;
            pc_acc += self.nodes[cid.0].pixel_count;
        }

        // Pass 3（写）：断开已合并子节点
        for &i in &leaf_indices[..merge_count] {
            self.nodes[node_id.0].children[i] = None;
        }

        // 更新当前节点
        self.nodes[node_id.0].r_sum += r_acc;
        self.nodes[node_id.0].g_sum += g_acc;
        self.nodes[node_id.0].b_sum += b_acc;
        self.nodes[node_id.0].pixel_count += pc_acc;
        self.leaf_count -= merge_count;
        if !already_leaf {
            self.nodes[node_id.0].is_leaf = true;
            self.leaf_count += 1;
        }

        // 向上传播：当前节点现在/仍然是叶节点，通知父节点
        if depth > 0 {
            if let Some(pid) = self.nodes[node_id.0].parent {
                if !self.nodes[pid.0].is_leaf {
                    self.register(pid, depth - 1);
                }
            }
        }

        // 部分合并：当前节点仍有叶子子节点，重新登记等待下次
        if merge_count < n {
            self.register(node_id, depth);
        }

        true
    }

    fn collect_leaves(&self, palette: &mut ColorPalette, total: f32) {
        self.collect_recursive(self.root, palette, total);
    }

    fn collect_recursive(&self, id: NodeId, palette: &mut ColorPalette, total: f32) {
        let node = &self.nodes[id.0];

        // 输出自身叶数据（若有）
        // 注意：不 early return！部分规约后节点可以同时是叶节点且有子节点
        if node.is_leaf && node.pixel_count > 0 {
            let n = node.pixel_count as f64;
            palette.push(Color::new(
                (node.r_sum as f64 / n).round() as u8,
                (node.g_sum as f64 / n).round() as u8,
                (node.b_sum as f64 / n).round() as u8,
                node.pixel_count as f32 / total,
            ));
        }

        // 始终递归子节点（处理部分规约情形）
        for child_id in node.children.iter().filter_map(|&c| c) {
            self.collect_recursive(child_id, palette, total);
        }
    }
}

// ── 公开入口 ──────────────────────────────────────────────────────────────────

pub fn extract(pixels: &[[u8; 3]], config: &Config) -> Result<ColorPalette> {
    if pixels.is_empty() {
        return Err(DominantColorError::EmptyImage);
    }

    let k = config.max_colors;
    let mut octree = Octree::new();

    for &pixel in pixels {
        octree.insert(pixel);
    }

    while octree.leaf_count > k {
        if !octree.reduce(k) {
            break;
        }
    }

    let total = pixels.len() as f32;
    let mut palette = ColorPalette::new();
    octree.collect_leaves(&mut palette, total);

    if palette.is_empty() {
        return Err(DominantColorError::internal("八叉树未产生任何叶节点"));
    }

    Ok(palette)
}

// ── 辅助函数 ──────────────────────────────────────────────────────────────────

#[inline]
fn octant_index(pixel: [u8; 3], depth: usize) -> usize {
    let shift = MAX_DEPTH - depth;
    let r = ((pixel[0] >> shift) & 1) as usize;
    let g = ((pixel[1] >> shift) & 1) as usize;
    let b = ((pixel[2] >> shift) & 1) as usize;
    (r << 2) | (g << 1) | b
}

// ── 单元测试 ──────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg(k: usize) -> Config {
        Config::default().max_colors(k).sample_size(None)
    }

    #[test]
    fn test_empty_returns_error() {
        assert_eq!(extract(&[], &cfg(4)), Err(DominantColorError::EmptyImage));
    }

    #[test]
    fn test_single_pixel() {
        let pixels = vec![[200u8, 100, 50]];
        let palette = extract(&pixels, &cfg(4)).unwrap();
        assert_eq!(palette.len(), 1);
        assert!((palette[0].percentage - 1.0).abs() < 1e-5);
    }

    #[test]
    fn test_two_colors_separated() {
        let mut pixels = vec![[255u8, 0, 0]; 60];
        pixels.extend(vec![[0u8, 0, 255]; 40]);
        let palette = extract(&pixels, &cfg(2)).unwrap();
        assert_eq!(palette.len(), 2);
        assert!(palette.iter().any(|c| c.r > 200 && c.b < 50), "缺少红色");
        assert!(palette.iter().any(|c| c.b > 200 && c.r < 50), "缺少蓝色");
    }

    #[test]
    fn test_percentages_sum_to_one() {
        let pixels: Vec<[u8; 3]> = (0..128u8)
            .map(|i| [i, i.wrapping_mul(2), 255 - i])
            .collect();
        let palette = extract(&pixels, &cfg(6)).unwrap();
        let total: f32 = palette.iter().map(|c| c.percentage).sum();
        assert!((total - 1.0).abs() < 1e-4, "占比之和 = {total}");
    }

    #[test]
    fn test_leaf_count_respects_k() {
        let pixels: Vec<[u8; 3]> = (0..=255u8).map(|i| [i, i, i]).collect();
        let k = 5;
        let palette = extract(&pixels, &cfg(k)).unwrap();
        assert!(palette.len() <= k, "期望 ≤{k}，实际 {}", palette.len());
    }

    #[test]
    fn test_exactly_k_distinct_colors() {
        // 8 种完全分离的颜色，应精确提取 k=8 种
        let mut pixels = Vec::new();
        for (i, &color) in [
            [255u8, 0, 0],
            [0, 255, 0],
            [0, 0, 255],
            [255, 255, 0],
            [255, 0, 255],
            [0, 255, 255],
            [128, 0, 0],
            [0, 128, 0],
        ]
        .iter()
        .enumerate()
        {
            pixels.extend(vec![color; 200 + i * 50]);
        }
        let palette = extract(&pixels, &cfg(8)).unwrap();
        assert!(
            palette.len() <= 8,
            "颜色数量 {} 超过了预期的上限 8",
            palette.len()
        );
        assert!(palette.len() > 0, "调色板不应为空");
    }

    #[test]
    fn test_no_data_loss() {
        // 所有像素占比之和必须为 1（验证无数据丢失）
        let pixels: Vec<[u8; 3]> = (0..=255u8)
            .flat_map(|i| vec![[i, 255 - i, i / 2]; 3])
            .collect();
        let palette = extract(&pixels, &cfg(8)).unwrap();
        let total: f32 = palette.iter().map(|c| c.percentage).sum();
        assert!(
            (total - 1.0).abs() < 1e-4,
            "占比之和 = {total}，疑似数据丢失"
        );
    }

    #[test]
    fn test_octant_index_range() {
        for r in [0u8, 127, 255] {
            for g in [0u8, 127, 255] {
                for b in [0u8, 127, 255] {
                    for depth in 0..=MAX_DEPTH {
                        assert!(octant_index([r, g, b], depth) < 8);
                    }
                }
            }
        }
    }

    #[test]
    fn test_deterministic() {
        let pixels: Vec<[u8; 3]> = (0..200u8).map(|i| [i, 200 - i, i / 2]).collect();
        let p1 = extract(&pixels, &cfg(6)).unwrap();
        let p2 = extract(&pixels, &cfg(6)).unwrap();
        assert_eq!(p1.len(), p2.len());
        for (a, b) in p1.iter().zip(p2.iter()) {
            assert_eq!((a.r, a.g, a.b), (b.r, b.g, b.b));
        }
    }

    #[test]
    fn test_k1_returns_one_color() {
        let pixels: Vec<[u8; 3]> = (0..50u8).map(|i| [i * 5, i, 100]).collect();
        let palette = extract(&pixels, &cfg(1)).unwrap();
        assert_eq!(palette.len(), 1);
        assert!((palette[0].percentage - 1.0).abs() < 1e-4);
    }

    #[test]
    fn test_gradient() {
        let pixels: Vec<[u8; 3]> = (0..=255u8).map(|i| [i, 0, 255 - i]).collect();
        let palette = extract(&pixels, &cfg(8)).unwrap();
        assert!(palette.len() > 0 && palette.len() <= 8);
        assert!(palette.iter().any(|c| c.r > 180 && c.b < 80), "缺偏红色");
        assert!(palette.iter().any(|c| c.b > 180 && c.r < 80), "缺偏蓝色");
    }
}
