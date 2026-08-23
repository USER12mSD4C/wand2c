sc.true
#import <syscall>
#import <io>

sect.keys
    u64 arrow_up = 1000;
    u64 arrow_down = 1001;
    u64 arrow_right = 1002;
    u64 arrow_left = 1003;
    u64 key_delete = 1004;
    u64 page_up = 1005;
    u64 page_down = 1006;
    u64 key_home = 1010;
    u64 key_end = 1011;

    u64 key_f1 = 1101;
    u64 key_f2 = 1102;
    u64 key_f3 = 1103;
    u64 key_f4 = 1104;

    u64 key_esc = 27;
    u64 key_backspace = 127;
    u64 key_enter = 10;
    u64 key_tab = 9;
    u64 key_space = 32;

    u64 ctrl_a = 1;
    u64 ctrl_b = 2;
    u64 ctrl_c = 3;
    u64 ctrl_d = 4;
    u64 ctrl_e = 5;
    u64 ctrl_f = 6;
    u64 ctrl_g = 7;
    u64 ctrl_h = 8;
    u64 ctrl_i = 9;
    u64 ctrl_j = 10;
    u64 ctrl_k = 11;
    u64 ctrl_l = 12;
    u64 ctrl_m = 13;
    u64 ctrl_n = 14;
    u64 ctrl_o = 15;
    u64 ctrl_p = 16;
    u64 ctrl_q = 17;
    u64 ctrl_r = 18;
    u64 ctrl_s = 19;
    u64 ctrl_t = 20;
    u64 ctrl_u = 21;
    u64 ctrl_v = 22;
    u64 ctrl_w = 23;
    u64 ctrl_x = 24;
    u64 ctrl_y = 25;
    u64 ctrl_z = 26;
EOS

fn char_available() -> u64 {
    u32 count = 0;
    sys_ioctl(0, 21531, count*adr);
    return(u64 count);
}

fn read_key() -> u64 {
    u8 c = read_char();
    if (c == 27) {
        u64 count = char_available();
        if (count == 0) {
            return(u64 27);
        }

        u8 c1 = read_char();
        if (c1 == 91) { // '['
            u8 c2 = read_char();
            if (c2 == 65) { return(u64 1000); } // Arrow Up
            if (c2 == 66) { return(u64 1001); } // Arrow Down
            if (c2 == 67) { return(u64 1002); } // Arrow Right
            if (c2 == 68) { return(u64 1003); } // Arrow Left
            if (c2 == 72) { return(u64 1010); } // Home
            if (c2 == 70) { return(u64 1011); } // End

            if (c2 >= 48) {
                if (c2 <= 57) {
                    u8 c3 = read_char();
                    if (c3 == 126) { // '~'
                        if (c2 == 51) { return(u64 1004); } // Delete
                        if (c2 == 53) { return(u64 1005); } // Page Up
                        if (c2 == 54) { return(u64 1006); } // Page Down
                        if (c2 == 49) { return(u64 1010); } // Home
                        if (c2 == 52) { return(u64 1011); } // End
                    }
                }
            }
        }
        if (c1 == 79) { // 'O'
            u8 c2 = read_char();
            if (c2 == 80) { return(u64 1101); } // F1
            if (c2 == 81) { return(u64 1102); } // F2
            if (c2 == 82) { return(u64 1103); } // F3
            if (c2 == 83) { return(u64 1104); } // F4
        }

        if (c1 >= 32) {
            if (c1 <= 126) {
                u64 alt_code = 2000 + c1;
                return(alt_code);
            }
        }
        return(u64 27);
    }
    return(u64 c);
}
