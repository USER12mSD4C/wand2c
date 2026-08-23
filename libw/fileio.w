sc.true

#import <syscall>
#import <fileio>

fn file_reader_init(FileReader* r, i64 fd) {
    r->fd = fd;
    r->pos = 0;
    r->len = 0;
}

fn file_reader_next_line(FileReader* r, u8* out, u64 max_size) -> i64 {
    u64 pos = 0;
    i64 running = 1;

    while (running == 1 && pos < max_size - 1) {
        if (r->pos >= r->len) {
            i64 n = sys_read(r->fd, r->buf, 4096);
            if (n <= 0) {
                running = 0;
            } else {
                r->pos = 0;
                r->len = (u64)n;
            }
        }

        if (running == 1) {
            u8 c = r->buf[r->pos];
            r->pos = r->pos + 1;

            if (c == 10) {
                running = 0;
            } else {
                out[pos] = c;
                pos = pos + 1;
            }
        }
    }

    out[pos] = 0;
    if (pos == 0 && running == 0) {
        return(-1);
    }
    return(pos);
}
