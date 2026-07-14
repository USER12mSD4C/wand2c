sc.true

fn print_char(u8 c) {
    sys_write(1, c*adr, 1);
}

fn read_char() {
    u8 val = 0;
    sys_read(0, val*adr, 1);
    return(u8 val);
}

fn read_string(u8* buf, u64 max_size) {
    u64 i = 0;
    u64 limit = max_size - 1;
    u8 active = 1;
    while (i < limit) {
        if (active == 1) {
            u8 c = read_char();
            if (c == 10) { // '\n'
                active = 0;
            } else {
                if (c == 0) { // EOF
                    active = 0;
                } else {
                    u8* p_out*o = buf;
                    p_out = c;
                    buf++;
                    i = i + 1;
                }
            }
        } else {
            limit = 0;
        }
    }
    u8* p_end*o = buf;
    p_end = 0;
}

fn print_string(u8* s) {
    u64 len = 0;
    u8* p*i = s;
    u8 active_char = 1;
    while (active_char != 0) {
        active_char = p;
        if (active_char != 0) {
            p++;
            len = len + 1;
        }
    }
    sys_write(1, s, len);
}

fn print_number(u64 num) {
    u8[32] buf;
    u64 pos = 30;
    buf[31] = 0;

    u64 temp = num;
    if (temp == 0) {
        buf[30] = 48; // '0'
        pos = 30;
    } else {
        while (temp > 0) {
            u64 rem = temp % 10;
            u8 char_digit = rem + 48;
            buf[pos] = char_digit;
            pos = pos - 1;
            temp = temp / 10;
        }
        pos = pos + 1;
    }

    u8* result_str*i = buf[pos]*adr;
    print_string(result_str);
}

fn print_signed_number(i64 num) {
    if (num < 0) {
        print_char(45); // '-'
        u64 positive = 0 - num;
        print_number(positive);
    } else {
        print_number(num);
    }
}

fn printf(u8* format, u64 arg1, u64 arg2, u64 arg3) {
    u8* p*i = format;
    u8 active_char = 1;
    u64 arg_idx = 1;

    while (active_char != 0) {
        active_char = p;
        if (active_char != 0) {
            if (active_char == 37) { // '%'
                p++;
                active_char = p;

                if (active_char == 118) {
                    if (arg_idx == 1) { print_number(arg1); }
                    if (arg_idx == 2) { print_number(arg2); }
                    if (arg_idx == 3) { print_number(arg3); }
                    arg_idx = arg_idx + 1;
                } else {
                    if (active_char == 100) {
                        if (arg_idx == 1) { print_signed_number(arg1); }
                        if (arg_idx == 2) { print_signed_number(arg2); }
                        if (arg_idx == 3) { print_signed_number(arg3); }
                        arg_idx = arg_idx + 1;
                    } else {
                        if (active_char == 115) {
                            if (arg_idx == 1) { print_string(arg1); }
                            if (arg_idx == 2) { print_string(arg2); }
                            if (arg_idx == 3) { print_string(arg3); }
                            arg_idx = arg_idx + 1;
                        } else {
                            print_char(37); // '%'
                            print_char(active_char);
                        }
                    }
                }
            } else {
                print_char(active_char);
            }
            p++;
        }
    }
}

fn file_open(u8* path, u64 flags, u64 mode) -> i64 {
    i64 fd = sys_open(path, flags, mode);
    return(fd);
}

fn file_close(u64 fd) -> i64 {
    i64 res = sys_close(fd);
    return(res);
}

fn file_remove(u8* path) -> i64 {
    i64 res = sys_unlink(path);
    return(res);
}

fn file_read(u64 fd, u8* buf, u64 size) -> i64 {
    i64 res = sys_read(fd, buf, size);
    return(res);
}

fn file_write(u64 fd, u8* buf, u64 size) -> i64 {
    i64 res = sys_write(fd, buf, size);
    return(res);
}

fn parse_float(u8* s) -> f64 {
    f64 res = 0.0;
    f64 sign = 1.0;
    u8* p*i = s;
    u8 c = p;

    if (c == 45) { // '-'
        sign = 0.0 - 1.0;
        p++;
        c = p;
    }

    while (c >= 48) {
        if (c <= 57) {
            res = res * 10.0 + (f64)(c - 48);
            p++;
            c = p;
        } else {
            c = 0;
        }
    }

    c = p;
    if (c == 46) { // '.'
        p++;
        c = p;
        f64 factor = 0.1;
        while (c >= 48) {
            if (c <= 57) {
                res = res + (f64)(c - 48) * factor;
                factor = factor * 0.1;
                p++;
                c = p;
            } else {
                c = 0;
            }
        }
    }
    return(res * sign);
}

fn read_float() -> f64 {
    u8[64] buf;
    u8* p_buf = buf[0]*adr; // Обычный указатель без *i
    read_string(p_buf, 64);
    return(parse_float(p_buf));
}

fn read_integer() {
    u8 buf[64];
    u8* p_buf = buf*adr;
    read_string(p_buf, 64);

    i64 res = 0;
    i64 sign = 1;
    u8* p*i = p_buf;
    u8 c = p;
    if (c == 45) { // '-'
        sign = 0 - 1;
        p++;
        c = p;
    }
    while (c >= 48) {
        if (c <= 57) {
            res = res * 10 + (c - 48);
            p++;
            c = p;
        } else {
            c = 0;
        }
    }
    return(res * sign);
}
