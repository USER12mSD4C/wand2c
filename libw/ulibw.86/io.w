sc.true

#import <syscall>
#import <syscall_constants>
#import <string>

// Basic I/O for UTMS7. 

export fn print_char(u8 c) {
    u8 tmp[1];
    tmp[0] = c;
    sys_write(1, tmp*adr, 1);
}

export fn print_string(u8* s) {
    u64 len = strlen(s);
    sys_write(1, s, len);
}

export fn print_number(u64 num) {
    u8 buf[32];
    u64 pos = 31;
    buf[31] = 0;
    if (num == 0) {
        buf[30] = 48;
        print_string(buf*adr + 30);
        return;
    }
    while (num > 0) {
        pos = pos - 1;
        buf[pos] = (u8)((num % 10) + 48);
        num = num / 10;
    }
    print_string(buf*adr + pos);
}

export fn print_signed_number(i64 num) {
    if (num < 0) {
        print_char(45);
        u64 positive = 0 - num;
        print_number(positive);
    } else {
        print_number((u64)num);
    }
}

export fn eprint(u8* s) {
    sys_write(2, s, strlen(s));
}

export fn eprint_number(u64 n) {
    u8 buf[32];
    utoa(n, buf*adr);
    eprint(buf*adr);
}

export fn die(u64 code, u8* msg) {
    eprint(msg);
    eprint("\n");
    sys_exit(code);
}

export fn read_char() -> u8 {
    u8 val = 0;
    u64 r = sys_read(0, val*adr, 1);
    if (syscall_error(r) == 1) {
        return(0);
    }
    if (r == 0) {
        return(0);
    }
    return(val);
}

export fn read_line(u8* buf, u64 max_size) {
    if (max_size == 0) { return; }
    u64 i = 0;
    u64 limit = max_size - 1;
    while (i < limit) {
        u8 c = read_char();
        if (c == 10 || c == 0) {
            buf[i] = 0;
            return;
        }
        buf[i] = c;
        i = i + 1;
    }
    buf[i] = 0;
}

export fn write_all(u64 fd, u8* buf, u64 len) -> u64 {
    u64 done = 0;
    while (done < len) {
        u64 r = sys_write(fd, buf + done, len - done);
        if (syscall_error(r) == 1) {
            u64 err = 0 - r;
            if (err == EINTR) {
                continue;
            }
            return(err);
        }
        if (r == 0) {
            return(EIO);
        }
        done = done + r;
    }
    return(0);
}

export fn read_full(u64 fd, u8* buf, u64 len) -> i64 {
    u64 done = 0;
    while (done < len) {
        u64 r = sys_read(fd, buf + done, len - done);
        if (syscall_error(r) == 1) {
            u64 err = 0 - r;
            if (err == EINTR) {
                continue;
            }
            return(0 - err);
        }
        if (r == 0) {
            return((i64)done);
        }
        done = done + r;
    }
    return((i64)done);
}

export fn strerror(u64 err) -> u8* {
    if (err == EPERM) { return("Operation not permitted"); }
    if (err == ENOENT) { return("No such file or directory"); }
    if (err == ESRCH) { return("No such process"); }
    if (err == EINTR) { return("Interrupted system call"); }
    if (err == EIO) { return("Input/output error"); }
    if (err == EBADF) { return("Bad file descriptor"); }
    if (err == ECHILD) { return("No child processes"); }
    if (err == EAGAIN) { return("Resource temporarily unavailable"); }
    if (err == ENOMEM) { return("Cannot allocate memory"); }
    if (err == EACCES) { return("Permission denied"); }
    if (err == EFAULT) { return("Bad address"); }
    if (err == EBUSY) { return("Device or resource busy"); }
    if (err == EEXIST) { return("File exists"); }
    if (err == ENOTDIR) { return("Not a directory"); }
    if (err == EISDIR) { return("Is a directory"); }
    if (err == EINVAL) { return("Invalid argument"); }
    if (err == ENOSPC) { return("No space left on device"); }
    if (err == EPIPE) { return("Broken pipe"); }
    if (err == ENAMETOOLONG) { return("File name too long"); }
    if (err == ENOSYS) { return("Function not implemented"); }
    if (err == ENOTEMPTY) { return("Directory not empty"); }
    return("Unknown error");
}
