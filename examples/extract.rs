//! 示例程序：从图片文件中提取主色调，支持三种算法对比与终端色块预览。
//!
//! # 用法
//!
//! ```bash
//! # 使用默认参数（6 色，三种算法对比）
//! cargo run --example extract -- photo.jpg
//!
//! # 指定提取颜色数量
//! cargo run --example extract -- photo.jpg 8
//!
//! # 只运行指定算法：kmeans / mediancut / octree
//! cargo run --example extract -- photo.jpg 6 kmeans
//!
//! # 关闭终端色块预览（在不支持 ANSI 转义的环境中使用）
//! cargo run --example extract -- photo.jpg 6 all nocolor
//! ```
//!
//! # 输出示例
//!
//! ```text
//! 图片：photo.jpg（800×600，采样至 256×256）
//! ──────────────────────────────────────────
//! [K-Means 聚类]  耗时: 42.3ms
//!   1. ██ #F4A261  34.2%  ████████████████████
//!   2. ██ #264653  21.7%  █████████████
//!   3. ██ #2A9D8F  18.5%  ███████████
//!   ...
//! ```

use std::time::Instant;

use dominant_colors::{Algorithm, Color, Config, DominantColors};

// ── CLI 参数解析 ──────────────────────────────────────────────────────────────

/// 命令行参数。
struct Args {
    /// 图片路径。
    path: String,
    /// 提取的颜色数量。
    k: usize,
    /// 要运行的算法列表。
    algorithms: Vec<Algorithm>,
    /// 是否在终端中输出 ANSI 色块。
    ansi_color: bool,
}

impl Args {
    fn parse() -> Result<Self, String> {
        let raw: Vec<String> = std::env::args().skip(1).collect();

        if raw.is_empty() || raw.iter().any(|a| a == "-h" || a == "--help") {
            return Err(usage());
        }

        let path = raw[0].clone();

        let k = raw
            .get(1)
            .map(|s| {
                s.parse::<usize>()
                    .map_err(|_| format!("无效的颜色数量 '{s}'，应为正整数"))
            })
            .transpose()?
            .unwrap_or(6);

        if k == 0 {
            return Err("颜色数量不能为 0".into());
        }

        let alg_str = raw.get(2).map(String::as_str).unwrap_or("all");
        let algorithms = parse_algorithms(alg_str)?;

        let ansi_color = raw.get(3).map(String::as_str).unwrap_or("color") != "nocolor";

        Ok(Self { path, k, algorithms, ansi_color })
    }
}

fn usage() -> String {
    "用法: extract <图片路径> [颜色数量] [算法] [color|nocolor]\n\
     算法: kmeans | mediancut | octree | all（默认）\n\
     示例: extract photo.jpg 8 kmeans"
        .into()
}

fn parse_algorithms(s: &str) -> Result<Vec<Algorithm>, String> {
    match s.to_lowercase().as_str() {
        "all" => Ok(vec![Algorithm::KMeans, Algorithm::MedianCut, Algorithm::Octree]),
        "kmeans" => Ok(vec![Algorithm::KMeans]),
        "mediancut" | "median_cut" => Ok(vec![Algorithm::MedianCut]),
        "octree" => Ok(vec![Algorithm::Octree]),
        other => Err(format!(
            "未知算法 '{other}'，可选值：kmeans / mediancut / octree / all"
        )),
    }
}

// ── 主函数 ────────────────────────────────────────────────────────────────────

fn main() {
    let args = Args::parse().unwrap_or_else(|msg| {
        eprintln!("{msg}");
        std::process::exit(1);
    });

    // 打开图片
    let img = image::open(&args.path).unwrap_or_else(|e| {
        eprintln!("无法打开图片 '{}'：{e}", args.path);
        std::process::exit(1);
    });

    let (orig_w, orig_h) = (img.width(), img.height());
    let sample_size: Option<u32> = Some(256);
    let effective_w = sample_size
        .map(|s| orig_w.min(s))
        .unwrap_or(orig_w);
    let effective_h = sample_size
        .map(|s| orig_h.min(s))
        .unwrap_or(orig_h);

    println!();
    println!(
        "图片：{}（{}×{}{}）",
        args.path,
        orig_w,
        orig_h,
        if orig_w > effective_w || orig_h > effective_h {
            format!("，采样至 {}×{}", effective_w, effective_h)
        } else {
            String::new()
        }
    );
    println!("提取 {} 种主色调\n", args.k);

    let cfg = Config::default()
        .max_colors(args.k)
        .sample_size(sample_size);

    let separator = "─".repeat(50);

    // 逐算法运行并打印结果
    for alg in &args.algorithms {
        let t0 = Instant::now();
        let palette = DominantColors::new(img.clone())
            .config(cfg.clone())
            .extract(*alg)
            .unwrap_or_else(|e| {
                eprintln!("[{alg:?}] 提取失败：{e}");
                std::process::exit(1);
            });
        let elapsed = t0.elapsed();

        println!("{separator}");
        println!(
            "[{}]  耗时: {:.1}ms",
            algorithm_label(*alg),
            elapsed.as_secs_f64() * 1000.0
        );
        println!("{separator}");

        print_palette(&palette, args.ansi_color);
        println!();
    }

    // 多算法时输出差异摘要
    if args.algorithms.len() > 1 {
        print_comparison_summary(&img, &cfg, &args.algorithms, args.ansi_color);
    }
}

// ── 输出格式化 ────────────────────────────────────────────────────────────────

/// 打印单个调色板，可选 ANSI 色块。
fn print_palette(palette: &[Color], ansi_color: bool) {
    // 最长进度条宽度（对应 100%）
    const BAR_WIDTH: usize = 24;

    for (i, color) in palette.iter().enumerate() {
        let bar_len = (color.percentage * BAR_WIDTH as f32).round() as usize;
        let bar = "█".repeat(bar_len.max(1));

        if ansi_color {
            // 用 ANSI 真彩色在终端渲染色块（前景+背景同色实现实心方块）
            let swatch = format!(
                "\x1b[38;2;{r};{g};{b}m██\x1b[0m",
                r = color.r,
                g = color.g,
                b = color.b
            );
            println!(
                "  {:2}. {} #{:02X}{:02X}{:02X}  {:5.1}%  {}",
                i + 1,
                swatch,
                color.r,
                color.g,
                color.b,
                color.percentage * 100.0,
                bar,
            );
        } else {
            println!(
                "  {:2}. #{:02X}{:02X}{:02X}  {:5.1}%  {}",
                i + 1,
                color.r,
                color.g,
                color.b,
                color.percentage * 100.0,
                bar,
            );
        }
    }
}

/// 多算法对比摘要：打印三种算法首位颜色的差异。
fn print_comparison_summary(
    img: &image::DynamicImage,
    cfg: &Config,
    algorithms: &[Algorithm],
    ansi_color: bool,
) {
    println!("{}", "─".repeat(50));
    println!("【算法对比摘要】");
    println!("{}", "─".repeat(50));

    // 重新运行一次（只取首位颜色）
    for alg in algorithms {
        let palette = DominantColors::new(img.clone())
            .config(cfg.clone())
            .extract(*alg)
            .unwrap();

        let top = &palette[0];

        if ansi_color {
            let swatch = format!(
                "\x1b[38;2;{r};{g};{b}m██\x1b[0m",
                r = top.r,
                g = top.g,
                b = top.b
            );
            println!(
                "  {:<14} {} #{:02X}{:02X}{:02X}  {:.1}%  ({} 种颜色)",
                algorithm_label(*alg),
                swatch,
                top.r,
                top.g,
                top.b,
                top.percentage * 100.0,
                palette.len(),
            );
        } else {
            println!(
                "  {:<14} #{:02X}{:02X}{:02X}  {:.1}%  ({} 种颜色)",
                algorithm_label(*alg),
                top.r,
                top.g,
                top.b,
                top.percentage * 100.0,
                palette.len(),
            );
        }
    }
    println!();
}

/// 返回算法的中文显示名称。
fn algorithm_label(alg: Algorithm) -> &'static str {
    match alg {
        Algorithm::KMeans => "K-Means 聚类",
        Algorithm::MedianCut => "中位切分",
        Algorithm::Octree => "八叉树量化",
    }
}
