sc.true

#import <syscall>
#import <syscall_constants>
#import <syscall_structs>
#import <string>
#import <path>

// UTMS7 path helpers. Uses sys_stat/sys_newfstatat as implemented by UTMS7.

export fn path_exists(u8* path) -> u64 {
    stat st;
    u64 ret = sys_stat(path, st*adr);
    if (syscall_error(ret) == 1) {
        return(0);
    }
    return(1);
}

export fn path_is_dir(u8* path) -> u64 {
    stat st;
    u64 ret = sys_stat(path, st*adr);
    if (syscall_error(ret) == 1) {
        return(0);
    }
    if ((st.st_mode & S_IFMT) == S_IFDIR) {
        return(1);
    }
    return(0);
}

export fn path_is_file(u8* path) -> u64 {
    stat st;
    u64 ret = sys_stat(path, st*adr);
    if (syscall_error(ret) == 1) {
        return(0);
    }
    if ((st.st_mode & S_IFMT) == S_IFREG) {
        return(1);
    }
    return(0);
}

export fn path_join(u8* dest, u8* a, u8* b) {
    strcpy(dest, a);
    u64 len = strlen(dest);
    if (len > 0) {
        if (dest[len - 1] != 47) {
            dest[len] = 47;
            len = len + 1;
            dest[len] = 0;
        }
    }
    u8* src = b;
    if (b[0] == 47) {
        src = b + 1;
    }
    strcpy(dest + len, src);
}

export fn path_dirname(u8* path, u8* dest) {
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

export fn path_basename(u8* path) -> u8* {
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
    if (n >= 255) {
        n = 255;
    }
    memcpy(pathbuf:base_buf*adr, path + start, n);
    pathbuf:base_buf[n] = 0;
    return(pathbuf:base_buf*adr);
}

export fn path_stat(u8* path, stat* st) -> i64 {
    u64 r = sys_stat(path, (u8*)st);
    if (syscall_error(r) == 1) {
        return(0 - (i64)(0 - r));
    }
    return(0);
}
