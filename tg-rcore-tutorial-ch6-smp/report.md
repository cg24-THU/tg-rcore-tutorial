## 一、 实验要求分析

- 本次实验的主目标是补全 ch6 文件系统相关系统调用：`linkat`、`unlinkat`、`fstat`。
- 结合当前工程结构，不能只改内核 syscall 分发层，还需要把 easy-fs 的 inode 元数据和目录项操作补齐，否则内核层没有足够的信息完成硬链接计数与 inode 回收。
- 实际测试中，`test.sh exercise` 不仅覆盖 ch6 文件系统练习，还会顺带验证前面章节的 `spawn`、`mmap`、`munmap` 等接口，因此最终实现需要保证这些 syscall 也能通过当前仓库的练习测试。

## 二、 代码实现逻辑

本次修改分成两层：内核 syscall 层和 easy-fs 文件系统层。

### 1. 内核 syscall 层

修改文件：

- `src/main.rs`
- `src/fs.rs`

核心思路：

- 增加从用户地址空间读取字符串的辅助函数，统一处理 `open`、`exec`、`linkat`、`unlinkat`、`spawn` 的路径读取。
- 在 `fstat` 中根据文件描述符反查 inode，填写 `Stat { ino, mode, nlink }`。
- `linkat/unlinkat` 只做参数解析与错误返回，真正的目录项修改委托给 `FSManager`。
- 为了通过当前 `exercise` 测试，一并补上 `spawn/mmap/munmap` 的最小实现。

核心代码片段如下：

```rust
#[inline]
fn read_c_string(current: &ProcStruct, path: usize) -> Option<String> {
    current
        .address_space
        .translate::<u8>(VAddr::new(path), READABLE)
        .map(|ptr| {
            let mut string = String::new();
            let mut raw_ptr = ptr.as_ptr();
            loop {
                unsafe {
                    let ch = *raw_ptr;
                    if ch == 0 {
                        break;
                    }
                    string.push(ch as char);
                    raw_ptr = raw_ptr.add(1);
                }
            }
            string
        })
}
```

- 这段逻辑负责把用户态 `\0` 结尾路径翻译成内核里的 `String`，避免每个 syscall 重复写地址翻译代码。

```rust
fn linkat(
    &self,
    _caller: Caller,
    _olddirfd: i32,
    oldpath: usize,
    _newdirfd: i32,
    newpath: usize,
    _flags: u32,
) -> isize {
    let current = PROCESSOR.get_mut().current().unwrap();
    match (read_c_string(current, oldpath), read_c_string(current, newpath)) {
        (Some(oldpath), Some(newpath)) => FS.link(oldpath.as_str(), newpath.as_str()),
        _ => -1,
    }
}
```

- `linkat` 的内核层实现很薄，只负责把用户参数安全转成 Rust 字符串，再调用文件系统层。

```rust
fn fstat(&self, _caller: Caller, fd: usize, st: usize) -> isize {
    let current = PROCESSOR.get_mut().current().unwrap();
    if fd >= current.fd_table.len() {
        return -1;
    }
    let Some(file) = current.fd_table[fd].as_ref() else {
        return -1;
    };
    let file = file.lock();
    let Some(inode) = file.inode.as_ref() else {
        return -1;
    };
    let mut stat = Stat::new();
    stat.dev = 0;
    stat.ino = inode.inode_id() as u64;
    stat.mode = if inode.is_dir() { StatMode::DIR } else { StatMode::FILE };
    stat.nlink = inode.nlink();
    if let Some(mut ptr) = current.address_space.translate::<Stat>(VAddr::new(st), WRITEABLE) {
        *unsafe { ptr.as_mut() } = stat;
        0
    } else {
        -1
    }
}
```

- `fstat` 的关键点是：用户看到的 `ino` 和 `nlink` 不是内核拍脑袋填的，而是从 easy-fs inode 元数据中回读出来。

### 2. 文件系统层

修改文件：

- `src/fs.rs`
- `../tg-rcore-tutorial-easy-fs/src/layout.rs`
- `../tg-rcore-tutorial-easy-fs/src/efs.rs`
- `../tg-rcore-tutorial-easy-fs/src/vfs.rs`

核心思路：

- 在磁盘 inode 中增加 `nlink` 字段，初始化时设为 `1`。
- 在 easy-fs 中增加“根据 inode 位置反查 inode id”的能力，供 `fstat` 使用。
- 在目录 inode 上实现 `link/unlink`：
  - `link`：为目标 inode 增加一个新的目录项，并把 `nlink += 1`
  - `unlink`：删除目录项，若 `nlink > 1` 仅减计数；若为最后一个链接，则清空数据块并释放 inode bitmap/data bitmap

核心代码片段如下：

```rust
pub struct DiskInode {
    pub size: u32,
    pub direct: [u32; INODE_DIRECT_COUNT],
    pub indirect1: u32,
    pub indirect2: u32,
    pub nlink: u32,
    type_: DiskInodeType,
}

pub fn initialize(&mut self, type_: DiskInodeType) {
    self.size = 0;
    self.direct.iter_mut().for_each(|v| *v = 0);
    self.indirect1 = 0;
    self.indirect2 = 0;
    self.nlink = 1;
    self.type_ = type_;
}
```

- 这是硬链接语义成立的前提。没有 `nlink`，`fstat` 和 `unlink` 都无法正确实现。

```rust
pub fn link(&self, name: &str, inode_id: u32) -> isize {
    let mut fs = self.fs.lock();
    if self.read_disk_inode(|disk_inode| self.find_inode_id(name, disk_inode).is_some()) {
        return -1;
    }
    let (inode_block_id, inode_block_offset) = fs.get_disk_inode_pos(inode_id);
    get_block_cache(inode_block_id as usize, Arc::clone(&self.block_device))
        .lock()
        .modify(inode_block_offset, |disk_inode: &mut DiskInode| {
            disk_inode.nlink += 1;
        });
    self.modify_disk_inode(|dir_inode| {
        let file_count = (dir_inode.size as usize) / DIRENT_SZ;
        let new_size = (file_count + 1) * DIRENT_SZ;
        self.increase_size(new_size as u32, dir_inode, &mut fs);
        let dirent = DirEntry::new(name, inode_id);
        dir_inode.write_at(file_count * DIRENT_SZ, dirent.as_bytes(), &self.block_device);
    });
    block_cache_sync_all();
    0
}
```

- `link` 本质是“目录项复制”，不是复制文件内容；两个文件名最终指向同一个 inode。

```rust
pub fn unlink(&self, name: &str) -> isize {
    let mut fs = self.fs.lock();
    let Some(inode_id) = self.modify_disk_inode(|dir_inode| {
        let (idx, inode_id) = self.find_inode_id_with_index(name, dir_inode)?;
        let file_count = (dir_inode.size as usize) / DIRENT_SZ;
        if idx + 1 != file_count {
            let mut last_dirent = DirEntry::empty();
            assert_eq!(
                dir_inode.read_at((file_count - 1) * DIRENT_SZ, last_dirent.as_bytes_mut(), &self.block_device),
                DIRENT_SZ,
            );
            dir_inode.write_at(idx * DIRENT_SZ, last_dirent.as_bytes(), &self.block_device);
        }
        dir_inode.size -= DIRENT_SZ as u32;
        Some(inode_id)
    }) else {
        return -1;
    };

    let (inode_block_id, inode_block_offset) = fs.get_disk_inode_pos(inode_id);
    let reclaimed = get_block_cache(inode_block_id as usize, Arc::clone(&self.block_device))
        .lock()
        .modify(inode_block_offset, |disk_inode: &mut DiskInode| {
            if disk_inode.nlink > 1 {
                disk_inode.nlink -= 1;
                None
            } else {
                disk_inode.nlink = 0;
                Some(disk_inode.clear_size(&self.block_device))
            }
        });
    if let Some(data_blocks) = reclaimed {
        for block_id in data_blocks {
            fs.dealloc_data(block_id);
        }
        fs.dealloc_inode(inode_id);
    }
    block_cache_sync_all();
    0
}
```

- `unlink` 先删目录项，再根据 `nlink` 判断是否真正回收 inode 和数据块。
- 为了让目录项删除是 O(1) 风格实现，这里采用“用最后一个目录项覆盖被删除项，再缩小目录大小”的方式，不保留目录项顺序。

### 3. 最终验证命令

本次编译和测试均按要求在 Docker 容器中完成，最终通过的关键命令为：

```bash
docker exec -it rcore-container sh -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch6 && TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user cargo build --features exercise'
```

```bash
docker exec -it rcore-container bash -lc 'cd /tmp/tg-rcore-tutorial/tg-rcore-tutorial-ch6 && TG_USER_DIR=/tmp/tg-rcore-tutorial/tg-rcore-tutorial-user bash ./test.sh exercise'
```

- 最终 `tg-rcore-tutorial-checker` 输出为 `Test PASSED: 33/33`。

## 三、 遇到的问题与 Debug 记录（核心重点）

### 1. `docker exec -it` 首次执行时直接报 TTY 错误

- **Bug 描述**：第一次在容器里运行构建命令时，直接报错 `the input device is not a TTY`。
- **原因排查**：
  - 这不是 Rust 编译错误，而是 Docker 交互模式的要求。
  - 由于本次实验强约束要求命令必须写成 `docker exec -it rcore-container ...`，因此执行端也必须真的分配伪终端。
- **解决过程**：
  - 确认问题出在命令执行方式，而不是仓库代码。
  - 改成带真实 TTY 的容器会话后重新执行。
  - 之后所有 `docker exec -it ...` 的构建和测试命令都按交互终端方式运行。

### 2. 测试脚本直接执行失败

- **Bug 描述**：第一次在容器里运行 `./test.sh exercise`，报错 `Permission denied`。
- **原因排查**：脚本文件本身没有可执行权限，直接依赖 `./test.sh` 运行会失败。
- **解决过程**：
  - 先确认不是路径问题，而是执行位问题。
  - 不改脚本权限，直接改为显式调用解释器执行。
  - 后续统一使用 `bash ./test.sh exercise` 运行测试。

### 3. 用 `sh` 执行脚本再次失败

- **Bug 描述**：改成 `sh ./test.sh exercise` 后，报错 `set: Illegal option -o pipefail`。
- **原因排查**：`test.sh` 的 shebang 是 `#!/bin/bash`，并且脚本显式使用了 `set -o pipefail`，这是 `bash` 语义，不保证被 `/bin/sh` 支持。
- **解决过程**：
  - 读取 `test.sh`，确认第 1 行 shebang 和第 33 行 `set -o pipefail`。
  - 将测试命令固定为 `bash ./test.sh exercise`。
  - 之后脚本流程可以正常进入 `cargo run --features exercise | tg-rcore-tutorial-checker`。

### 4. 代码已经改了，但容器里运行的仍是旧实现

- **Bug 描述**：第一次完整跑通脚本后，运行日志里仍然出现 `spawn: parent pid = 1, not implemented`，说明内核执行的还是旧版 `spawn`。
- **原因排查**：
  - 宿主机上的源码已经修改，但容器测试目录 `/tmp/tg-rcore-tutorial/...` 里仍然保留旧副本。
  - 这次实验要求“只考虑宿主机内容，容器只用来测试”，因此宿主机是唯一真源，容器代码副本必须显式同步。
- **解决过程**：
  - 先在容器中 `grep` 对比 `src/main.rs`，确认 `spawn` 位置还是旧的 `not implemented`。
  - 放弃“直接假设容器会自动看到宿主机修改”的思路。
  - 最终使用 `docker cp` 将宿主机上已修改的 `src/main.rs`、`src/fs.rs`、`easy-fs/src/layout.rs`、`easy-fs/src/efs.rs`、`easy-fs/src/vfs.rs` 同步到容器内对应目录。
  - 同步后重新构建和测试，`spawn` 相关用例恢复正常。

### 5. 尝试用补丁方式同步容器代码失败

- **Bug 描述**：中间尝试通过 `git apply`/heredoc 的方式把宿主机 diff 打进容器，但补丁内容在 shell 转义过程中被破坏，导致应用失败。
- **原因排查**：
  - 这类长补丁命令经过多层 shell、TTY、引号转义后，极易出现内容截断或字符污染。
  - 对当前任务来说，容器不是开发主战场，只是测试环境，没有必要在容器里复杂打补丁。
- **解决过程**：
  - 终止继续折腾容器内补丁。
  - 改用更直接、可控的 `docker cp` 覆盖容器测试目录。
  - 这样既符合“宿主机为准，容器只负责测试”的原则，也减少了无意义的调试噪音。

### 6. 运行日志里出现 `StorePageFault/LoadPageFault`，最初看起来像新 Bug

- **Bug 描述**：重新跑 `exercise` 后，`ch4_mmap1`、`ch4_mmap2`、`sbrk` 都打印了 `unsupported trap: Exception(StorePageFault/LoadPageFault)`。
- **原因排查**：
  - 第一反应是 `mmap/munmap/sbrk` 实现有问题，但继续对照用户态测试代码后发现这几项本来就是“故意访问非法地址，期待被内核杀死”的负例测试。
  - `ch4_mmap1.rs` 会在只读页上写入；`ch4_mmap2.rs` 会在非法保护位映射后读内存；`sbrk.rs` 最后会故意写回已经释放的页。
  - `tg-rcore-tutorial-checker` 的判定标准也不是“不出现 page fault”，而是“不打印用户程序里的失败字符串”。
- **解决过程**：
  - 回读 `ch4_mmap1.rs`、`ch4_mmap2.rs`、`sbrk.rs`，确认这些 page fault 是预期行为。
  - 不再把这类 trap 误判成回归错误。
  - 继续看最终 checker 结果，确认 `33/33` 全部通过。

### 7. `linkat/unlinkat/fstat` 不能只在内核层补壳

- **Bug 描述**：在分析实现路径时发现，若只在 `src/main.rs` 增加 syscall 包装，`fstat` 无法返回真实 `ino/nlink`，`unlink` 也无法判断是否应该真正释放 inode。
- **原因排查**：
  - easy-fs 原始版本没有 `nlink` 字段，也没有“按 inode 位置反查 inode id”与“释放 inode bitmap 项”的接口。
  - 这说明问题根源不在 syscall 参数处理，而在文件系统元数据能力缺失。
- **解决过程**：
  - 在 `DiskInode` 中新增 `nlink`。
  - 在 `EasyFileSystem` 中新增 `dealloc_inode()` 和 `get_inode_id()`。
  - 在 `Inode` 层补齐 `inode_id()`、`nlink()`、`is_dir()`、`link()`、`unlink()`。
  - 回到内核层后，`fstat/linkat/unlinkat` 才能做成真正可用的实现，而不是伪实现。
