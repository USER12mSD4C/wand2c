sc.true
#import <syscall>
#import <string>

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
    u8 buf[32];
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

    u8* base = buf*adr;
    u8* result_str = base + pos;
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
    u8 buf[64];
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

fn eprint(u8* s) {
    sys_write(2, s, strlen(s));
}

fn eprint_number(u64 n) {
    u8 buf[32];
    utoa(n, buf*adr);
    eprint(buf*adr);
}

fn die(u64 code, u8* msg) {
    eprint(msg);
    eprint("\n");
    sys_exit(code);
}

fn strerror(u64 err) -> u8* {
    match (err) {
        case 1 { return("Operation not permitted"); }
        case 2 { return("No such file or directory"); }
        case 3 { return("No such process"); }
        case 4 { return("Interrupted system call"); }
        case 5 { return("Input/output error"); }
        case 6 { return("No such device or address"); }
        case 7 { return("Argument list too long"); }
        case 8 { return("Exec format error"); }
        case 9 { return("Bad file descriptor"); }
        case 10 { return("No child processes"); }
        case 11 { return("Resource temporarily unavailable"); }
        case 12 { return("Cannot allocate memory"); }
        case 13 { return("Permission denied"); }
        case 14 { return("Bad address"); }
        case 15 { return("Block device required"); }
        case 16 { return("Device or resource busy"); }
        case 17 { return("File exists"); }
        case 18 { return("Invalid cross-device link"); }
        case 19 { return("No such device"); }
        case 20 { return("Not a directory"); }
        case 21 { return("Is a directory"); }
        case 22 { return("Invalid argument"); }
        case 23 { return("Too many open files in system"); }
        case 24 { return("Too many open files"); }
        case 25 { return("Inappropriate ioctl for device"); }
        case 26 { return("Text file busy"); }
        case 27 { return("File too large"); }
        case 28 { return("No space left on device"); }
        case 29 { return("Illegal seek"); }
        case 30 { return("Read-only file system"); }
        case 31 { return("Too many links"); }
        case 32 { return("Broken pipe"); }
        case 33 { return("Numerical argument out of domain"); }
        case 34 { return("Numerical result out of range"); }
        case 36 { return("File name too long"); }
        case 37 { return("No locks available"); }
        case 38 { return("Function not implemented"); }
        case 39 { return("Directory not empty"); }
        case 40 { return("Too many levels of symbolic links"); }
        case 61 { return("No data available"); }
        case 62 { return("Timer expired"); }
        case 75 { return("Value too large for defined data type"); }
        case 95 { return("Operation not supported"); }
        default { return("Unknown error"); }
    }
    return("Unknown error");
}

fn write_all(u64 fd, u8* buf, u64 len) -> u64 {
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

fn read_full(u64 fd, u8* buf, u64 len) -> i64 {
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
            return(done);
        }
        done = done + r;
    }
    return(done);
}

sect.iobuf
    u8 out[65536];
    u64 out_len = 0;
EOS

fn out_flush() {
    if (iobuf:out_len > 0) {
        write_all(1, iobuf:out*adr, iobuf:out_len);
        iobuf:out_len = 0;
    }
}

fn out_write(u8* s, u64 n) {
    if (n == 0) { return; }
    if (iobuf:out_len + n > 65536) {
        out_flush();
    }
    if (n >= 65536) {
        write_all(1, s, n);
        return;
    }
    memcpy(iobuf:out*adr + iobuf:out_len, s, n);
    iobuf:out_len = iobuf:out_len + n;
}

fn out_char(u8 c) {
    if (iobuf:out_len >= 65536) { out_flush(); }
    iobuf:out[iobuf:out_len] = c;
    iobuf:out_len = iobuf:out_len + 1;
}

fn out_str(u8* s) {
    out_write(s, strlen(s));
}

fn out_num(u64 n) {
    u8 buf[32];
    utoa(n, buf*adr);
    out_str(buf*adr);
}

fn out_i64(i64 n) {
    u8 buf[32];
    itoa(n, buf*adr);
    out_str(buf*adr);
}
