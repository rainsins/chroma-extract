//! 主色调提取过程中可能出现的错误类型。

use thiserror::Error;

/// 主色调提取过程中所有可能的错误。
#[derive(Debug, Error, PartialEq)]
pub enum DominantColorError {
    /// 源图片不含任何像素（尺寸为零，或采样后为空）。
    #[error("图片为空，没有可处理的像素")]
    EmptyImage,

    /// 请求的颜色数量超过图片中唯一颜色的总数。
    ///
    /// 仅在 `max_colors > 唯一像素数` 且算法无法产生足够聚类时返回。
    #[error("请求 {requested} 种颜色，但图片中只有 {available} 种唯一颜色")]
    TooFewColors {
        /// 通过 [`Config::max_colors`] 请求的颜色数量。
        requested: usize,
        /// （采样后）图片中实际存在的唯一颜色数量。
        available: usize,
    },

    /// 算法内部不变式被违反。
    ///
    /// 正常使用下不应出现此错误；如果遇到，请提交 issue。
    #[error("算法内部错误：{message}")]
    InternalError {
        /// 不变式违反的可读描述。
        message: String,
    },
}

/// `std::result::Result<T, DominantColorError>` 的便捷别名。
pub type Result<T> = std::result::Result<T, DominantColorError>;

impl DominantColorError {
    /// 构造 [`InternalError`](DominantColorError::InternalError)。
    pub(crate) fn internal(msg: impl Into<String>) -> Self {
        Self::InternalError { message: msg.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_display_empty_image() {
        let e = DominantColorError::EmptyImage;
        assert!(e.to_string().contains("空"));
    }

    #[test]
    fn test_display_too_few_colors() {
        let e = DominantColorError::TooFewColors { requested: 10, available: 3 };
        let s = e.to_string();
        // 消息中应包含请求数和可用数
        assert!(s.contains("10") && s.contains("3"));
    }

    #[test]
    fn test_display_internal_error() {
        let e = DominantColorError::internal("发生了意料之外的错误");
        assert!(e.to_string().contains("发生了意料之外的错误"));
    }

    #[test]
    fn test_equality() {
        // 相同变体应相等，不同变体应不等
        assert_eq!(DominantColorError::EmptyImage, DominantColorError::EmptyImage);
        assert_ne!(
            DominantColorError::EmptyImage,
            DominantColorError::TooFewColors { requested: 1, available: 0 }
        );
    }
}
