sc.true

#import <syscall>
#import <string>
#import <io>
#import <fileio>

fn file_reader_init(FileReader* r, i64 fd) {
    r->fd = fd;
    r->pos = 0;
    r->len = 0;
}

fn file_reader_next_line(FileReader* r, u8* out, u64 max_size) -> i64 {
    u64 pos = 0;
    i64 running = 1;
    i64 saw_any_data = 0;

    while (running == 1 && pos < max_size - 1) {
        if (r->pos >= r->len) {
            i64 n = sys_read(r->fd, r->buf, 4096);
            if (n <= 0) {
                running = 0;
            } else {
                r->pos = 0;
                r->len = (u64)n;
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
    return(pos);
}

fn file_reader_read(FileReader* r, u8* out, u64 n) -> i64 {
    u64 done = 0;
    while (done < n) {
        if (r->pos >= r->len) {
            i64 got = sys_read(r->fd, r->buf, 4096);
            if (got <= 0) {
                if (done > 0) {
                    return(done);
                }
                return(got);
            }
            r->pos = 0;
            r->len = (u64)got;
        }
        u64 avail = r->len - r->pos;
        u64 take = n - done;
        if (take > avail) {
            take = avail;
        }
        memcpy(out + done, r->buf + r->pos, take);
        r->pos = r->pos + take;
        done = done + take;
    }
    return(done);
}

fn file_writer_init(FileWriter* w, i64 fd) {
    w->fd = fd;
    w->len = 0;
    w->err = 0;
}

fn file_writer_flush(FileWriter* w) -> u64 {
    if (w->len == 0) {
        return(0);
    }
    u64 err = write_all((u64)w->fd, w->buf, w->len);
    w->len = 0;
    if (err != 0) {
        w->err = err;
    }
    return(err);
}

fn file_writer_write(FileWriter* w, u8* data, u64 n) {
    if (w->err != 0) {
        return;
    }
    if (n >= 16384) {
        file_writer_flush(w);
        u64 err = write_all((u64)w->fd, data, n);
        if (err != 0) {
            w->err = err;
        }
        return;
    }
    if (w->len + n > 16384) {
        file_writer_flush(w);
    }
    memcpy(w->buf + w->len, data, n);
    w->len = w->len + n;
}

fn file_writer_str(FileWriter* w, u8* s) {
    file_writer_write(w, s, strlen(s));
}

fn file_writer_char(FileWriter* w, u8 c) {
    if (w->err != 0) {
        return;
    }
    if (w->len >= 16384) {
        file_writer_flush(w);
    }
    w->buf[w->len] = c;
    w->len = w->len + 1;
}

fn file_writer_num(FileWriter* w, u64 n) {
    u8 buf[32];
    utoa(n, buf*adr);
    file_writer_str(w, buf*adr);
}

fn file_writer_signed(FileWriter* w, i64 n) {
    u8 buf[32];
    itoa(n, buf*adr);
    file_writer_str(w, buf*adr);
}
