//! WebAssembly 绑定层。
//!
//! 通过 [`wasm-bindgen`] 将核心算法暴露给 JavaScript，支持：
//! - 直接传入原始 RGB 像素数据（`Uint8Array`）
//! - 传入 `ImageData`（浏览器 Canvas API 的原生格式，RGBA）
//! - 以 JSON 格式返回调色板，便于 JS 消费
//!
//! # 编译方式
//!
//! ```bash
//! # 安装工具链（一次性）
//! cargo install wasm-pack
//! rustup target add wasm32-unknown-unknown
//!
//! # 编译为 ES Module（推荐，适配现代打包工具）
//! wasm-pack build --target bundler
//!
//! # 编译为纯 Web（无打包工具，直接 <script type="module">）
//! wasm-pack build --target web --out-dir pkg-web
//!
//! # 编译为 Node.js
//! wasm-pack build --target nodejs --out-dir pkg-node
//!
//! # 启用调试 feature（panic 转发到 console.error）
//! wasm-pack build --target web --features wasm-debug
//! ```
//!
//! # JavaScript 使用示例
//!
//! ```js
//! import init, { extractColors, extractColorsFromImageData, getAlgorithmInfo }
//!     from './pkg/dominant_colors.js';
//!
//! await init();
//!
//! // 方式一：传入 RGB 字节数组
//! const rgb = new Uint8Array([255,0,0, 0,0,255]);
//! const palette = extractColors(rgb, 2, 'mediancut');
//! // → [{"r":255,"g":0,"b":0,"hex":"#FF0000","percentage":0.5}, ...]
//!
//! // 方式二：从 Canvas ImageData 提取（自动跳过透明像素）
//! const ctx = canvas.getContext('2d');
//! const { data } = ctx.getImageData(0, 0, canvas.width, canvas.height);
//! const palette2 = extractColorsFromImageData(data, 6, 'kmeans');
//!
//! // 方式三：查询算法元信息
//! const algos = JSON.parse(getAlgorithmInfo());
//! ```

// wasm-bindgen 宏只在 wasm32 目标下生效；
// 在 native 目标下，#[wasm_bindgen] 是 no-op stub，代码仍可编译和测试。
#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

use crate::{algorithms, Algorithm, Config};

// ── panic hook ────────────────────────────────────────────────────────────────

/// 初始化 WASM 模块。
///
/// 在 `wasm-debug` feature 下将 panic 信息重定向到 `console.error`；
/// 其他情况下此函数为空操作。建议在任何其他调用前先调用此函数。
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(start))]
pub fn wasm_init() {
    #[cfg(all(target_arch = "wasm32", feature = "wasm-debug"))]
    console_error_panic_hook::set_once();
}

// ── 公开 API ──────────────────────────────────────────────────────────────────

/// 从 RGB 原始字节数组中提取主色调。
///
/// # 参数
///
/// - `rgb_data`：连续的 RGB 字节，长度必须为 3 的倍数（`[r0,g0,b0, r1,g1,b1, ...]`）
/// - `max_colors`：最多提取多少种颜色（1–64）
/// - `algorithm`：`"kmeans"` / `"mediancut"` / `"octree"`
///
/// # 返回值（JSON 字符串）
///
/// ```json
/// [{"r":255,"g":72,"b":0,"hex":"#FF4800","percentage":0.34}, ...]
/// ```
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = extractColors))]
pub fn extract_colors(
    rgb_data: &[u8],
    max_colors: usize,
    algorithm: &str,
) -> Result<String, String> {
    validate_max_colors(max_colors)?;

    if rgb_data.len() % 3 != 0 {
        return Err(format!(
            "rgb_data 的长度 ({}) 不是 3 的倍数",
            rgb_data.len()
        ));
    }

    let pixels: Vec<[u8; 3]> = rgb_data
        .chunks_exact(3)
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    run_extraction(&pixels, max_colors, algorithm)
}

/// 从 Canvas `ImageData`（RGBA 格式）中提取主色调。
///
/// 完全透明的像素（`alpha == 0`）会被自动跳过，避免透明背景污染调色板。
///
/// # 参数
///
/// - `rgba_data`：来自 `ctx.getImageData().data` 的字节数组，长度为 4 的倍数
/// - `max_colors`：最多提取多少种颜色（1–64）
/// - `algorithm`：`"kmeans"` / `"mediancut"` / `"octree"`
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = extractColorsFromImageData))]
pub fn extract_colors_from_image_data(
    rgba_data: &[u8],
    max_colors: usize,
    algorithm: &str,
) -> Result<String, String> {
    validate_max_colors(max_colors)?;

    if rgba_data.len() % 4 != 0 {
        return Err(format!(
            "rgba_data 的长度 ({}) 不是 4 的倍数",
            rgba_data.len()
        ));
    }

    // 跳过完全透明像素，防止透明背景干扰调色板
    let pixels: Vec<[u8; 3]> = rgba_data
        .chunks_exact(4)
        .filter(|c| c[3] > 0)
        .map(|c| [c[0], c[1], c[2]])
        .collect();

    if pixels.is_empty() {
        return Err("图片中没有不透明像素".into());
    }

    run_extraction(&pixels, max_colors, algorithm)
}

/// 返回三种算法的元信息（JSON）。
///
/// 可在 UI 中动态渲染算法选择器，无需硬编码算法列表。
///
/// # 返回值（JSON 字符串）
///
/// ```json
/// [
///   {"id":"kmeans","name":"K-Means 聚类","description":"...","is_deterministic":false,"speed_rank":3},
///   {"id":"mediancut","name":"中位切分","description":"...","is_deterministic":true,"speed_rank":1},
///   {"id":"octree","name":"八叉树量化","description":"...","is_deterministic":true,"speed_rank":2}
/// ]
/// ```
#[cfg_attr(target_arch = "wasm32", wasm_bindgen(js_name = getAlgorithmInfo))]
pub fn get_algorithm_info() -> String {
    use crate::algorithms::AlgorithmInfo;

    let entries: Vec<String> = [
        ("kmeans",    Algorithm::KMeans),
        ("mediancut", Algorithm::MedianCut),
        ("octree",    Algorithm::Octree),
    ]
    .iter()
    .map(|(id, alg)| {
        let info = AlgorithmInfo::of(*alg);
        format!(
            r#"{{"id":"{id}","name":"{name}","description":"{desc}","is_deterministic":{det},"speed_rank":{rank}}}"#,
            name = info.name,
            desc = info.description,
            det  = info.is_deterministic,
            rank = info.speed_rank,
        )
    })
    .collect();

    format!("[{}]", entries.join(","))
}

// ── 内部辅助 ──────────────────────────────────────────────────────────────────

/// 校验 `max_colors` 范围（1–64）。
fn validate_max_colors(max_colors: usize) -> Result<(), String> {
    if max_colors == 0 || max_colors > 64 {
        Err(format!(
            "max_colors 必须在 1–64 之间，实际收到 {max_colors}"
        ))
    } else {
        Ok(())
    }
}

/// 将算法名称字符串解析为 [`Algorithm`]，支持多种别名。
fn parse_algorithm(s: &str) -> Result<Algorithm, String> {
    match s.to_lowercase().as_str() {
        "kmeans" | "k-means" | "k_means" => Ok(Algorithm::KMeans),
        "mediancut" | "median_cut" | "median-cut" => Ok(Algorithm::MedianCut),
        "octree" => Ok(Algorithm::Octree),
        other => Err(format!(
            "未知算法 \"{other}\"，可选值：\"kmeans\" / \"mediancut\" / \"octree\""
        )),
    }
}

/// 执行提取并将结果序列化为 JSON 字符串。
///
/// 手动构建 JSON（避免引入 serde_json 增大 .wasm 体积）。
fn run_extraction(
    pixels: &[[u8; 3]],
    max_colors: usize,
    algorithm: &str,
) -> Result<String, String> {
    let alg = parse_algorithm(algorithm)?;

    let config = Config::default().max_colors(max_colors).sample_size(None); // WASM 场景下像素已由 JS 端控制大小

    let palette = algorithms::extract(pixels, alg, &config).map_err(|e| e.to_string())?;

    let entries: Vec<String> = palette
    .iter()
    .map(|c| { // 1. 让 Rust 自动推导 c 的类型，避免语法冲突
        format!(
            // 2. 使用 r##" 开头，确保内部的 "# 不会误触发字符串结束
            r##"{{"r":{},"g":{},"b":{},"hex":"#{:02X}{:02X}{:02X}","percentage":{:.4}}}"##,
            c.r, c.g, c.b, // 对应前三个 {}
            c.r, c.g, c.b, // 对应 hex 里的三个 :02X
            c.percentage   // 对应最后的 :.4
        )
    })
    .collect();

Ok(format!("[{}]", entries.join(",")))
}

// ── 单元测试（native + wasm32 双目标均可运行）────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn flatten_rgb(pixels: &[[u8; 3]]) -> Vec<u8> {
        pixels.iter().flat_map(|p| p.iter().copied()).collect()
    }

    fn flatten_rgba(pixels: &[[u8; 3]]) -> Vec<u8> {
        pixels
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255u8])
            .collect()
    }

    #[test]
    fn test_extract_colors_rgb_valid() {
        let mut pixels = vec![[255u8, 0, 0]; 50];
        pixels.extend(vec![[0u8, 0, 255]; 50]);
        let json = extract_colors(&flatten_rgb(&pixels), 2, "mediancut").unwrap();
        assert!(json.starts_with('[') && json.ends_with(']'));
        assert!(json.contains("\"hex\""));
    }

    #[test]
    fn test_extract_colors_bad_length() {
        let result = extract_colors(&[255u8, 0], 2, "mediancut");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("3 的倍数"));
    }

    #[test]
    fn test_extract_colors_bad_algorithm() {
        let data = flatten_rgb(&[[100u8, 100, 100]; 10]);
        let result = extract_colors(&data, 3, "unknown");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("未知算法"));
    }

    #[test]
    fn test_extract_colors_max_colors_zero() {
        let data = flatten_rgb(&[[100u8, 100, 100]; 10]);
        assert!(extract_colors(&data, 0, "octree").is_err());
    }

    #[test]
    fn test_extract_colors_max_colors_too_large() {
        let data = flatten_rgb(&[[100u8, 100, 100]; 10]);
        assert!(extract_colors(&data, 100, "octree").is_err());
    }

    #[test]
    fn test_extract_from_image_data_rgba() {
        let mut pixels = vec![[255u8, 0, 0]; 50];
        pixels.extend(vec![[0u8, 0, 255]; 50]);
        let json = extract_colors_from_image_data(&flatten_rgba(&pixels), 2, "mediancut").unwrap();
        assert!(json.contains("\"hex\""));
    }

    #[test]
    fn test_extract_from_image_data_skips_transparent() {
        // 红色不透明 + 蓝色全透明 → 结果中不应出现蓝色
        let mut data = vec![255u8, 0, 0, 255]; // 红色
        data.extend([0u8, 0, 255, 0]); // 蓝色，alpha=0
        let json = extract_colors_from_image_data(&data, 4, "octree").unwrap();
        assert!(!json.contains("\"r\":0,\"g\":0,\"b\":255"));
    }

    #[test]
    fn test_extract_from_image_data_all_transparent() {
        let data = vec![0u8, 0, 0, 0, 0, 0, 0, 0];
        assert!(extract_colors_from_image_data(&data, 2, "octree").is_err());
    }

    #[test]
    fn test_extract_from_image_data_bad_length() {
        assert!(extract_colors_from_image_data(&[255u8, 0, 0], 2, "kmeans").is_err());
    }

    #[test]
    fn test_get_algorithm_info_valid_json() {
        let json = get_algorithm_info();
        assert!(json.contains("\"id\":\"kmeans\""));
        assert!(json.contains("\"id\":\"mediancut\""));
        assert!(json.contains("\"id\":\"octree\""));
        assert!(json.contains("\"is_deterministic\""));
    }

    #[test]
    fn test_algorithm_name_aliases() {
        let data = flatten_rgb(&[[128u8, 128, 128]; 20]);
        for alias in ["kmeans", "k-means", "k_means"] {
            assert!(
                extract_colors(&data, 2, alias).is_ok(),
                "别名 '{alias}' 应被识别"
            );
        }
        for alias in ["mediancut", "median_cut", "median-cut"] {
            assert!(
                extract_colors(&data, 2, alias).is_ok(),
                "别名 '{alias}' 应被识别"
            );
        }
    }

    #[test]
    fn test_json_has_all_fields() {
        let data = flatten_rgb(&[[200u8, 100, 50]; 30]);
        let json = extract_colors(&data, 2, "octree").unwrap();
        for field in ["\"r\"", "\"g\"", "\"b\"", "\"hex\"", "\"percentage\""] {
            assert!(json.contains(field), "JSON 应包含字段 {field}");
        }
    }

    #[test]
    fn test_hex_format_uppercase() {
        let data = flatten_rgb(&[[171u8, 205, 239]; 10]); // 0xABCDEF
        let json = extract_colors(&data, 1, "octree").unwrap();
        assert!(json.contains("#ABCDEF"), "hex 应为大写，实际：{json}");
    }
}
