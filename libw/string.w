sc.true

fn strlen(u8* s) -> u64 {
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
    return(u64 len);
}

fn strcmp(u8* s1, u8* s2) {
    u8* p1*i = s1;
    u8* p2*i = s2;
    u8 c1 = 1;
    u8 c2 = 1;
    u64 equal = 1;

    while (equal == 1) {
        c1 = p1;
        c2 = p2;
        if (c1 != c2) {
            equal = 0;
        } else {
            if (c1 == 0) {
                return(i64 0);
            }
            p1++;
            p2++;
        }
    }

    if (c1 < c2) {
        i64 res = 0 - 1;
        return(res);
    }
    i64 one = 1;
    return(one);
}

fn memcpy(u8* dest, u8* src, u64 n) {
    u64 i = 0;
    while (i < n) {
        u8* p_out*o = dest;
        u8* p_in*i = src;
        p_out = p_in;
        dest++;
        src++;
        i = i + 1;
    }
    return(dest);
}

fn memset(u8* s, u8 c, u64 n) {
    u64 i = 0;
    while (i < n) {
        u8* p_out*o = s;
        p_out = c;
        s++;
        i = i + 1;
    }
    return(s);
}

fn atoi(u8* s) {
    u8* p*i = s;
    u64 res = 0;
    u8 active_char = 1;
    while (active_char != 0) {
        active_char = p;
        if (active_char != 0) {
            if (active_char >= 48) {
                if (active_char <= 57) {
                    u64 digit = active_char - 48;
                    res = (res * 10) + digit;
                }
            }
            p++;
        }
    }
    return(u64 res);
}
