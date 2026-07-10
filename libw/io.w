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
            u8 is_format = 0;
            if (active_char == 37) { // '%'
                is_format = 1;
            }

            if (is_format == 1) {
                p++;
                active_char = p;
                u8 known = 0;
                if (active_char == 118) { // 'v'
                    known = 1;
                    if (arg_idx == 1) { print_number(arg1); }
                    if (arg_idx == 2) { print_number(arg2); }
                    if (arg_idx == 3) { print_number(arg3); }
                    arg_idx = arg_idx + 1;
                }
                if (active_char == 100) { // 'd'
                    known = 1;
                    if (arg_idx == 1) { print_signed_number(arg1); }
                    if (arg_idx == 2) { print_signed_number(arg2); }
                    if (arg_idx == 3) { print_signed_number(arg3); }
                    arg_idx = arg_idx + 1;
                }
                if (active_char == 115) { // 's'
                    known = 1;
                    if (arg_idx == 1) { print_string(arg1); }
                    if (arg_idx == 2) { print_string(arg2); }
                    if (arg_idx == 3) { print_string(arg3); }
                    arg_idx = arg_idx + 1;
                }
                if (known == 0) {
                    print_char(37); // '%'
                    print_char(active_char);
                }
            }

            if (is_format == 0) {
                print_char(active_char);
            }
            p++;
        }
    }
}
