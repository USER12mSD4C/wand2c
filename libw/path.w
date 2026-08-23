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
    u32* mode_ptr = statbuf*adr + 24;
    u32 mode = mode_ptr;
    if ((mode & 61440) == 16384) {
        return(1);
    }
    return(0);
}

fn path_join(u8* dest, u8* a, u8* b) {
    strcpy(dest, a);
    u64 len = strlen(dest);
    u8* end = dest + len;
    u8* p_out*o = end;
    p_out = 47;
    end = end + 1;
    strcpy(end, b);
}

fn path_dirname(u8* path, u8* dest) {
    u64 len = strlen(path);
    u64 last_slash = 0;
    for (u64 i = 0; i < len; i = i + 1) {
        u8* p*i = path + i;
        u8 c = p;
        if (c == 47) {
            last_slash = i;
        }
    }
    if (last_slash == 0) {
        strcpy(dest, ".");
        return(0);
    }
    memcpy(dest, path, last_slash);
    u8* end = dest + last_slash;
    u8* p_out*o = end;
    p_out = 0;
    return(0);
}

fn path_basename(u8* path) -> u8* {
    u64 len = strlen(path);
    u64 last_slash = 0;
    for (u64 i = 0; i < len; i = i + 1) {
        u8* p*i = path + i;
        u8 c = p;
        if (c == 47) {
            last_slash = i;
        }
    }
    if (last_slash == 0) {
        return(path);
    }
    return(path + last_slash + 1);
}
