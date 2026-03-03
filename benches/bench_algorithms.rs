//! 基准测试套件：全面评估三种主色调提取算法的性能特征。
//!
//! # 运行方式
//!
//! ```bash
//! # 运行全部基准测试，生成 HTML 报告（target/criterion/）
//! cargo bench
//!
//! # 只运行特定测试组
//! cargo bench -- "图片尺寸"
//! cargo bench -- "颜色数量"
//! cargo bench -- "采样开销"
//! cargo bench -- "图片类型"
//! ```
//!
//! # 测试维度
//!
//! | 组名       | 测试目的                         |
//! |-----------|----------------------------------|
//! | 图片尺寸   | 像素数量对各算法吞吐量的影响      |
//! | 颜色数量   | `k` 值变化对各算法耗时的影响      |
//! | 采样开销   | 图片缩放步骤本身的开销占比        |
//! | 图片类型   | 不同颜色分布（渐变/均匀/噪声）的影响 |

use criterion::{
    black_box, criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion,
    PlotConfiguration,
};
use dominant_colors::{Algorithm, Config, DominantColors};
use image::{DynamicImage, ImageBuffer, Rgb};

// ── 测试图片生成 ───────────────────────────────────────────────────────────────

/// 平滑渐变图：颜色随 x/y 坐标线性变化，模拟自然照片的颜色分布。
fn make_gradient(size: u32) -> DynamicImage {
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(size, size, |x, y| {
        Rgb([
            (x * 255 / size) as u8,
            (y * 255 / size) as u8,
            (255u32.saturating_sub((x + y) * 255 / (size * 2))) as u8,
        ])
    }))
}

/// 均匀色块图：图片被划分为 4×4 = 16 个纯色方块，模拟卡通/插画风格。
fn make_uniform_blocks(size: u32) -> DynamicImage {
    // 16 种预设颜色，覆盖色相环
    const COLORS: [[u8; 3]; 16] = [
        [255, 0, 0],   [0, 255, 0],   [0, 0, 255],   [255, 255, 0],
        [0, 255, 255], [255, 0, 255], [128, 0, 0],   [0, 128, 0],
        [0, 0, 128],   [128, 128, 0], [0, 128, 128], [128, 0, 128],
        [255, 128, 0], [0, 255, 128], [128, 0, 255], [64, 64, 64],
    ];
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(size, size, |x, y| {
        // 按 4×4 方块索引选取颜色
        let block = (y / (size / 4)) * 4 + (x / (size / 4));
        let c = COLORS[(block as usize).min(15)];
        Rgb(c)
    }))
}

/// 随机噪声图：每个像素独立随机，模拟颜色极度分散的最坏情况。
///
/// 使用确定性的线性同余生成器（LCG）以保证基准可复现。
fn make_noise(size: u32) -> DynamicImage {
    let mut state: u64 = 0xDEAD_BEEF_1234_5678;
    DynamicImage::ImageRgb8(ImageBuffer::from_fn(size, size, |_, _| {
        // 简单 LCG，足够生成视觉上均匀的噪声
        state = state.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        let r = (state >> 56) as u8;
        let g = (state >> 48) as u8;
        let b = (state >> 40) as u8;
        Rgb([r, g, b])
    }))
}

// ── 基准测试组 1：图片尺寸 ────────────────────────────────────────────────────

/// 测试像素数量对各算法吞吐量的影响（固定 k=8，禁用采样）。
///
/// 尺寸从 32×32 到 512×512，覆盖 ~1K 到 ~262K 个像素。
fn bench_image_sizes(c: &mut Criterion) {
    let mut group = c.benchmark_group("图片尺寸");
    group.plot_config(PlotConfiguration::default().summary_scale(AxisScale::Logarithmic));

    let sizes = [32u32, 64, 128, 256, 512];
    let cfg = Config::default().max_colors(8).sample_size(None);

    for &size in &sizes {
        let img = make_gradient(size);
        let pixel_count = size * size;

        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            group.bench_with_input(
                BenchmarkId::new(format!("{alg:?}"), format!("{pixel_count}px")),
                &size,
                |b, _| {
                    b.iter(|| {
                        DominantColors::new(black_box(img.clone()))
                            .config(cfg.clone())
                            .extract(black_box(alg))
                            .unwrap()
                    })
                },
            );
        }
    }

    group.finish();
}

// ── 基准测试组 2：颜色数量（k）─────────────────────────────────────────────────

/// 测试 `k`（提取颜色数量）对各算法耗时的影响（固定 128×128 渐变图）。
///
/// K-Means 的迭代次数随 k 增大而增多；Octree 的规约次数同理；
/// Median Cut 的切分次数为 O(k)，增长最平稳。
fn bench_color_counts(c: &mut Criterion) {
    let mut group = c.benchmark_group("颜色数量");

    let img = make_gradient(128);
    let ks = [2usize, 4, 8, 16, 32];

    for &k in &ks {
        let cfg = Config::default().max_colors(k).sample_size(None);

        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            group.bench_with_input(
                BenchmarkId::new(format!("{alg:?}"), format!("k={k}")),
                &k,
                |b, _| {
                    b.iter(|| {
                        DominantColors::new(black_box(img.clone()))
                            .config(cfg.clone())
                            .extract(black_box(alg))
                            .unwrap()
                    })
                },
            );
        }
    }

    group.finish();
}

// ── 基准测试组 3：采样开销 ────────────────────────────────────────────────────

/// 测量图片缩放步骤本身的开销占整体耗时的比例。
///
/// 对比"启用 256px 采样"与"禁用采样（处理原图）"在 512×512 图片上的差异。
/// 采样能显著降低像素数（512²=262K → 256²=65K），理想情况下应有约 4× 加速。
fn bench_sampling_overhead(c: &mut Criterion) {
    let mut group = c.benchmark_group("采样开销");

    let img = make_gradient(512); // 较大图，体现采样收益

    let cfg_no_sample = Config::default().max_colors(8).sample_size(None);
    let cfg_sampled = Config::default().max_colors(8).sample_size(Some(256));

    for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
        // 不采样：处理完整 512×512 = 262144 像素
        group.bench_with_input(
            BenchmarkId::new(format!("{alg:?}"), "原始尺寸(512²)"),
            &alg,
            |b, &alg| {
                b.iter(|| {
                    DominantColors::new(black_box(img.clone()))
                        .config(cfg_no_sample.clone())
                        .extract(black_box(alg))
                        .unwrap()
                })
            },
        );

        // 采样至 256：处理 65536 像素
        group.bench_with_input(
            BenchmarkId::new(format!("{alg:?}"), "采样至256²"),
            &alg,
            |b, &alg| {
                b.iter(|| {
                    DominantColors::new(black_box(img.clone()))
                        .config(cfg_sampled.clone())
                        .extract(black_box(alg))
                        .unwrap()
                })
            },
        );
    }

    group.finish();
}

// ── 基准测试组 4：图片类型 ────────────────────────────────────────────────────

/// 测试不同颜色分布对各算法性能的影响（固定 128×128，k=8）。
///
/// - **渐变**：颜色连续均匀分布，接近自然照片
/// - **色块**：只有 16 种离散颜色，接近插画/卡通
/// - **噪声**：颜色完全随机，代表最坏情况（无法聚类，压测规约逻辑）
fn bench_image_types(c: &mut Criterion) {
    let mut group = c.benchmark_group("图片类型");

    let images: &[(&str, DynamicImage)] = &[
        ("渐变", make_gradient(128)),
        ("均匀色块", make_uniform_blocks(128)),
        ("随机噪声", make_noise(128)),
    ];

    let cfg = Config::default().max_colors(8).sample_size(None);

    for (label, img) in images {
        for alg in [Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree] {
            group.bench_with_input(
                BenchmarkId::new(format!("{alg:?}"), *label),
                img,
                |b, img| {
                    b.iter(|| {
                        DominantColors::new(black_box(img.clone()))
                            .config(cfg.clone())
                            .extract(black_box(alg))
                            .unwrap()
                    })
                },
            );
        }
    }

    group.finish();
}

// ── 注册与入口 ─────────────────────────────────────────────────────────────────

criterion_group!(
    benches,
    bench_image_sizes,
    bench_color_counts,
    bench_sampling_overhead,
    bench_image_types,
);
criterion_main!(benches);
