# dominant-colors

> 用三种经典算法从图片中提取主色调，纯 Rust 惯用风格实现。

[![Crates.io](https://img.shields.io/crates/v/dominant-colors)](https://crates.io/crates/dominant-colors)
[![docs.rs](https://docs.rs/dominant-colors/badge.svg)](https://docs.rs/dominant-colors)
[![License: MIT OR Apache-2.0](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue)](LICENSE)

## 算法特性对比

| 算法             | 速度 | 确定性 | 说明                          |
|-----------------|------|--------|-------------------------------|
| **K-Means**     | 中等 | 否（可设种子） | 质量最高；K-Means++ 初始化    |
| **Median Cut**  | 快   | 是     | 经典算法；自然照片效果好      |
| **Octree**      | 快   | 是     | 内存高效；适合 Logo 等图片    |

## 快速上手

```toml
[dependencies]
dominant-colors = "0.1"
image = "0.25"
```

```rust
use dominant_colors::{DominantColors, Algorithm, Config};

fn main() {
    let img = image::open("photo.jpg").unwrap();

    let palette = DominantColors::new(img)
        .config(Config::default().max_colors(6))
        .extract(Algorithm::KMeans)
        .unwrap();

    for color in &palette {
        println!("{}", color);  // #FF4B2B (32.7%)
    }
}
```

## 运行示例

```bash
cargo run --example extract -- photo.jpg 8
```

## 运行基准测试

```bash
cargo bench
```

## 基准测试报告

### 测试环境
- **处理器**: AMD R5 4600U
- **内存**: 16G
- **操作系统**: windows 11
- **Rust 版本**: v1.94.0

### 性能概览

我们对三种颜色量化算法（K‑Means、Median Cut、Octree）进行了多维度基准测试，包括图像尺寸、目标颜色数量、图像类型以及采样优化效果。主要发现如下：

- **速度排名（所有场景）**：**Median Cut** 最快，**Octree** 次之，**K‑Means** 最慢。
- **伸缩性**：  
  - K‑Means 对图像尺寸和颜色数量增长最敏感，时间呈超线性增长。  
  - Octree 在处理大尺寸图像（>6.5万像素）时表现优异，甚至超越 K‑Means。  
  - Median Cut 始终维持极低的延迟，尤其适合实时或对速度要求极高的场景。
- **采样优化**：对 K‑Means 和 Median Cut 效果显著，对 Octree 影响较小（因其本身对输入规模不敏感）。
- **图像类型**：均匀色块处理最快，随机噪声最慢；但算法间的相对排名不变。

### 详细基准测试结果

所有时间均取 **中位数**（单位：µs 或 ms），括号内为最小值/最大值。

#### 1. 图像尺寸对性能的影响（目标颜色数 k=16）

| 像素数量 | 对应尺寸 | K‑Means | Median Cut | Octree |
|---------|---------|---------|------------|--------|
| 1024    | 32×32   | 445.60 µs (444.91–446.36) | **48.36 µs** (48.349–48.381) | 372.60 µs (371.99–373.23) |
| 4096    | 64×64   | 1.751 ms (1.749–1.753)    | **181.64 µs** (181.45–181.86) | 1.500 ms (1.498–1.503) |
| 16384   | 128×128 | 6.769 ms (6.759–6.782)    | **733.35 µs** (733.10–733.66) | 3.423 ms (3.414–3.434) |
| 65536   | 256×256 | 25.89 ms (25.85–25.94)    | **2.973 ms** (2.968–2.978)    | 5.444 ms (5.431–5.461) |
| 262144  | 512×512 | 94.38 ms (94.20–94.59)    | **11.79 ms** (11.77–11.81)    | 9.93 ms (9.913–9.946) |

**分析**：  
- Median Cut 在所有尺寸下均保持最快，且增长曲线最平缓。  
- Octree 在大尺寸（≥65536像素）上反超 K‑Means，显示其更适合处理高分辨率图像。  
- K‑Means 随像素数增加而急剧变慢，从 1024 到 262144 像素，耗时增长约 212 倍。

---

#### 2. 颜色数量对性能的影响（固定图像 512×512）

| k   | K‑Means | Median Cut | Octree |
|-----|---------|------------|--------|
| 2   | 1.701 ms (1.700–1.703) | **341.4 µs** (341.1–341.7) | 3.427 ms (3.420–3.435) |
| 4   | 2.235 ms (2.234–2.237) | **586.7 µs** (586.3–587.2) | 3.432 ms (3.424–3.442) |
| 8   | 6.820 ms (6.796–6.848) | **726.2 µs** (726.0–726.4) | 3.479 ms (3.451–3.511) |
| 16  | 28.55 ms (28.44–28.70) | **823.1 µs** (822.4–823.9) | 3.409 ms (3.402–3.417) |
| 32  | 49.97 ms (49.82–50.16) | **985.7 µs** (985.2–986.3) | 3.413 ms (3.404–3.424) |

**分析**：  
- Median Cut 对 k 的增长不敏感，从 k=2 到 k=32 仅增加约 2.9 倍时间。  
- Octree 时间基本恒定，与 k 无关（因其基于树结构，颜色数由叶子节点数决定）。  
- K‑Means 对 k 高度敏感，k=32 时耗时约为 k=2 的 29 倍。

---

#### 3. 采样优化效果（512×512 图像，k=16）

| 算法       | 原始尺寸 (512²) | 采样至 256² | 加速比 |
|-----------|-----------------|-------------|--------|
| K‑Means   | 93.77 ms        | **24.86 ms** | 3.77×  |
| Median Cut| 11.80 ms        | **4.322 ms** | 2.73×  |
| Octree    | 9.938 ms        | **8.681 ms** | 1.14×  |

**分析**：采样对 K‑Means 和 Median Cut 加速效果明显，尤其 K‑Means 受益最大；Octree 由于本身对输入规模不敏感，加速有限。

---

#### 4. 图像类型对性能的影响（256×256 图像，k=16）

| 图像类型   | K‑Means   | Median Cut | Octree    |
|-----------|-----------|------------|-----------|
| 渐变       | 6.783 ms  | **724.7 µs** | 3.464 ms  |
| 均匀色块   | 2.052 ms  | **359.2 µs** | 370.4 µs  |
| 随机噪声   | 21.88 ms  | **1.016 ms** | 11.80 ms  |

**分析**：  
- 均匀色块（颜色数少）处理最快，随机噪声（颜色丰富）最慢。  
- Median Cut 在所有类型中均保持领先，尤其擅长处理渐变和噪声这类复杂图像。  
- Octree 在均匀色块上与 Median Cut 接近，但在噪声下明显变慢，但仍优于 K‑Means。

### 建议

- **追求极致速度**：选择 **Median Cut**，它在所有测试场景中均表现最佳，尤其适合实时应用或大尺寸图像。
- **需要高质量调色板且可接受中等速度**：选择 **Octree**，它在大尺寸图像上优于 K‑Means，且对颜色数量不敏感。
- **需要灵活调整颜色数且可接受较慢速度**：选择 **K‑Means**（但建议配合采样使用），它通常能产生更高质量的调色板（非本次测试重点）。

**采样建议**：对于 K‑Means 和 Median Cut，对输入图像进行适当下采样可大幅提升性能，而质量损失可接受。推荐在图像尺寸超过 256×256 时启用采样。


## 算法详解

### K-Means 聚类

将像素视为 RGB 三维空间中的点。使用 **K-Means++** 策略初始化质心（按 D² 权重随机选取，使初始质心尽量分散），然后交替执行"将像素分配给最近质心"和"将质心更新为簇均值"两个步骤，直到收敛。空簇通过从最大簇中随机借用像素来恢复。

### 中位切分（Median Cut）

所有像素初始放入一个包围盒。每次选取像素最多的包围盒，按颜色范围最大的通道（R/G/B）排序后在中位切分，直到产生 `k` 个包围盒。每个包围盒的像素均值即为代表色。

### 八叉树量化（Octree）

八叉树将 24 位 RGB 空间映射为深度 8 的树，每层用 R/G/B 各一位（共 3 位）决定走向哪个子节点。像素插入时同步进行**即时规约**：叶节点数超过预算时，将最深层拥有多个叶子子节点的内部节点的所有叶子合并到该父节点，以控制内存峰值。

## 配置说明

```rust
let config = Config::default()
    .max_colors(8)            // 提取的颜色数量
    .sample_size(Some(256))   // 缩放最长边到 256px（None = 原始尺寸）
    .kmeans_seed(42)          // K-Means 随机种子
    .kmeans_max_iterations(100); // K-Means 最大迭代次数
```

## 错误处理

```rust
use dominant_colors::DominantColorError;

match DominantColors::new(img).extract(Algorithm::Octree) {
    Ok(palette) => { /* 处理调色板 */ }
    Err(DominantColorError::EmptyImage) => eprintln!("图片不含任何像素"),
    Err(DominantColorError::TooFewColors { requested, available }) => {
        eprintln!("请求 {requested} 种颜色，但只有 {available} 种唯一颜色");
    }
    Err(DominantColorError::InternalError { message }) => {
        eprintln!("内部错误（请提交 issue）：{message}");
    }
}
```

## 许可证

MIT许可。
