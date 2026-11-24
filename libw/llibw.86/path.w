sc.true
#import <syscall>
#import <string>

fn path_exists(u8* path) -> u64 {
    u8 statbuf[144];
    u64 ret = sys_stat(path, statbuf*adr);
    if (syscall_error(ret) == 1) {
        return(0);
    }
    return(1);
}

fn path_is_dir(u8* path) -> u64 {
    u8 statbuf[144];
    u64 ret = sys_stat(path, statbuf*adr);
    if (syscall_error(ret) == 1) {
        return(0);
    }
    u32* mode_ptr*i = statbuf*adr + 24;
    u32 mode = mode_ptr;
    if ((mode & 61440) == 16384) {
        return(1);
    }
    return(0);
}

fn path_join(u8* dest, u8* a, u8* b) {
    strcpy(dest, a);
    u64 len = strlen(dest);
    if (len > 0) {
        if (dest[len - 1] != 47) {
            dest[len] = 47;
            len = len + 1;
            dest[len] = 0;
        }
    }
    strcpy(dest + len, b);
}

fn path_dirname(u8* path, u8* dest) {
    u64 len = strlen(path);
    if (len == 0) {
        strcpy(dest, ".");
        return;
    }
    u64 end = len;
    while (end > 0) {
        if (path[end - 1] != 47) {
            break;
        }
        end = end - 1;
    }
    if (end == 0) {
        dest[0] = 47;
        dest[1] = 0;
        return;
    }
    u64 last = 0;
    u64 found = 0;
    u64 i = end;
    while (i > 0) {
        if (path[i - 1] == 47) {
            last = i - 1;
            found = 1;
            i = 0;
        } else {
            i = i - 1;
        }
    }
    if (found == 0) {
        strcpy(dest, ".");
        return;
    }
    u64 pend = last;
    while (pend > 0) {
        if (path[pend - 1] != 47) {
            break;
        }
        pend = pend - 1;
    }
    if (pend == 0) {
        dest[0] = 47;
        dest[1] = 0;
        return;
    }
    memcpy(dest, path, pend);
    dest[pend] = 0;
}

sect.pathbuf
    u8 base_buf[256];
EOS

fn path_basename(u8* path) -> u8* {
    u64 len = strlen(path);
    if (len == 0) {
        return(path);
    }
    u64 end = len;
    while (end > 0) {
        if (path[end - 1] != 47) {
            break;
        }
        end = end - 1;
    }
    if (end == 0) {
        return(path + len - 1);
    }
    u64 start = 0;
    u64 i = end;
    while (i > 0) {
        if (path[i - 1] == 47) {
            start = i;
            i = 0;
        } else {
            i = i - 1;
        }
    }
    if (end == len) {
        return(path + start);
    }
    u64 n = end - start;
    if (n >= 256) {
        n = 255;
    }
    memcpy(pathbuf:base_buf*adr, path + start, n);
    pathbuf:base_buf[n] = 0;
    return(pathbuf:base_buf*adr);
}

fn path_stat(u8* path, Stat* st) -> u64 {
    u64 r = sys_stat(path, (u8*)st);
    if (syscall_error(r) == 1) {
        return(0 - r);
    }
    return(0);
}

fn path_lstat(u8* path, Stat* st) -> u64 {
    u64 r = sys_lstat(path, (u8*)st);
    if (syscall_error(r) == 1) {
        return(0 - r);
    }
    return(0);
}

fn strmode(u32 mode, u8* buf) {
    u32 fmt = mode & S_IFMT;
    u8 t = 45;
    if (fmt == S_IFDIR) { t = 100; }
    else if (fmt == S_IFLNK) { t = 108; }
    else if (fmt == S_IFCHR) { t = 99; }
    else if (fmt == S_IFBLK) { t = 98; }
    else if (fmt == S_IFIFO) { t = 112; }
    else if (fmt == S_IFSOCK) { t = 115; }
    buf[0] = t;
    if (mode & S_IRUSR) { buf[1] = 114; } else { buf[1] = 45; }
    if (mode & S_IWUSR) { buf[2] = 119; } else { buf[2] = 45; }
    if (mode & S_ISUID) {
        if (mode & S_IXUSR) { buf[3] = 115; } else { buf[3] = 83; }
    } else {
        if (mode & S_IXUSR) { buf[3] = 120; } else { buf[3] = 45; }
    }
    if (mode & S_IRGRP) { buf[4] = 114; } else { buf[4] = 45; }
    if (mode & S_IWGRP) { buf[5] = 119; } else { buf[5] = 45; }
    if (mode & S_ISGID) {
        if (mode & S_IXGRP) { buf[6] = 115; } else { buf[6] = 83; }
    } else {
        if (mode & S_IXGRP) { buf[6] = 120; } else { buf[6] = 45; }
    }
    if (mode & S_IROTH) { buf[7] = 114; } else { buf[7] = 45; }
    if (mode & S_IWOTH) { buf[8] = 119; } else { buf[8] = 45; }
    if (mode & S_ISVTX) {
        if (mode & S_IXOTH) { buf[9] = 116; } else { buf[9] = 84; }
    } else {
        if (mode & S_IXOTH) { buf[9] = 120; } else { buf[9] = 45; }
    }
    buf[10] = 0;
}
