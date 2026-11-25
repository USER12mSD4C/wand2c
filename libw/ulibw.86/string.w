sc.true

// UTMS7 string/memory helpers. Adapted from llibw.86 style. 

export fn strlen(u8* s) -> u64 {
    u64 len = 0;
    u8* p = s;
    while (p != null) {
        u8 c = p[0];
        if (c == 0) {
            return(len);
        }
        len = len + 1;
        p = p + 1;
    }
    return(len);
}

export fn strcmp(u8* s1, u8* s2) -> i64 {
    u64 i = 0;
    while (1) {
        u8 c1 = s1[i];
        u8 c2 = s2[i];
        if (c1 != c2) {
            if (c1 < c2) {
                return(-1);
            }
            return(1);
        }
        if (c1 == 0) {
            return(0);
        }
        i = i + 1;
    }
    return(0);
}

export fn strncmp(u8* s1, u8* s2, u64 n) -> i64 {
    u64 i = 0;
    while (i < n) {
        u8 c1 = s1[i];
        u8 c2 = s2[i];
        if (c1 != c2) {
            if (c1 < c2) {
                return(-1);
            }
            return(1);
        }
        if (c1 == 0) {
            return(0);
        }
        i = i + 1;
    }
    return(0);
}

export fn strcpy(u8* dest, u8* src) -> u8* {
    u8* original_dest = dest;
    u64 i = 0;
    while (1) {
        u8 c = src[i];
        dest[i] = c;
        if (c == 0) {
            return(original_dest);
        }
        i = i + 1;
    }
    return(original_dest);
}

export fn strncpy(u8* dest, u8* src, u64 n) -> u8* {
    u64 i = 0;
    while (i < n) {
        u8 c = src[i];
        dest[i] = c;
        if (c == 0) {
            u64 j = i + 1;
            while (j < n) {
                dest[j] = 0;
                j = j + 1;
            }
            return(dest);
        }
        i = i + 1;
    }
    return(dest);
}

export fn strcat(u8* dest, u8* src) -> u8* {
    u8* original_dest = dest;
    u64 dest_len = strlen(dest);
    u64 i = 0;
    while (1) {
        u8 c = src[i];
        dest[dest_len + i] = c;
        if (c == 0) {
            return(original_dest);
        }
        i = i + 1;
    }
    return(original_dest);
}

export fn strchr(u8* s, u8 c) -> u8* {
    u64 i = 0;
    while (1) {
        u8 ch = s[i];
        if (ch == c) {
            return(s + i);
        }
        if (ch == 0) {
            return(null);
        }
        i = i + 1;
    }
    return(null);
}

export fn strrchr(u8* s, u8 c) -> u8* {
    u8* last = null;
    u64 i = 0;
    while (1) {
        u8 ch = s[i];
        if (ch == c) {
            last = s + i;
        }
        if (ch == 0) {
            return(last);
        }
        i = i + 1;
    }
    return(last);
}

export fn strstr(u8* haystack, u8* needle) -> u8* {
    u64 nlen = strlen(needle);
    if (nlen == 0) {
        return(haystack);
    }
    u64 i = 0;
    while (haystack[i] != 0) {
        if (haystack[i] == needle[0]) {
            u64 j = 1;
            u64 matched = 1;
            while (j < nlen) {
                if (haystack[i + j] != needle[j]) {
                    matched = 0;
                    j = nlen;
                } else {
                    j = j + 1;
                }
            }
            if (matched == 1) {
                return(haystack + i);
            }
        }
        i = i + 1;
    }
    return(null);
}

export fn memcpy(u8* dest, u8* src, u64 n) -> u8* {
    u64 i = 0;
    while (i < n) {
        dest[i] = src[i];
        i = i + 1;
    }
    return(dest);
}

export fn memmove(u8* dest, u8* src, u64 n) -> u8* {
    u64 d = (u64)dest;
    u64 s = (u64)src;
    if (d < s) {
        return(memcpy(dest, src, n));
    }
    if (d > s) {
        u64 i = n;
        while (i > 0) {
            i = i - 1;
            dest[i] = src[i];
        }
    }
    return(dest);
}

export fn memset(u8* s, u8 c, u64 n) -> u8* {
    u64 i = 0;
    while (i < n) {
        s[i] = c;
        i = i + 1;
    }
    return(s);
}

export fn memcmp(u8* s1, u8* s2, u64 n) -> i64 {
    u64 i = 0;
    while (i < n) {
        u8 c1 = s1[i];
        u8 c2 = s2[i];
        if (c1 != c2) {
            if (c1 < c2) {
                return(-1);
            }
            return(1);
        }
        i = i + 1;
    }
    return(0);
}

export fn atoi(u8* s) -> u64 {
    u64 i = 0;
    while (s[i] == 32 || s[i] == 9 || s[i] == 10 || s[i] == 13) {
        i = i + 1;
    }
    u64 res = 0;
    while (1) {
        u8 c = s[i];
        if (c < 48 || c > 57) {
            return(res);
        }
        u64 digit = (u64)(c - 48);
        res = res * 10 + digit;
        i = i + 1;
    }
    return(res);
}

export fn itoa(i64 num, u8* buf) -> u8* {
    u8* original_buf = buf;
    if (num == 0) {
        buf[0] = 48;
        buf[1] = 0;
        return(original_buf);
    }
    u64 is_negative = 0;
    u64 temp_num = 0;
    if (num < 0) {
        is_negative = 1;
        temp_num = (u64)(0 - num);
    } else {
        temp_num = (u64)num;
    }
    u8 rev_buf[32];
    u64 pos = 0;
    while (temp_num > 0) {
        u64 rem = temp_num % 10;
        rev_buf[pos] = (u8)(rem + 48);
        pos = pos + 1;
        temp_num = temp_num / 10;
    }
    if (is_negative == 1) {
        buf[0] = 45;
        buf = buf + 1;
    }
    while (pos > 0) {
        pos = pos - 1;
        buf[0] = rev_buf[pos];
        buf = buf + 1;
    }
    buf[0] = 0;
    return(original_buf);
}

export fn utoa(u64 num, u8* buf) -> u8* {
    u8* original_buf = buf;
    if (num == 0) {
        buf[0] = 48;
        buf[1] = 0;
        return(original_buf);
    }
    u8 rev_buf[32];
    u64 pos = 0;
    while (num > 0) {
        u64 rem = num % 10;
        rev_buf[pos] = (u8)(rem + 48);
        pos = pos + 1;
        num = num / 10;
    }
    while (pos > 0) {
        pos = pos - 1;
        buf[0] = rev_buf[pos];
        buf = buf + 1;
    }
    buf[0] = 0;
    return(original_buf);
}
