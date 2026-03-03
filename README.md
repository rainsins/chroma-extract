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
