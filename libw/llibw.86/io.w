sc.true
#import <syscall>

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
            if (c == 10) {
                active = 0;
            } else {
                if (c == 0) {
                    active = 0;
                } else {
                    u8* p_out*o = buf;
                    p_out = c;
                    buf = buf + 1;
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
            p = p + 1;
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
        buf[30] = 48;
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

    u8* base*i = buf*adr;
    u8* result_str*i = base + pos;
    print_string(result_str);
}

fn print_signed_number(i64 num) {
    if (num < 0) {
        print_char(45);
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
            if (active_char == 37) {
                p = p + 1;
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
                            print_char(37);
                            print_char(active_char);
                        }
                    }
                }
            } else {
                print_char(active_char);
            }
            p = p + 1;
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

    if (c == 45) {
        sign = 0.0 - 1.0;
        p = p + 1;
        c = p;
    }

    while (c >= 48) {
        if (c <= 57) {
            res = res * 10.0 + (f64)(c - 48);
            p = p + 1;
            c = p;
        } else {
            c = 0;
        }
    }

    c = p;
    if (c == 46) {
        p = p + 1;
        c = p;
        f64 factor = 0.1;
        while (c >= 48) {
            if (c <= 57) {
                res = res + (f64)(c - 48) * factor;
                factor = factor * 0.1;
                p = p + 1;
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
    u8* p_buf = buf*adr;
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
    if (c == 45) {
        sign = 0 - 1;
        p = p + 1;
        c = p;
    }
    while (c >= 48) {
        if (c <= 57) {
            res = res * 10 + (c - 48);
            p = p + 1;
            c = p;
        } else {
            c = 0;
        }
    }
    return(res * sign);
}

fn snprintf(u8* buf, u64 size, u8* fmt, u64 arg1, u64 arg2, u64 arg3) -> i64 {
    if (size == 0) {
        return(0);
    }

    if (fmt == null) {
        buf[0] = 0;
        return(0);
    }

    u64 pos = 0;
    u64 arg_idx = 1;
    u8* p*i = fmt;
    u8 c = p;

    while (c != 0) {
        if (c == 37) {
            p++;
            c = p;

            if (c == 115) {
                u8* s = null;

                if (arg_idx == 1) {
                    s = arg1;
                }

                if (arg_idx == 2) {
                    s = arg2;
                }

                if (arg_idx == 3) {
                    s = arg3;
                }

                arg_idx = arg_idx + 1;

                if (s == null) {
                    u8* np*i = "(null)";
                    u8 nc = np;

                    while (nc != 0) {
                        if (pos + 1 < size) {
                            buf[pos] = nc;
                            pos = pos + 1;
                        }

                        np++;
                        nc = np;
                    }
                } else {
                    u8* sp*i = s;
                    u8 sc = sp;

                    while (sc != 0) {
                        if (pos + 1 < size) {
                            buf[pos] = sc;
                            pos = pos + 1;
                        }

                        sp++;
                        sc = sp;
                    }
                }
            } else {
                if (c == 100) {
                    i64 num = 0;

                    if (arg_idx == 1) {
                        num = arg1;
                    }

                    if (arg_idx == 2) {
                        num = arg2;
                    }

                    if (arg_idx == 3) {
                        num = arg3;
                    }

                    arg_idx = arg_idx + 1;

                    if (num < 0) {
                        if (pos + 1 < size) {
                            buf[pos] = 45;
                            pos = pos + 1;
                        }

                        num = 0 - num;
                    }

                    u8 tmp[32];
                    u64 tpos = 31;
                    tmp[31] = 0;
                    u64 unum = num;

                    if (unum == 0) {
                        tmp[30] = 48;
                        tpos = 30;
                    } else {
                        while (unum > 0) {
                            tpos = tpos - 1;
                            tmp[tpos] = (unum % 10) + 48;
                            unum = unum / 10;
                        }
                    }

                    while (tmp[tpos] != 0) {
                        if (pos + 1 < size) {
                            buf[pos] = tmp[tpos];
                            pos = pos + 1;
                        }

                        tpos = tpos + 1;
                    }
                } else {
                    if (c == 118) {
                        u64 num = 0;

                        if (arg_idx == 1) {
                            num = arg1;
                        }

                        if (arg_idx == 2) {
                            num = arg2;
                        }

                        if (arg_idx == 3) {
                            num = arg3;
                        }

                        arg_idx = arg_idx + 1;

                        u8 tmp[32];
                        u64 tpos = 31;
                        tmp[31] = 0;

                        if (num == 0) {
                            tmp[30] = 48;
                            tpos = 30;
                        } else {
                            while (num > 0) {
                                tpos = tpos - 1;
                                tmp[tpos] = (num % 10) + 48;
                                num = num / 10;
                            }
                        }

                        while (tmp[tpos] != 0) {
                            if (pos + 1 < size) {
                                buf[pos] = tmp[tpos];
                                pos = pos + 1;
                            }

                            tpos = tpos + 1;
                        }
                    } else {
                        if (pos + 1 < size) {
                            buf[pos] = 37;
                            pos = pos + 1;
                        }

                        if (pos + 1 < size) {
                            buf[pos] = c;
                            pos = pos + 1;
                        }
                    }
                }
            }
        } else {
            if (pos + 1 < size) {
                buf[pos] = c;
                pos = pos + 1;
            }
        }

        p++;
        c = p;
    }

    buf[pos] = 0;
    return(pos);
}
