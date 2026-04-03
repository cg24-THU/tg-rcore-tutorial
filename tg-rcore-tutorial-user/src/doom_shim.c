#include <errno.h>
#include <fcntl.h>
#include <stddef.h>
#include <stdlib.h>
#include <stdio.h>
#include <string.h>
#include <sys/stat.h>
#include <sys/time.h>
#include <sys/types.h>

#include "doomgeneric.h"

struct _reent;

extern long __tg_open(const char *path, int flags);
extern long __tg_close(int fd);
extern long __tg_read(int fd, void *buf, size_t len);
extern long __tg_write(int fd, const void *buf, size_t len);
extern long __tg_lseek(int fd, long offset, int whence);
extern long __tg_sbrk(int increment);
extern long __tg_gettimeofday(struct timeval *tv);
extern long __tg_getpid(void);
extern void __tg_exit(int status);

static int syscall_status(long ret)
{
    if (ret < 0)
    {
        errno = (int)-ret;
        return -1;
    }
    return (int)ret;
}

static long syscall_status_long(long ret)
{
    if (ret < 0)
    {
        errno = (int)-ret;
        return -1;
    }
    return ret;
}

int open(const char *path, int flags, ...)
{
    return syscall_status(__tg_open(path, flags));
}

int _open(const char *path, int flags, ...)
{
    return open(path, flags);
}

int _open_r(struct _reent *r, const char *path, int flags, int mode)
{
    (void)r;
    (void)mode;
    return open(path, flags);
}

int close(int fd)
{
    return syscall_status(__tg_close(fd));
}

int _close(int fd)
{
    return close(fd);
}

int _close_r(struct _reent *r, int fd)
{
    (void)r;
    return close(fd);
}

ssize_t read(int fd, void *buf, size_t len)
{
    return syscall_status_long(__tg_read(fd, buf, len));
}

ssize_t _read(int fd, void *buf, size_t len)
{
    return read(fd, buf, len);
}

ssize_t _read_r(struct _reent *r, int fd, void *buf, size_t len)
{
    (void)r;
    return read(fd, buf, len);
}

ssize_t write(int fd, const void *buf, size_t len)
{
    return syscall_status_long(__tg_write(fd, buf, len));
}

ssize_t _write(int fd, const void *buf, size_t len)
{
    return write(fd, buf, len);
}

ssize_t _write_r(struct _reent *r, int fd, const void *buf, size_t len)
{
    (void)r;
    return write(fd, buf, len);
}

off_t lseek(int fd, off_t offset, int whence)
{
    return (off_t)syscall_status_long(__tg_lseek(fd, offset, whence));
}

off_t _lseek(int fd, off_t offset, int whence)
{
    return lseek(fd, offset, whence);
}

off_t _lseek_r(struct _reent *r, int fd, off_t offset, int whence)
{
    (void)r;
    return lseek(fd, offset, whence);
}

int _fstat(int fd, struct stat *st)
{
    if (st == NULL)
    {
        errno = EINVAL;
        return -1;
    }

    memset(st, 0, sizeof(*st));
    st->st_mode = fd <= 2 ? S_IFCHR : S_IFREG;
    st->st_nlink = 1;
    return 0;
}

int _fstat_r(struct _reent *r, int fd, struct stat *st)
{
    (void)r;
    return _fstat(fd, st);
}

int stat(const char *path, struct stat *st)
{
    int fd = open(path, O_RDONLY);
    if (fd < 0)
    {
        return -1;
    }
    if (_fstat(fd, st) < 0)
    {
        close(fd);
        return -1;
    }
    close(fd);
    return 0;
}

int _stat(const char *path, struct stat *st)
{
    return stat(path, st);
}

int _stat_r(struct _reent *r, const char *path, struct stat *st)
{
    (void)r;
    return stat(path, st);
}

int _isatty(int fd)
{
    return fd <= 2;
}

int _isatty_r(struct _reent *r, int fd)
{
    (void)r;
    return _isatty(fd);
}

void *sbrk(ptrdiff_t increment)
{
    long ret = __tg_sbrk((int)increment);
    if (ret < 0)
    {
        errno = ENOMEM;
        return (void *)-1;
    }
    return (void *)ret;
}

void *_sbrk(ptrdiff_t increment)
{
    return sbrk(increment);
}

void *_sbrk_r(struct _reent *r, ptrdiff_t increment)
{
    (void)r;
    return sbrk(increment);
}

int _gettimeofday(struct timeval *tv, void *tz)
{
    (void)tz;
    return syscall_status(__tg_gettimeofday(tv));
}

int _gettimeofday_r(struct _reent *r, struct timeval *tv, void *tz)
{
    (void)r;
    return _gettimeofday(tv, tz);
}

int _getpid(void)
{
    return (int)__tg_getpid();
}

int _kill(int pid, int sig)
{
    (void)pid;
    (void)sig;
    errno = EINVAL;
    return -1;
}

int mkdir(const char *path, mode_t mode)
{
    (void)path;
    (void)mode;
    return 0;
}

int remove(const char *path)
{
    (void)path;
    return 0;
}

int rename(const char *oldpath, const char *newpath)
{
    (void)oldpath;
    (void)newpath;
    return 0;
}

int brk(void *addr)
{
    (void)addr;
    errno = ENOSYS;
    return -1;
}

void _exit(int status)
{
    __tg_exit(status);
    for (;;)
    {
    }
}

void _exit_r(struct _reent *r, int status)
{
    (void)r;
    _exit(status);
}

int main(void)
{
    static char arg0[] = "doom";
    static char arg1[] = "-iwad";
    static char arg2[] = "doom1.wad";
    static char *argv[] = {arg0, arg1, arg2, NULL};

    doomgeneric_Create(3, argv);

    for (;;)
    {
        doomgeneric_Tick();
    }

    return 0;
}
