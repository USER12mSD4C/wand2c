sc.true

#import <syscall>
#import <syscall_constants>
#import <syscall_structs>
#import <string>
#import <dir>

// Minimal UTMS7 directory iteration using getdents64.

sect.dirbuf
    u8 buf[4096];
    u64 pos = 0;
    u64 end = 0;
    i64 fd = -1;
EOS

export fn dir_open(u8* path) -> i64 {
    u64 fd = sys_open(path, O_RDONLY | O_DIRECTORY, 0);
    if (syscall_error(fd) == 1) {
        return(0 - (i64)(0 - fd));
    }
    return((i64)fd);
}

export fn dir_close(i64 fd) -> i64 {
    u64 res = sys_close((u64)fd);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return(0);
}

export fn dir_read(i64 fd, Dirent* out) -> i64 {
    if (fd < 0) { return(-1); }
    if (dirbuf:fd != fd) {
        dirbuf:fd = fd;
        dirbuf:pos = 0;
        dirbuf:end = 0;
    }

    if (dirbuf:pos >= dirbuf:end) {
        u64 n = sys_getdents64((u64)fd, dirbuf:buf*adr, 4096);
        if (syscall_error(n) == 1) {
            return(0 - (i64)(0 - n));
        }
        if (n == 0) { return(0); }
        dirbuf:pos = 0;
        dirbuf:end = n;
    }

    u8* p = dirbuf:buf*adr + dirbuf:pos;
    u64* p_ino*i = p;
    out->ino = p_ino;
    u16* p_reclen*i = p + 16;
    u16 reclen = p_reclen;
    u8* p_type*i = p + 18;
    out->type = p_type;
    strncpy(out->name*adr, p + 19, 255);
    out->name[255] = 0;
    dirbuf:pos = dirbuf:pos + (u64)reclen;
    return(1);
}
