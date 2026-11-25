sc.true

#import <string>
#import <args>

export fn get_arg(u64 argv, u64 index) -> u8* {
    u64* table = (u64*)argv;
    u64 ptr_val = table[index];
    return((u8*)ptr_val);
}

export fn arg_equals(u64 argv, u64 index, u8* expected) -> u64 {
    u8* arg = get_arg(argv, index);
    if (arg == null) { return(0); }
    i64 cmp = strcmp(arg, expected);
    if (cmp == 0) {
        return(1);
    }
    return(0);
}

export fn find_arg(u64 argc, u64 argv, u8* name) -> u64 {
    u64 i = 1;
    while (i < argc) {
        if (arg_equals(argv, i, name) == 1) {
            return(i);
        }
        i = i + 1;
    }
    return(0);
}

export fn get_arg_value(u64 argc, u64 argv, u8* name) -> u8* {
    u64 idx = find_arg(argc, argv, name);
    if (idx == 0) {
        return(null);
    }
    if (idx + 1 >= argc) {
        return(null);
    }
    return(get_arg(argv, idx + 1));
}

export fn getenv(u64 envp, u8* name) -> u8* {
    if (envp == 0) { return(null); }
    u64 nlen = strlen(name);
    u64* table = (u64*)envp;
    u64 i = 0;
    while (table[i] != 0) {
        u8* entry = (u8*)table[i];
        if (strncmp(entry, name, nlen) == 0) {
            if (entry[nlen] == 61) {
                return(entry + nlen + 1);
            }
        }
        i = i + 1;
    }
    return(null);
}
