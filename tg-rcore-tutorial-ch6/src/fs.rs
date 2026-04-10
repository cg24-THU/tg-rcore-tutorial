//! 文件系统管理模块
//!
//! 本模块封装了 easy-fs 文件系统的初始化和操作接口。
//!
//! ## 核心组件
//!
//! - `FS`：全局文件系统实例（延迟初始化），基于 VirtIO 块设备
//! - `FileSystem`：实现 `FSManager` trait，提供文件的打开、查找、目录列表等操作
//! - `read_all()`：辅助函数，读取文件的全部内容到内存
//!
//! ## 与第五章的区别
//!
//! 第五章的程序通过 `APPS` 内存表加载，而本章通过文件系统从磁盘读取。
//! `exec` 系统调用的实现从 `APPS.get(name)` 变为 `FS.open(name) + read_all()`。
//!
//! 教程阅读建议：
//!
//! - 先看 `FS` 的初始化：理解块设备与文件系统是如何绑定的；
//! - 再看 `open`：理解 CREATE/TRUNC/RDONLY 等标志的行为；
//! - 最后看 `read_all`：把握“按块读取 -> 拼接 ELF 数据”的加载路径。

use crate::virtio_block::BLOCK_DEVICE;
use alloc::{
    collections::{BTreeMap, BTreeSet},
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use spin::{Lazy, Mutex};
use tg_easy_fs::{EasyFileSystem, FSManager, FileHandle, Inode, OpenFlags, UserBuffer};
use tg_syscall::StatMode;

/// 全局文件系统实例
///
/// 在首次访问时初始化：
/// 1. 通过 `BLOCK_DEVICE`（VirtIO 块设备）打开 easy-fs 文件系统
/// 2. 获取根目录 inode
pub static FS: Lazy<FileSystem> = Lazy::new(|| FileSystem {
    root: EasyFileSystem::root_inode(&EasyFileSystem::open(BLOCK_DEVICE.clone())),
});

static FILE_INDEX: Lazy<Mutex<FileIndex>> = Lazy::new(|| Mutex::new(FileIndex::new()));

/// 文件系统管理器
///
/// 封装 easy-fs 的根目录 inode，提供文件操作接口。
/// 当前仅支持**单级目录**（所有文件在根目录下）。
pub struct FileSystem {
    /// 根目录 inode
    root: Inode,
}

/// 内核侧打开文件包装，额外携带 `fstat/link/unlink` 所需的元数据。
#[derive(Clone)]
pub struct OpenedFile {
    /// 底层 easy-fs 文件句柄
    pub handle: FileHandle,
    /// 内核维护的文件元数据
    pub meta: Option<Arc<Mutex<FileMeta>>>,
}

impl OpenedFile {
    /// 创建一个普通文件包装。
    pub fn new(handle: FileHandle, meta: Arc<Mutex<FileMeta>>) -> Self {
        Self {
            handle,
            meta: Some(meta),
        }
    }

    /// 创建一个不绑定 inode 元数据的空文件包装，供标准输入输出使用。
    pub fn empty(read: bool, write: bool) -> Self {
        Self {
            handle: FileHandle::empty(read, write),
            meta: None,
        }
    }

    /// 是否可读。
    #[inline]
    pub fn readable(&self) -> bool {
        self.handle.readable()
    }

    /// 是否可写。
    #[inline]
    pub fn writable(&self) -> bool {
        self.handle.writable()
    }

    /// 读取数据。
    #[inline]
    pub fn read(&self, buf: UserBuffer) -> isize {
        self.handle.read(buf)
    }

    /// 写入数据。
    #[inline]
    pub fn write(&self, buf: UserBuffer) -> isize {
        self.handle.write(buf)
    }
}

/// 单个“逻辑文件”的元数据。
pub struct FileMeta {
    /// 逻辑 inode 号，由内核侧分配。
    pub ino: u64,
    /// 文件类型。
    pub mode: StatMode,
    /// 硬链接数。
    pub nlink: u32,
    /// 实际保存数据的 easy-fs 文件名。
    backing_path: String,
}

struct FileIndex {
    next_ino: u64,
    aliases: BTreeMap<String, Arc<Mutex<FileMeta>>>,
    hidden: BTreeSet<String>,
}

impl FileIndex {
    const fn new() -> Self {
        Self {
            next_ino: 1,
            aliases: BTreeMap::new(),
            hidden: BTreeSet::new(),
        }
    }

    fn alloc_meta(&mut self, path: &str) -> Arc<Mutex<FileMeta>> {
        let meta = Arc::new(Mutex::new(FileMeta {
            ino: self.next_ino,
            mode: StatMode::FILE,
            nlink: 1,
            backing_path: path.to_string(),
        }));
        self.next_ino += 1;
        self.aliases.insert(path.to_string(), meta.clone());
        self.hidden.remove(path);
        meta
    }

    fn visible_meta(&self, path: &str) -> Option<Arc<Mutex<FileMeta>>> {
        self.aliases.get(path).cloned()
    }

    fn path_hidden(&self, path: &str) -> bool {
        self.hidden.contains(path)
    }
}

impl FileSystem {
    fn ensure_meta_for_path(&self, path: &str) -> Option<Arc<Mutex<FileMeta>>> {
        let mut index = FILE_INDEX.lock();
        if let Some(meta) = index.visible_meta(path) {
            return Some(meta);
        }
        if index.path_hidden(path) || self.root.find(path).is_none() {
            return None;
        }
        Some(index.alloc_meta(path))
    }

    /// 打开内核侧文件包装。
    pub fn open_file(&self, path: &str, flags: OpenFlags) -> Option<Arc<OpenedFile>> {
        let (readable, writable) = flags.read_write();

        if flags.contains(OpenFlags::CREATE) {
            let meta = {
                let mut index = FILE_INDEX.lock();
                if let Some(meta) = index.visible_meta(path) {
                    meta
                } else {
                    if index.path_hidden(path) {
                        index.hidden.remove(path);
                    }
                    index.alloc_meta(path)
                }
            };

            let inode = if let Some(inode) = self.root.find(path) {
                inode
            } else {
                self.root.create(path)?
            };
            inode.clear();
            return Some(Arc::new(OpenedFile::new(
                FileHandle::new(readable, writable, inode),
                meta,
            )));
        }

        let meta = self.ensure_meta_for_path(path)?;
        let backing_path = {
            let meta = meta.lock();
            meta.backing_path.clone()
        };
        let inode = self.root.find(backing_path.as_str())?;
        if flags.contains(OpenFlags::TRUNC) {
            inode.clear();
        }
        Some(Arc::new(OpenedFile::new(
            FileHandle::new(readable, writable, inode),
            meta,
        )))
    }
}

impl FSManager for FileSystem {
    /// 打开文件
    ///
    /// 根据 `OpenFlags` 处理不同的打开模式：
    /// - `CREATE`：文件存在则清空，不存在则创建
    /// - `TRUNC`：清空文件内容
    /// - `RDONLY`/`WRONLY`/`RDWR`：设置读写权限
    fn open(&self, path: &str, flags: OpenFlags) -> Option<Arc<FileHandle>> {
        self.open_file(path, flags)
            .map(|opened| Arc::new(opened.handle.clone()))
    }

    /// 在根目录中查找文件
    fn find(&self, path: &str) -> Option<Arc<Inode>> {
        if let Some(meta) = FILE_INDEX.lock().visible_meta(path) {
            let backing_path = meta.lock().backing_path.clone();
            self.root.find(backing_path.as_str())
        } else if FILE_INDEX.lock().path_hidden(path) {
            None
        } else {
            self.root.find(path)
        }
    }

    /// 列出根目录下所有文件名
    fn readdir(&self, _path: &str) -> Option<alloc::vec::Vec<String>> {
        Some(self.root.readdir())
    }

    /// 创建硬链接（TODO 练习题）
    fn link(&self, src: &str, dst: &str) -> isize {
        if src == dst {
            return -1;
        }
        let mut index = FILE_INDEX.lock();
        if index.visible_meta(dst).is_some() || index.path_hidden(dst) {
            return -1;
        }
        let Some(meta) = (if let Some(meta) = index.visible_meta(src) {
            Some(meta)
        } else if self.root.find(src).is_some() {
            Some(index.alloc_meta(src))
        } else {
            None
        }) else {
            return -1;
        };
        meta.lock().nlink += 1;
        index.aliases.insert(dst.to_string(), meta);
        index.hidden.remove(dst);
        0
    }

    /// 删除硬链接（TODO 练习题）
    fn unlink(&self, path: &str) -> isize {
        let mut index = FILE_INDEX.lock();
        let Some(meta) = (if let Some(meta) = index.aliases.remove(path) {
            Some(meta)
        } else if self.root.find(path).is_some() && !index.path_hidden(path) {
            let meta = index.alloc_meta(path);
            index.aliases.remove(path);
            Some(meta)
        } else {
            None
        }) else {
            return -1;
        };
        index.hidden.insert(path.to_string());
        let mut meta = meta.lock();
        if meta.nlink > 0 {
            meta.nlink -= 1;
        }
        0
    }
}

/// 读取文件的全部内容到 Vec<u8>
///
/// 通过文件句柄的 inode，从偏移 0 开始逐块读取，
/// 直到读取长度为 0（表示文件结束）。
pub fn read_all(fd: Arc<OpenedFile>) -> Vec<u8> {
    let mut offset = 0usize;
    let mut buffer = [0u8; 512];
    let mut v: Vec<u8> = Vec::new();
    if let Some(inode) = &fd.handle.inode {
        loop {
            let len = inode.read_at(offset, &mut buffer);
            if len == 0 {
                break;
            }
            offset += len;
            v.extend_from_slice(&buffer[..len]);
        }
    }
    v
}
