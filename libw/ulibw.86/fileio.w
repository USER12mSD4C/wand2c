sc.true

#import <syscall>
#import <syscall_structs>
#import <syscall_constants>
#import <string>
#import <io>
#import <fileio>

export fn file_open(u8* path, u64 flags, u64 mode) -> i64 {
    u64 fd = sys_open(path, flags, mode);
    if (syscall_error(fd) == 1) {
        return(0 - (i64)(0 - fd));
    }
    return((i64)fd);
}

export fn file_close(i64 fd) -> i64 {
    u64 res = sys_close((u64)fd);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return(0);
}

export fn file_read(i64 fd, u8* buf, u64 size) -> i64 {
    u64 res = sys_read((u64)fd, buf, size);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return((i64)res);
}

export fn file_write(i64 fd, u8* buf, u64 size) -> i64 {
    u64 res = sys_write((u64)fd, buf, size);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return((i64)res);
}

export fn file_unlink(u8* path) -> i64 {
    u64 res = sys_unlink(path);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return(0);
}

export fn file_mkdir(u8* path, u64 mode) -> i64 {
    u64 res = sys_mkdir(path, mode);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return(0);
}

export fn file_rmdir(u8* path) -> i64 {
    u64 res = sys_rmdir(path);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return(0);
}

export fn file_stat(u8* path, u8* statbuf) -> i64 {
    u64 res = sys_stat(path, statbuf);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return(0);
}

export fn file_exists(u8* path) -> i64 {
    stat st;
    u64 res = sys_stat(path, st*adr);
    if (syscall_error(res) == 1) {
        return(0);
    }
    return(1);
}

export fn file_is_dir(u8* path) -> i64 {
    stat st;
    u64 res = sys_stat(path, st*adr);
    if (syscall_error(res) == 1) {
        return(0);
    }
    if ((st.st_mode & S_IFMT) == S_IFDIR) {
        return(1);
    }
    return(0);
}

export fn file_seek(i64 fd, u64 offset, u64 whence) -> i64 {
    u64 res = sys_lseek((u64)fd, offset, whence);
    if (syscall_error(res) == 1) {
        return(0 - (i64)(0 - res));
    }
    return((i64)res);
}

export fn file_reader_init(FileReader* r, i64 fd) {
    r->fd = fd;
    r->pos = 0;
    r->len = 0;
}

export fn file_reader_next_line(FileReader* r, u8* out, u64 max_size) -> i64 {
    u64 pos = 0;
    i64 running = 1;
    i64 saw_any_data = 0;

    while (running == 1 && pos < max_size - 1) {
        if (r->pos >= r->len) {
            u64 n = sys_read((u64)r->fd, r->buf*adr, 4096);
            if (syscall_error(n) == 1) {
                return(0 - (i64)(0 - n));
            }
            if (n == 0) {
                running = 0;
            } else {
                r->pos = 0;
                r->len = n;
                saw_any_data = 1;
            }
        }

        if (running == 1) {
            u8 c = r->buf[r->pos];
            r->pos = r->pos + 1;
            if (c == 10) {
                running = 0;
                saw_any_data = 1;
            } else {
                out[pos] = c;
                pos = pos + 1;
            }
        }
    }

    out[pos] = 0;
    if (pos == 0 && saw_any_data == 0) {
        return(-1);
    }
    return((i64)pos);
}

export fn file_writer_init(FileWriter* w, i64 fd) {
    w->fd = fd;
    w->len = 0;
    w->err = 0;
}

export fn file_writer_flush(FileWriter* w) -> u64 {
    if (w->len == 0) {
        return(0);
    }
    u64 err = write_all((u64)w->fd, w->buf*adr, w->len);
    w->len = 0;
    if (err != 0) {
        w->err = err;
    }
    return(err);
}

export fn file_writer_write(FileWriter* w, u8* data, u64 n) {
    if (w->err != 0) {
        return;
    }
    if (n >= 4096) {
        file_writer_flush(w);
        u64 err = write_all((u64)w->fd, data, n);
        if (err != 0) {
            w->err = err;
        }
        return;
    }
    if (w->len + n > 4096) {
        file_writer_flush(w);
    }
    memcpy(w->buf*adr + w->len, data, n);
    w->len = w->len + n;
}

export fn file_writer_str(FileWriter* w, u8* s) {
    file_writer_write(w, s, strlen(s));
}

export fn file_writer_char(FileWriter* w, u8 c) {
    if (w->err != 0) {
        return;
    }
    if (w->len >= 4096) {
        file_writer_flush(w);
    }
    w->buf[w->len] = c;
    w->len = w->len + 1;
}
