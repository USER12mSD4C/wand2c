sc.true
#import <syscall>
#import <string>

fn get_arg(u64 argv, u64 index) -> u8* {
    u64* table = argv;
    u64 ptr_val = table[index];
    return(ptr_val);
}

fn arg_equals(u64 argv, u64 index, u8* expected) -> u64 {
    u8* arg = get_arg(argv, index);
    i64 cmp = strcmp(arg, expected);
    if (cmp == 0) {
        return(1);
    }
    return(0);
}

fn find_arg(u64 argc, u64 argv, u8* name) -> u64 {
    for (u64 i = 1; i < argc; i = i + 1) {
        if (arg_equals(argv, i, name) == 1) {
            return(i);
        }
    }
    return(0);
}

fn get_arg_value(u64 argc, u64 argv, u8* name) -> u8* {
    u64 idx = find_arg(argc, argv, name);
    if (idx == 0) {
        return(null);
    }
    if (idx + 1 >= argc) {
        return(null);
    }
    return(get_arg(argv, idx + 1));
}
