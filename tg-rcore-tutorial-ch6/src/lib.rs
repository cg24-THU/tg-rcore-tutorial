#![cfg_attr(not(test), no_std)]

//! `cg-tg-rcore-tutorial-ch6` 是一个面向学习者的 rCore Tutorial 第六章实验 crate。
//!
//! 它整理了一个可运行、可复现、可发布的 RISC-V 教学内核，重点覆盖：
//!
//! - VirtIO block 设备接入
//! - easy-fs 文件系统
//! - 进程文件描述符表
//! - `open/read/write/close`
//! - `linkat/unlinkat/fstat`
//! - 配套练习、测试脚本与调试报告
//!
//! 推荐把它当成“学习型 crate”而不是“通用库”来使用：
//!
//! 1. 先阅读 `README.md` 掌握运行方式和学习目标；
//! 2. 再看 `src/main.rs`、`src/fs.rs`、`src/process.rs`；
//! 3. 最后对照 `report.md` 理解本次实验中的真实 Debug 过程。

/// 发布用的 crate 名称。
pub const CRATE_NAME: &str = env!("CARGO_PKG_NAME");

/// 发布用的版本号。
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// 课程作业对应的仓库 tag。
pub const RELEASE_TAG: &str = "v0.0.0";

/// 该 crate 在 crates.io 上声明的关键字。
pub const KEYWORDS: &[&str] = &["ai", "ai4ose", "kernel", "learning", "os"];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn release_metadata_is_consistent() {
        assert_eq!(CRATE_NAME, "cg-tg-rcore-tutorial-ch6");
        assert_eq!(VERSION, "0.0.0");
        assert_eq!(RELEASE_TAG, "v0.0.0");
        assert_eq!(KEYWORDS.len(), 5);
    }
}
