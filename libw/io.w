sc.true

#import <string>

fn print_string(u8* s) {
    u64 len = strlen(s);
    ::nasm::{
        mov rax, 1
        mov rdi, 1
        mov rsi, [s]
        mov rdx, [len]
        syscall
    }
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
        u8[2] sign_buf;
        sign_buf[0] = 45; // '-'
        sign_buf[1] = 0;
        print_string(sign_buf*adr);

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

                if (active_char == 118) { // 'v'
                    if (arg_idx == 1) { print_number(arg1); }
                    if (arg_idx == 2) { print_number(arg2); }
                    if (arg_idx == 3) { print_number(arg3); }
                    arg_idx = arg_idx + 1;
                } else {
                    if (active_char == 100) { // 'd'
                        if (arg_idx == 1) { print_signed_number(arg1); }
                        if (arg_idx == 2) { print_signed_number(arg2); }
                        if (arg_idx == 3) { print_signed_number(arg3); }
                        arg_idx = arg_idx + 1;
                    } else {
                        if (active_char == 115) { // 's'
                            if (arg_idx == 1) { print_string(arg1); }
                            if (arg_idx == 2) { print_string(arg2); }
                            if (arg_idx == 3) { print_string(arg3); }
                            arg_idx = arg_idx + 1;
                        } else {
                            u8[2] temp_buf;
                            temp_buf[0] = 37;
                            temp_buf[1] = 0;
                            print_string(temp_buf*adr);

                            u8[2] temp_buf2;
                            temp_buf2[0] = active_char;
                            temp_buf2[1] = 0;
                            print_string(temp_buf2*adr);
                        }
                    }
                }
            } else {
                u8[2] temp_buf;
                temp_buf[0] = active_char;
                temp_buf[1] = 0;
                print_string(temp_buf*adr);
            }
            p++;
        }
    }
}
